//! Restart and update without a shell: the running app spawns a small detached helper (this same exe in a special mode), quits the
//! normal way (tray Quit path), and the helper waits for it to be gone, optionally runs the installer silently, then starts the app
//! again quietly (`--background`) or as before (no argument).
//!
//! The helper is spawned by the real app, never by the script that asked, so it (and the app it starts) run outside the Claude
//! app's private MSIX view of %APPDATA% (see notes C5): a child inherits the package identity of its PARENT, and the real app, started
//! from the Start menu or Explorer, has none.
//!
//! For an update the helper is a COPY of the exe in the temp folder (the installer must be able to replace the installed exe while
//! the helper still runs); `UnifiedControl-updater-<pid>.exe` copies of earlier runs are removed at the next helper start.
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION};

/// The argument that puts this exe into helper mode (also what `tools/install-when-idle.ps1` looks for to know a build can update).
pub const HELPER_FLAG: &str = "--relaunch-after";

const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
const SYNCHRONIZE: u32 = 0x0010_0000;

fn log(msg: &str) {
    uc_core::device::log(&format!("updater: {msg}"));
}

/// Only a file named like the NSIS installer of this app (`UnifiedControl_<x>.<y>.<z>_x64-setup.exe`), on a local drive, that exists.
pub fn valid_installer(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    p.is_absolute() && !p.to_string_lossy().starts_with(r"\\") && p.is_file() && installer_name_ok(name)
}

/// The file-name rule alone (see `valid_installer`).
pub fn installer_name_ok(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("UnifiedControl_").and_then(|r| r.strip_suffix("_x64-setup.exe")) else { return false };
    let parts: Vec<&str> = rest.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Start the detached helper for `app_pid` (the caller then quits normally). `background`: start the app again with `--background`
/// (else with no argument, so its window shows). `setup`: run this installer silently first (already validated).
pub fn spawn_helper(app_pid: u32, background: bool, setup: Option<&Path>) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    // for an update the helper must not be the installed exe (the installer replaces that file)
    let helper: PathBuf = if setup.is_some() {
        let tmp = std::env::temp_dir().join(format!("UnifiedControl-updater-{app_pid}.exe"));
        std::fs::copy(&exe, &tmp)?;
        tmp
    } else {
        exe.clone()
    };
    let mut args: Vec<std::ffi::OsString> = vec![HELPER_FLAG.into(), app_pid.to_string().into(), (if background { "background" } else { "plain" }).into(), exe.clone().into()];
    if let Some(s) = setup {
        args.push(s.into());
    }
    let go = |flags: u32| Command::new(&helper).args(&args).creation_flags(flags).spawn();
    // out of the app's job object when it is allowed to leave (so the helper outlives the app), else as is
    go(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| go(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW))
        .map(|_| ())
}

/// Wait for a process to end; true when it has ended (or cannot be opened any more).
fn wait_gone(pid: u32) -> bool {
    let h = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
    if h.is_null() {
        return true;
    }
    // Shutdown can be draining an active Monitor Toggle/audio recovery. The
    // prepared helper must not abandon restart while that transaction finishes.
    // This handle refers to the exact original process, not a reusable PID.
    let r = unsafe { WaitForSingleObject(h, u32::MAX) };
    unsafe { CloseHandle(h) };
    r == WAIT_OBJECT_0
}

/// Other processes that run from one of `files` (the installed exe and its agent-request copy). The dock worker runs from the
/// installed exe and can still be closing after the host has gone; while any of them runs, the installer cannot replace the file.
fn running_from(files: &[PathBuf]) -> Vec<u32> {
    let me = std::process::id();
    let mut found = Vec::new();
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return found;
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) };
    while ok != 0 {
        let pid = entry.th32ProcessID;
        if pid != me && pid != 0 {
            if let Some(path) = image_path(pid) {
                if files.iter().any(|f| same_file_path(f, &path)) {
                    found.push(pid);
                }
            }
        }
        ok = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    unsafe { CloseHandle(snapshot) };
    found
}

fn image_path(pid: u32) -> Option<PathBuf> {
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if h.is_null() {
        return None;
    }
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) };
    unsafe { CloseHandle(h) };
    (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
}

fn same_file_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().replace('/', "\\").eq_ignore_ascii_case(&b.to_string_lossy().replace('/', "\\"))
}

