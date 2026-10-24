//! Asking the running Unified Control (the one that uses the same app folder and is the same kind of build) to quit, restart or update
//! itself, from a second process: `UnifiedControl.exe --quit`, `--restart`, `--update <setup.exe>`. No window is shown and nothing is
//! started when no app is running.
//!
//! Exit codes of the requesting process:
//!   0  accepted, and the running app has exited (`--restart` / `--update`: its fresh copy / the installer follows by itself)
//!   1  accepted, but the app was still running after 20 s (it has its own 3 s exit watchdog, so this is unusual)
//!   2  refused: a job is running (upload, conversion, store, recording); nothing was done
//!   3  no running instance
//!   4  `--update`: that file is not a Unified Control installer (or does not exist); nothing was done
//!
//! How: the running app owns named events (`Local\UnifiedControlQuit-<scope>[-restart|-update]`, scope = build kind + app folder, so a
//! test instance never answers for the installed app). The requester creates answer events (`...-ok`, `-busy`, `-bad`), signals the
//! request event and waits for an answer; the `--update` path travels in a named memory section (`...-arg`). Only kernel objects are
//! used, never a file: a script started from a packaged app (Claude desktop, MSIX) sees a private, virtualised copy of %APPDATA% and
//! would never see a file the real app writes.
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Memory::{CreateFileMappingW, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ, FILE_MAP_WRITE, PAGE_READWRITE};
use windows_sys::Win32::System::Threading::{CreateEventW, OpenEventW, SetEvent, OpenProcess, WaitForMultipleObjects, WaitForSingleObject, EVENT_MODIFY_STATE, INFINITE};

pub const EXIT_OK: i32 = 0;
pub const EXIT_SLOW: i32 = 1;
pub const EXIT_BUSY: i32 = 2;
pub const EXIT_NONE: i32 = 3;
pub const EXIT_BAD: i32 = 4;

#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    Quit,
    Restart,
    /// The installer to run (already checked by `relaunch::valid_installer`).
    Update(String),
}

