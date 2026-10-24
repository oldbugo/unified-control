//! Child processes that must not outlive the app (ffmpeg for scene recordings and core tasks): they are put in one
//! Windows Job Object with "kill on job close", so when the app exits, even by a crash or Task Manager, Windows
//! ends them too.
use std::process::Child;

#[cfg(windows)]
mod imp {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    struct Job(HANDLE);
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    fn job() -> Option<&'static Job> {
        static J: OnceLock<Option<Job>> = OnceLock::new();
        J.get_or_init(|| unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if h.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                h,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            // The handle is never closed: it closes when the process ends, which kills the job's processes.
            (ok != 0).then_some(Job(h))
        })
        .as_ref()
    }

    pub fn adopt(child: &std::process::Child) -> bool {
        match job() {
            Some(j) => unsafe { AssignProcessToJobObject(j.0, child.as_raw_handle() as HANDLE) != 0 },
            None => false,
        }
    }
}

/// Tie `child`'s lifetime to this process. Returns false if that was not possible (the child still runs normally).
pub fn adopt(child: &Child) -> bool {
    #[cfg(windows)]
    {
        imp::adopt(child)
    }
    #[cfg(not(windows))]
    {
        let _ = child;
        false
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(windows)]
    fn children_die_with_the_app() {
        // A helper process adopts a long-running child, then exits; the child must be gone.
        let exe = std::env::current_exe().unwrap();
        let out = std::process::Command::new(&exe)
            .args(["--exact", "core::jobobject::tests::helper_spawns_child", "--ignored", "--nocapture"])
            .env("CS_JOB_HELPER", "1")
            .output()
            .unwrap();
        let s = String::from_utf8_lossy(&out.stdout);
        let pid: u32 = s.lines().find_map(|l| l.strip_prefix("CHILD_PID ")).expect("helper printed the child pid").trim().parse().unwrap();
        // the system ends the child when the job handle closes; on a busy machine that can take a moment, so wait (up to 10 s)
        let t0 = std::time::Instant::now();
        let alive = || {
            let list = std::process::Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH"]).output().unwrap();
            String::from_utf8_lossy(&list.stdout).contains(&pid.to_string())
        };
        while alive() && t0.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(!alive(), "child {pid} outlived its parent");
    }

    #[test]
    #[ignore = "helper for children_die_with_the_app"]
    fn helper_spawns_child() {
        if std::env::var_os("CS_JOB_HELPER").is_none() {
            return;
        }
        let child = std::process::Command::new("powershell").args(["-NoProfile", "-Command", "Start-Sleep -Seconds 60"]).spawn().unwrap();
        assert!(super::adopt(&child));
        println!("CHILD_PID {}", child.id());
        std::mem::forget(child); // exit without waiting or killing
    }
}