/// Before an install: wait (up to `limit`) until nothing else runs from the installed files. Never ends a process. True when free.
fn wait_files_free(files: &[PathBuf], limit: Duration) -> bool {
    let t0 = Instant::now();
    loop {
        let busy = running_from(files);
        if busy.is_empty() {
            if t0.elapsed() >= Duration::from_millis(250) {
                log(&format!("installed files are free after {} ms", t0.elapsed().as_millis()));
            }
            return true;
        }
        if t0.elapsed() >= limit {
            log(&format!("{} process(es) still run from the installed files after {} s", busy.len(), limit.as_secs()));
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The helper process: `--relaunch-after <pid> <plain|background> <exe> [<setup>]`. Returns its exit code (0 started the app again).
pub fn run_helper(args: &[String]) -> i32 {
    let i = args.iter().position(|a| a == HELPER_FLAG).unwrap_or(0);
    let (Some(pid), Some(mode), Some(exe)) = (args.get(i + 1).and_then(|p| p.parse::<u32>().ok()), args.get(i + 2), args.get(i + 3)) else { return 1 };
    let setup = args.get(i + 4).map(PathBuf::from);
    // earlier updater copies (this one is running: it cannot be deleted, and is skipped)
    if let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) {
        let me = std::env::current_exe().ok();
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("UnifiedControl-updater-") && n.ends_with(".exe") && Some(e.path()) != me {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    if !wait_gone(pid) {
        log("could not wait for the app to exit: nothing installed, not started again");
        return 1;
    }
    if let Some(s) = &setup {
        let installed = PathBuf::from(exe);
        let files = [installed.clone(), installed.with_file_name("UC-AgentRequest-v1.exe")];
        if !valid_installer(s) {
            log("the installer is not valid any more: not run");
        } else if !wait_files_free(&files, Duration::from_secs(30)) {
            // A half install (new engine, old app) is worse than none: keep the current version and start it again.
            log("not installed: the app files are still in use (nothing was ended); try the update again later");
        } else {
            log(&format!("running {} /S", s.display()));
            match Command::new(s).arg("/S").creation_flags(CREATE_NO_WINDOW).spawn() {
                Ok(mut c) => {
                    let t0 = Instant::now();
                    loop {
                        match c.try_wait() {
                            Ok(Some(st)) => {
                                log(&format!("installer finished: {st}"));
                                break;
                            }
                            Ok(None) if t0.elapsed() < Duration::from_secs(600) => std::thread::sleep(Duration::from_millis(250)),
                            _ => {
                                log("installer still running after 10 min: not waiting any longer");
                                break;
                            }
                        }
                    }
                }
                Err(e) => log(&format!("could not run the installer: {e}")),
            }
        }
    }
    // start the app again, quietly or as it was; whatever happened above, the user gets an app back
    let mut c = Command::new(exe);
    if mode == "background" {
        c.arg("--background");
    }
    match c.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW).spawn() {
        Ok(_) => {
            log(&format!("started {exe} again ({mode})"));
            0
        }
        Err(e) => {
            log(&format!("could not start {exe}: {e}"));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_waits_for_every_process_running_from_the_installed_files() {
        assert!(same_file_path(Path::new("C:/Users/x/AppData/Local/Unified Control/UnifiedControl.exe"), Path::new(r"c:\users\x\appdata\local\unified control\unifiedcontrol.exe")));
        assert!(!same_file_path(Path::new(r"C:\a\UnifiedControl.exe"), Path::new(r"C:\b\UnifiedControl.exe")));
        // A real, quiet child process is found by its image path while it runs, and the wait gives up without ending it.
        let ping = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join(r"System32\PING.EXE");
        let mut child = Command::new(&ping).args(["-n", "4", "127.0.0.1"]).creation_flags(CREATE_NO_WINDOW).stdout(std::process::Stdio::null()).spawn().unwrap();
        assert!(running_from(std::slice::from_ref(&ping)).contains(&child.id()));
        assert!(!wait_files_free(std::slice::from_ref(&ping), Duration::from_millis(300)));
        assert!(child.try_wait().unwrap().is_none(), "waiting never ends the process");
        child.wait().unwrap();
        assert!(wait_files_free(std::slice::from_ref(&ping), Duration::from_secs(5)));
        // This process itself is never counted, and files nothing runs from are free at once.
        let me = std::env::current_exe().unwrap();
        assert!(running_from(std::slice::from_ref(&me)).is_empty());
        let t0 = Instant::now();
        assert!(wait_files_free(&[PathBuf::from(r"C:\definitely\missing\UnifiedControl.exe")], Duration::from_secs(5)));
        assert!(t0.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn only_the_installer_file_name_of_this_app_is_accepted() {
        assert!(installer_name_ok("UnifiedControl_4.10.1_x64-setup.exe"));
        assert!(installer_name_ok("UnifiedControl_10.0.12_x64-setup.exe"));
        assert!(!installer_name_ok("CoolerScreen_4.18.0_x64-setup.exe"), "never the CoolerScreen installer");
        for bad in ["UnifiedControl.exe", "setup.exe", "UnifiedControl_4.10_x64-setup.exe", "UnifiedControl_4.10.1_x64-setup.exe.bat", "UnifiedControl_4.x.1_x64-setup.exe", "UnifiedControl_4.10.1_x86-setup.exe",
            "..\\UnifiedControl_4.10.1_x64-setup.exe", "UnifiedControl_4.10.1.2_x64-setup.exe", "unifiedcontrol_4.10.1_x64-setup.exe", "UnifiedControl__x64-setup.exe", "UnifiedControl_4.10.1_x64-setup.exe "] {
            assert!(!installer_name_ok(bad), "{bad}");
        }
        // it must also exist, be absolute and not be a network path
        assert!(!valid_installer(Path::new("UnifiedControl_4.10.1_x64-setup.exe")));
        assert!(!valid_installer(Path::new(r"\\server\share\UnifiedControl_4.10.1_x64-setup.exe")));
        assert!(!valid_installer(Path::new(r"C:\definitely\missing\UnifiedControl_4.10.1_x64-setup.exe")));
    }
}
