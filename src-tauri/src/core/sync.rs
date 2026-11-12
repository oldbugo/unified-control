//! Crash handling shared by the host and the dock worker.
//!
//! Release builds unwind on panic (Cargo.toml), so a fault in one job or thread can be recovered instead of ending the
//! whole program. This module holds the three pieces that make that safe:
//!
//! - **Poison-tolerant locks.** A panic while a lock is held "poisons" it; a plain `lock().unwrap()` would then panic on
//!   every later use and spread the fault. [`MutexExt`]/[`RwLockExt`] takes the lock anyway, clears the poison (so the recovery happens
//!   and is logged once) and returns the guard. Where a half-finished update could matter, the caller passes a fix-up
//!   with [`MutexExt::lock_or_reset`].
//! - **Guarded regions.** [`guarded`] runs a closure and turns a panic into `None`, for supervisor loops and reader
//!   threads that must not die silently.
//! - **The panic hook.** [`install_panic_hook`] writes a bounded, privacy-reduced line to `device.log` for every panic,
//!   so a crash leaves evidence. In the dock worker process an unguarded panic ends the worker (exit code 101), so the
//!   host's supervisor restarts it from a clean state, exactly as before the switch from `panic = "abort"`.
use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};

/// Panic and lock-recovery lines written to device.log per process; later ones are counted but not written.
const LOG_LIMIT: usize = 32;
/// Longest panic message kept in device.log.
const MESSAGE_LIMIT: usize = 160;

static LOGGED: AtomicUsize = AtomicUsize::new(0);
static EXIT_ON_UNGUARDED: AtomicBool = AtomicBool::new(false);
static HOOK: std::sync::Once = std::sync::Once::new();

thread_local! {
    static GUARD_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// Lock helpers for `Mutex<T>`.
pub trait MutexExt<T: ?Sized> {
    /// `lock()` that recovers a lock poisoned by an earlier panic (the poison is cleared and the recovery logged once).
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
    /// Like [`MutexExt::lock_or_recover`], but runs `reset` on the value when it was poisoned, for state whose
    /// half-finished update must not be used (for example, drop a cache so it is read again from disk).
    fn lock_or_reset(&self, reset: impl FnOnce(&mut T)) -> MutexGuard<'_, T>;
    /// `try_lock()` that recovers poison: `None` only when another thread holds the lock.
    fn try_lock_or_recover(&self) -> Option<MutexGuard<'_, T>>;
}

impl<T: ?Sized> MutexExt<T> for Mutex<T> {
    #[track_caller]
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        self.lock_or_reset(|_| {})
    }
    #[track_caller]
    fn lock_or_reset(&self, reset: impl FnOnce(&mut T)) -> MutexGuard<'_, T> {
        match self.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                reset(&mut guard);
                self.clear_poison();
                note_recovery(std::panic::Location::caller());
                guard
            }
        }
    }
    #[track_caller]
    fn try_lock_or_recover(&self) -> Option<MutexGuard<'_, T>> {
        match self.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(poisoned)) => {
                let guard = poisoned.into_inner();
                self.clear_poison();
                note_recovery(std::panic::Location::caller());
                Some(guard)
            }
        }
    }
}

/// Lock helpers for `RwLock<T>`.
pub trait RwLockExt<T: ?Sized> {
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T>;
    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T>;
    /// `try_read()` that recovers poison: `None` only when a writer holds the lock.
    fn try_read_or_recover(&self) -> Option<RwLockReadGuard<'_, T>>;
    /// `try_write()` that recovers poison: `None` only when the lock is held.
    fn try_write_or_recover(&self) -> Option<RwLockWriteGuard<'_, T>>;
}

impl<T: ?Sized> RwLockExt<T> for RwLock<T> {
    #[track_caller]
    fn read_or_recover(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|poisoned| {
            self.clear_poison();
            note_recovery(std::panic::Location::caller());
            poisoned.into_inner()
        })
    }
    #[track_caller]
    fn write_or_recover(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|poisoned| {
            self.clear_poison();
            note_recovery(std::panic::Location::caller());
            poisoned.into_inner()
        })
    }
    #[track_caller]
    fn try_read_or_recover(&self) -> Option<RwLockReadGuard<'_, T>> {
        match self.try_read() {
            Ok(guard) => Some(guard),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(poisoned)) => {
                let guard = poisoned.into_inner();
                self.clear_poison();
                note_recovery(std::panic::Location::caller());
                Some(guard)
            }
        }
    }
    #[track_caller]
    fn try_write_or_recover(&self) -> Option<RwLockWriteGuard<'_, T>> {
        match self.try_write() {
            Ok(guard) => Some(guard),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(poisoned)) => {
                let guard = poisoned.into_inner();
                self.clear_poison();
                note_recovery(std::panic::Location::caller());
                Some(guard)
            }
        }
    }
}

