//! "Start Unified Control when Windows starts" (opt-in, off by default): one value in
//! HKCU\Software\Microsoft\Windows\CurrentVersion\Run that starts the app hidden in the tray (`--tray`).
//! The registry is the truth (the user can also remove it in Task Manager > Startup); the installer's uninstall removes it too.
pub const VALUE_NAME: &str = "UnifiedControl";
pub const TRAY_FLAG: &str = "--tray";

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::Registry::*;

    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn get(name: &str) -> Option<String> {
        let (k, n) = (w(KEY), w(name));
        let mut size: u32 = 0;
        unsafe {
            if RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), &mut size) != 0 || size == 0 {
                return None;
            }
            let mut buf = vec![0u16; size as usize / 2 + 1];
            let mut size2 = (buf.len() * 2) as u32;
            if RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr() as *mut _, &mut size2) != 0 {
                return None;
            }
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..len]))
        }
    }

    pub fn set(name: &str, value: &str) -> bool {
        let (k, n, v) = (w(KEY), w(name), w(value));
        unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr(), REG_SZ, v.as_ptr() as *const _, (v.len() * 2) as u32) == 0 }
    }

    pub fn remove(name: &str) -> bool {
        let (k, n) = (w(KEY), w(name));
        let r = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ptr()) };
        r == 0 || r == 2 // 2 = not there
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn get(_: &str) -> Option<String> {
        None
    }
    pub fn set(_: &str, _: &str) -> bool {
        false
    }
    pub fn remove(_: &str) -> bool {
        true
    }
}

/// The command the Run entry holds for this exe.
pub fn command_for(exe: &std::path::Path) -> String {
    format!("\"{}\" {}", exe.display(), TRAY_FLAG)
}

pub fn is_on() -> bool {
    imp::get(VALUE_NAME).is_some()
}

/// Add or remove the entry (named `name`; the app uses `VALUE_NAME`).
pub fn set_named(name: &str, on: bool) -> bool {
    if on {
        match std::env::current_exe() {
            Ok(exe) => imp::set(name, &command_for(&exe)),
            Err(_) => false,
        }
    } else {
        imp::remove(name)
    }
}

pub fn set(on: bool) -> bool {
    set_named(VALUE_NAME, on)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn run_entry_round_trip_uses_its_own_name() {
        let name = "UnifiedControlTestEntry";
        assert!(set_named(name, true));
        let v = imp::get(name).expect("written");
        assert!(v.starts_with('"') && v.ends_with(" --tray"), "{v}");
        assert!(set_named(name, false));
        assert!(imp::get(name).is_none());
        assert!(set_named(name, false), "removing twice is fine");
    }
}