/// The request event's name: one per build kind and app folder (see `instance_id::scope`).
pub fn event_name() -> String {
    let scope = crate::instance_id::scope(cfg!(debug_assertions), Some(&uc_core::core::paths::base())).unwrap_or_default();
    format!(r"Local\UnifiedControlQuit-{scope}")
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const SYNCHRONIZE: u32 = 0x0010_0000;

fn open(name: &str) -> HANDLE {
    unsafe { OpenEventW(EVENT_MODIFY_STATE | SYNCHRONIZE, 0, wide(name).as_ptr()) }
}

fn create(name: &str, manual: bool) -> HANDLE {
    unsafe { CreateEventW(std::ptr::null(), manual as i32, 0, wide(name).as_ptr()) }
}

const ARG_BYTES: usize = 4096;

/// The requesting process: returns its exit code.
pub fn run_request(req: Request) -> i32 {
    let name = event_name();
    let suffix = match &req {
        Request::Quit => "",
        Request::Restart => "-restart",
        Request::Update(_) => "-update",
    };
    let alive = open(&name); // the app's main event: gone when it has exited
    let target = open(&format!("{name}{suffix}"));
    if alive.is_null() || target.is_null() {
        for h in [alive, target] {
            if !h.is_null() {
                unsafe { CloseHandle(h) };
            }
        }
        return EXIT_NONE;
    }
    unsafe { CloseHandle(alive) };
    // the argument (a path) goes in a named memory section that lives while this process waits
    let mut map: HANDLE = std::ptr::null_mut();
    if let Request::Update(p) = &req {
        let w = wide(p);
        if w.len() * 2 > ARG_BYTES {
            unsafe { CloseHandle(target) };
            return EXIT_BAD;
        }
        unsafe {
            map = CreateFileMappingW(INVALID_HANDLE_VALUE, std::ptr::null(), PAGE_READWRITE, 0, ARG_BYTES as u32, wide(&format!("{name}-arg")).as_ptr());
            if map.is_null() {
                CloseHandle(target);
                return EXIT_NONE;
            }
            let v = MapViewOfFile(map, FILE_MAP_WRITE, 0, 0, ARG_BYTES);
            if v.Value.is_null() {
                CloseHandle(map);
                CloseHandle(target);
                return EXIT_NONE;
            }
            std::ptr::copy_nonoverlapping(w.as_ptr() as *const u8, v.Value as *mut u8, w.len() * 2);
            UnmapViewOfFile(v);
        }
    }
    // the app writes its process id here when it accepts (the NEW app of a restart uses the same event names at once, so the old
    // one is recognised by its pid, not by the events)
    let pid_map = unsafe { CreateFileMappingW(INVALID_HANDLE_VALUE, std::ptr::null(), PAGE_READWRITE, 0, 64, wide(&format!("{name}-pid")).as_ptr()) };
    let (ok, busy, bad) = (create(&format!("{name}-ok"), true), create(&format!("{name}-busy"), true), create(&format!("{name}-bad"), true));
    unsafe {
        SetEvent(target);
        CloseHandle(target);
    }
    let hs = [ok, busy, bad];
    let w = unsafe { WaitForMultipleObjects(3, hs.as_ptr(), 0, 8000) };
    let mut code = EXIT_NONE;
    if w == WAIT_OBJECT_0 {
        // accepted: wait until that process is gone
        let pid = unsafe {
            let v = MapViewOfFile(pid_map, FILE_MAP_READ, 0, 0, 64);
            let p = if v.Value.is_null() { 0 } else { *(v.Value as *const u32) };
            if !v.Value.is_null() {
                UnmapViewOfFile(v);
            }
            p
        };
        let h = if pid == 0 { std::ptr::null_mut() } else { unsafe { OpenProcess(SYNCHRONIZE, 0, pid) } };
        code = if h.is_null() {
            EXIT_OK // already gone (or its id was not given)
        } else {
            let r = unsafe { WaitForSingleObject(h, 20_000) };
            unsafe { CloseHandle(h) };
            if r == WAIT_OBJECT_0 { EXIT_OK } else { EXIT_SLOW }
        };
    } else if w == WAIT_OBJECT_0 + 1 {
        code = EXIT_BUSY;
    } else if w == WAIT_OBJECT_0 + 2 {
        code = EXIT_BAD;
    }
    unsafe {
        for h in hs {
            CloseHandle(h);
        }
        if !map.is_null() {
            CloseHandle(map);
        }
        if !pid_map.is_null() {
            CloseHandle(pid_map);
        }
    }
    code
}

/// Read the path a requester put in the named section (None when there is none).
fn read_arg(name: &str) -> Option<String> {
    unsafe {
        let map = OpenFileMappingW(FILE_MAP_READ, 0, wide(&format!("{name}-arg")).as_ptr());
        if map.is_null() {
            return None;
        }
        let v = MapViewOfFile(map, FILE_MAP_READ, 0, 0, ARG_BYTES);
        let out = if v.Value.is_null() {
            None
        } else {
            let units = std::slice::from_raw_parts(v.Value as *const u16, ARG_BYTES / 2);
            let n = units.iter().position(|u| *u == 0).unwrap_or(units.len());
            let s = String::from_utf16_lossy(&units[..n]);
            UnmapViewOfFile(v);
            Some(s)
        };
        CloseHandle(map);
        out
    }
}

/// In the running app: wait for requests on a small thread (blocked on the events, no polling) and answer them. `busy` says whether a
/// job is running; `handle` carries a request out (the tray Quit path, with a relaunch for restart and update) after "ok" was sent.
pub fn listen(busy: impl Fn() -> bool + Send + 'static, handle: impl Fn(Request) + Send + 'static) {
    let name = event_name();
    let main = create(&name, false);
    let restart = create(&format!("{name}-restart"), false);
    let update = create(&format!("{name}-update"), false);
    if main.is_null() || restart.is_null() || update.is_null() {
        return;
    }
    struct H([HANDLE; 3]);
    unsafe impl Send for H {}
    let hs = H([main, restart, update]);
    std::thread::spawn(move || {
        let hs = hs;
        loop {
            let w = unsafe { WaitForMultipleObjects(3, hs.0.as_ptr(), 0, INFINITE) };
            let idx = w.wrapping_sub(WAIT_OBJECT_0) as usize;
            let answer = |suffix: &str| {
                let h = open(&format!("{name}-{suffix}"));
                if !h.is_null() {
                    unsafe {
                        SetEvent(h);
                        CloseHandle(h);
                    }
                }
            };
            let req = match idx {
                0 => Request::Quit,
                1 => Request::Restart,
                2 => match read_arg(&name).filter(|p| crate::relaunch::valid_installer(std::path::Path::new(p))) {
                    Some(p) => Request::Update(p),
                    None => {
                        uc_core::device::log("--update refused: that is not a Unified Control installer file");
                        answer("bad");
                        continue;
                    }
                },
                _ => return,
            };
            if busy() {
                uc_core::device::log(&format!("{} requested but a job is running: refused", label(&req)));
                answer("busy");
                continue;
            }
            uc_core::device::log(&format!("{} requested: the tray Quit path", label(&req)));
            unsafe {
                let m = OpenFileMappingW(FILE_MAP_WRITE, 0, wide(&format!("{name}-pid")).as_ptr());
                if !m.is_null() {
                    let v = MapViewOfFile(m, FILE_MAP_WRITE, 0, 0, 64);
                    if !v.Value.is_null() {
                        *(v.Value as *mut u32) = std::process::id();
                        UnmapViewOfFile(v);
                    }
                    CloseHandle(m);
                }
            }
            answer("ok");
            handle(req);
            return;
        }
    });
    // (the handles stay open for the life of the process: they are what `run_request` finds)
}

fn label(r: &Request) -> &'static str {
    match r {
        Request::Quit => "quit (--quit)",
        Request::Restart => "restart (--restart)",
        Request::Update(_) => "update (--update)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_event_name_follows_the_app_folder() {
        let a = event_name();
        assert!(a.starts_with(r"Local\UnifiedControlQuit-") && a.len() > r"Local\UnifiedControlQuit-".len() + 16);
        assert_eq!(a, event_name());
    }
}