/// For lock results that come back from `Condvar` waits: keep the guard even when the lock was poisoned
/// (the next `lock_or_recover` on that mutex clears the poison).
pub trait RecoverResult<T> {
    fn recover(self) -> T;
}
impl<T> RecoverResult<T> for Result<T, PoisonError<T>> {
    fn recover(self) -> T {
        self.unwrap_or_else(PoisonError::into_inner)
    }
}

/// Free-function form of [`MutexExt::lock_or_recover`].
#[track_caller]
pub fn lock<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock_or_recover()
}

/// Runs `f`; a panic inside it is logged by the hook and returned as `None`. The thread stays alive and, in the dock
/// worker, the process does not exit. Use for one iteration of a supervisor loop or the body of a reader thread whose
/// clean-up must still run.
pub fn guarded<R>(what: &str, f: impl FnOnce() -> R) -> Option<R> {
    GUARD_DEPTH.with(|d| d.set(d.get() + 1));
    let result = catch_unwind(AssertUnwindSafe(f));
    GUARD_DEPTH.with(|d| d.set(d.get() - 1));
    match result {
        Ok(value) => Some(value),
        Err(_) => {
            limited_log(&format!("crash recovered: {}", sanitize(what)));
            None
        }
    }
}

/// Runs a long-lived service loop `f` and starts it again if it crashes (after 1 s, doubling to at most 30 s, and
/// logged each time), so a background thread never dies silently. Returns `f`'s value once it ends normally, or
/// `None` when `stopped()` turns true while waiting to restart. `f` must begin from a clean state each run.
pub fn restarting<R>(what: &str, stopped: impl Fn() -> bool, mut f: impl FnMut() -> R) -> Option<R> {
    let mut wait = std::time::Duration::from_secs(1);
    loop {
        if let Some(value) = guarded(what, &mut f) {
            return Some(value);
        }
        limited_log(&format!("{}: restarting in {} s after a crash", sanitize(what), wait.as_secs()));
        let until = std::time::Instant::now() + wait;
        while std::time::Instant::now() < until {
            if stopped() {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if stopped() {
            return None;
        }
        wait = (wait * 2).min(std::time::Duration::from_secs(30));
    }
}

/// True while this thread is inside [`guarded`] (a panic here is handled by the caller).
pub fn in_guard() -> bool {
    GUARD_DEPTH.with(|d| d.get() > 0)
}

/// Dock worker only: an unguarded panic on any thread ends this process, so the host restarts a clean worker
/// instead of keeping one with a dead thread.
pub fn exit_on_unguarded_panic() {
    EXIT_ON_UNGUARDED.store(true, Ordering::SeqCst);
}

/// Logs every panic (thread, place, short message) to device.log, then runs the default hook. `role` is "host" or
/// "dock worker". Idempotent.
pub fn install_panic_hook(role: &'static str) {
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current();
            let name = thread.name().unwrap_or("unnamed");
            let place = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "unknown place".into());
            let message = info.payload_as_str().unwrap_or("(no message)");
            let guarded = in_guard();
            limited_log(&format!(
                "panic in {role} thread '{}' at {}: {}{}",
                sanitize(name),
                sanitize(&place),
                sanitize(message),
                if guarded { " (recovered by its caller)" } else { "" }
            ));
            previous(info);
            if !guarded && EXIT_ON_UNGUARDED.load(Ordering::SeqCst) {
                limited_log(&format!("{role}: exiting after the crash so it is restarted cleanly"));
                std::process::exit(101);
            }
        }));
    });
}

fn note_recovery(at: &std::panic::Location<'_>) {
    limited_log(&format!(
        "lock recovered after a crash at {}:{}",
        sanitize(at.file()),
        at.line()
    ));
}

fn limited_log(line: &str) {
    let n = LOGGED.fetch_add(1, Ordering::SeqCst);
    if n < LOG_LIMIT {
        crate::device::log(line);
    } else if n == LOG_LIMIT {
        crate::device::log("crash log limit reached; further crash lines in this run are not written");
    }
}

/// One line, at most [`MESSAGE_LIMIT`] characters, with the user's folders replaced by placeholders.
fn sanitize(text: &str) -> String {
    let mut out = text.to_string();
    for (var, placeholder) in [
        ("UC_HOME", "<app folder>"),
        ("APPDATA", "%APPDATA%"),
        ("LOCALAPPDATA", "%LOCALAPPDATA%"),
        ("USERPROFILE", "%USERPROFILE%"),
    ] {
        if let Some(value) = std::env::var_os(var) {
            let value = value.to_string_lossy();
            if value.len() > 3 {
                out = replace_ignore_case(&out, &value, placeholder);
                out = replace_ignore_case(&out, &value.replace('\\', "/"), placeholder);
            }
        }
    }
    let mut out: String = out
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if out.chars().count() > MESSAGE_LIMIT {
        out = out.chars().take(MESSAGE_LIMIT).collect::<String>() + "...";
    }
    out
}

