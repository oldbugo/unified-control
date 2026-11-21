//! Restart the Vibe Key receiver (1.3.0, user request 2026-11-21).
//!
//! Real receiver 2026-11-21: after Windows started from a Fast Startup shutdown, the receiver stayed in Device
//! Manager with its light solid, but passed nothing on (no private message, no key press) until it was unplugged
//! and plugged in again. Fast Startup stays on (the user's choice), so the page offers "Restart receiver": only
//! when the user clicks it, Windows' own `pnputil /restart-device` restarts the receiver's USB device, through the
//! Windows permission prompt (UAC). Nothing is sent to the key or the receiver by UC itself, and nothing runs on
//! its own.

/// The Vibe Key receiver (`USB\VID_FFF1&PID_00DD\<serial>`: the USB composite device, not one of its
/// interfaces).
const RECEIVER_PREFIX: &str = r"USB\VID_FFF1&PID_00DD\";

/// The receiver's USB device among Windows' device instance IDs: exactly one, with a plain serial.
pub fn receiver_instance(ids: &[String]) -> Result<String, String> {
    let found: Vec<&String> = ids
        .iter()
        .filter(|id| id.len() > RECEIVER_PREFIX.len() && id[..RECEIVER_PREFIX.len()].eq_ignore_ascii_case(RECEIVER_PREFIX))
        .filter(|id| !id[RECEIVER_PREFIX.len()..].contains('\\'))
        .collect();
    match found.as_slice() {
        [] => Err("The Vibe Key receiver is not plugged in".into()),
        [one] if valid_instance_id(one) => Ok((*one).clone()),
        [_] => Err("The receiver's Windows device ID has unexpected characters: not restarted".into()),
        _ => Err("More than one Vibe Key receiver is plugged in: unplug and replug the one to restart".into()),
    }
}

/// Only characters a device instance ID uses (it goes into a command line in quotes).
pub fn valid_instance_id(id: &str) -> bool {
    !id.is_empty() && id.len() < 200 && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '\\' | '&' | '_' | '.' | '-' | '#'))
}

/// Restart the receiver now (blocks until Windows answers, at most about a minute with the prompt).
#[cfg(windows)]
pub fn restart() -> Result<String, String> {
    let id = receiver_instance(&present_usb_ids())?;
    run_pnputil(&id)?;
    Ok(id)
}

#[cfg(not(windows))]
pub fn restart() -> Result<String, String> {
    Err("Windows only".into())
}

/// The instance IDs of every USB device present now.
#[cfg(windows)]
fn present_usb_ids() -> Vec<String> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_Device_ID_ListW, CM_Get_Device_ID_List_SizeW, CM_GETIDLIST_FILTER_ENUMERATOR, CM_GETIDLIST_FILTER_PRESENT, CR_SUCCESS,
    };
    let filter: Vec<u16> = "USB".encode_utf16().chain(Some(0)).collect();
    let flags = CM_GETIDLIST_FILTER_ENUMERATOR | CM_GETIDLIST_FILTER_PRESENT;
    for _ in 0..3 {
        let mut len = 0u32;
        if unsafe { CM_Get_Device_ID_List_SizeW(&mut len, filter.as_ptr(), flags) } != CR_SUCCESS || len == 0 {
            return Vec::new();
        }
        let mut buf = vec![0u16; len as usize];
        // The list can grow between the two calls: try again then.
        if unsafe { CM_Get_Device_ID_ListW(filter.as_ptr(), buf.as_mut_ptr(), len, flags) } != CR_SUCCESS {
            continue;
        }
        return buf.split(|c| *c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect();
    }
    Vec::new()
}

/// `pnputil /restart-device "<id>"`, elevated through the Windows prompt, no console window.
#[cfg(windows)]
fn run_pnputil(id: &str) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    const ERROR_CANCELLED: u32 = 1223;
    const SW_HIDE: i32 = 0;
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() };
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let file = wide(&format!(r"{root}\System32\pnputil.exe"));
    let params = wide(&format!("/restart-device \"{id}\""));
    let verb = wide("runas");
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: params.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let e = unsafe { GetLastError() };
        return Err(if e == ERROR_CANCELLED {
            "Not restarted: the Windows permission prompt was declined".into()
        } else {
            format!("Windows could not start the restart (error {e})")
        });
    }
    if info.hProcess.is_null() {
        return Err("Windows started the restart but gave no way to follow it".into());
    }
    let waited = unsafe { WaitForSingleObject(info.hProcess, 60_000) };
    let mut code = 0u32;
    let got = unsafe { GetExitCodeProcess(info.hProcess, &mut code) };
    unsafe { CloseHandle(info.hProcess) };
    if waited != WAIT_OBJECT_0 {
        return Err("The restart did not finish within a minute".into());
    }
    // pnputil: 0 done; 3010 done, a Windows restart is needed to finish.
    match (got != 0, code) {
        (true, 0) => Ok(()),
        (true, 3010) => Err("Windows says the receiver restarts only after a Windows restart".into()),
        (true, c) => Err(format!("Windows could not restart the receiver (pnputil exit code {c})")),
        (false, _) => Err("The restart's result could not be read".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_receiver_is_its_usb_device_not_an_interface() {
        let list = ids(&[
            r"USB\ROOT_HUB30\7&360C8EAD&0&0",
            r"USB\VID_FFF1&PID_00DD&MI_02\A&12B9AE1D&0&0002",
            r"USB\VID_FFF1&PID_00DD\202606031150",
            r"USB\VID_FFF1&PID_00DD&MI_03\A&12B9AE1D&0&0003",
        ]);
        assert_eq!(receiver_instance(&list).unwrap(), r"USB\VID_FFF1&PID_00DD\202606031150");
        assert_eq!(receiver_instance(&ids(&[r"usb\vid_fff1&pid_00dd\ABC"])).unwrap(), r"usb\vid_fff1&pid_00dd\ABC");
    }

    #[test]
    fn none_or_several_receivers_or_odd_ids_are_not_restarted() {
        assert!(receiver_instance(&ids(&[r"USB\VID_046D&PID_C53A\1"])).unwrap_err().contains("not plugged in"));
        assert!(receiver_instance(&ids(&[r"USB\VID_FFF1&PID_00DD\1", r"USB\VID_FFF1&PID_00DD\2"])).unwrap_err().contains("More than one"));
        assert!(receiver_instance(&ids(&[r#"USB\VID_FFF1&PID_00DD\1" /x"#])).unwrap_err().contains("unexpected"));
        assert!(!valid_instance_id(r#"a"b"#));
        assert!(!valid_instance_id(""));
    }
}