fn replace_ignore_case(text: &str, needle: &str, with: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(&needle_lower) {
        out.push_str(&text[at..at + found]);
        out.push_str(with);
        at += found + needle.len();
    }
    out.push_str(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn poison<T: Send + 'static>(m: &Arc<Mutex<T>>) {
        let m = m.clone();
        let _ = std::thread::spawn(move || {
            let _guard = m.lock().unwrap();
            panic!("poison on purpose");
        })
        .join();
    }

    #[test]
    fn lock_helper_recovers_a_poisoned_mutex_and_clears_the_poison() {
        let m = Arc::new(Mutex::new(5));
        poison(&m);
        assert!(m.is_poisoned());
        assert_eq!(*m.lock_or_recover(), 5);
        assert!(!m.is_poisoned(), "recovery clears the poison so it is handled once");
        *lock(&m) += 1;
        assert_eq!(*m.lock().unwrap(), 6);
    }

    #[test]
    fn lock_or_reset_runs_the_fix_up_only_after_poison() {
        let m = Arc::new(Mutex::new(vec![1, 2, 3]));
        assert_eq!(m.lock_or_reset(|v| v.clear()).len(), 3);
        poison(&m);
        assert!(m.lock_or_reset(|v| v.clear()).is_empty());
    }

    #[test]
    fn try_lock_helper_recovers_poison_but_still_reports_contention() {
        let m = Arc::new(Mutex::new(()));
        poison(&m);
        assert!(m.try_lock_or_recover().is_some());
        let held = m.lock_or_recover();
        let other = m.clone();
        assert!(std::thread::spawn(move || other.try_lock_or_recover().is_none()).join().unwrap());
        drop(held);
    }

    #[test]
    fn rwlock_helpers_recover_poison() {
        let rw = Arc::new(RwLock::new(1));
        let c = rw.clone();
        let _ = std::thread::spawn(move || {
            let _w = c.write().unwrap();
            panic!("poison on purpose");
        })
        .join();
        assert_eq!(*rw.read_or_recover(), 1);
        *rw.write_or_recover() = 2;
        assert_eq!(*rw.try_write_or_recover().unwrap(), 2);
        assert!(!rw.is_poisoned());
    }

    #[test]
    fn condvar_results_recover() {
        let m = Arc::new(Mutex::new(0));
        poison(&m);
        let cv = std::sync::Condvar::new();
        let g = m.lock().recover();
        let (g, timeout) = cv.wait_timeout(g, std::time::Duration::from_millis(1)).recover();
        assert!(timeout.timed_out());
        assert_eq!(*g, 0);
    }

    #[test]
    fn guarded_turns_a_panic_into_none_and_keeps_the_thread() {
        assert!(!in_guard());
        assert_eq!(guarded("test", || 3), Some(3));
        assert_eq!(guarded("test", || -> i32 { panic!("boom") }), None);
        assert!(!in_guard(), "depth is restored after a panic");
        assert_eq!(guarded("nested", || guarded("inner", || in_guard())), Some(Some(true)));
    }

    #[test]
    fn a_crashed_service_loop_is_started_again_and_its_result_returned() {
        let runs = std::cell::Cell::new(0);
        let started = std::time::Instant::now();
        let result = restarting("test loop", || false, || {
            runs.set(runs.get() + 1);
            if runs.get() == 1 {
                panic!("first run crashes");
            }
            "ended normally"
        });
        assert_eq!(result, Some("ended normally"));
        assert_eq!(runs.get(), 2);
        assert!(started.elapsed() >= std::time::Duration::from_millis(900), "waits before restarting");
    }

    #[test]
    fn a_stop_while_waiting_to_restart_ends_the_loop_promptly() {
        let runs = std::cell::Cell::new(0);
        let started = std::time::Instant::now();
        let result: Option<()> = restarting("test loop", || runs.get() > 0, || {
            runs.set(runs.get() + 1);
            panic!("always crashes")
        });
        assert_eq!(result, None);
        assert_eq!(runs.get(), 1);
        assert!(started.elapsed() < std::time::Duration::from_millis(900));
    }

    #[test]
    fn crash_lines_are_one_line_bounded_and_hide_user_folders() {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\someone".into());
        let text = format!("failed to open {home}\\secret.txt\nsecond line {}", "x".repeat(400));
        let clean = sanitize(&text);
        if home.len() > 3 && std::env::var_os("USERPROFILE").is_some() {
            assert!(!clean.to_ascii_lowercase().contains(&home.to_ascii_lowercase()));
            assert!(clean.contains("%USERPROFILE%"));
        }
        assert!(!clean.contains('\n'));
        assert!(clean.chars().count() <= MESSAGE_LIMIT + 3);
    }
}
