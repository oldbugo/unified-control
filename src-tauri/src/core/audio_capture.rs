//! Is an app using the microphone right now? (Claude "Dictate & send", 2026-11-17.)
//!
//! Lists the audio sessions on every active capture endpoint (Core Audio: IMMDeviceEnumerator ->
//! IAudioSessionManager2 -> IAudioSessionEnumerator) and reports whether one in the Active state belongs to a
//! process with the given exe name (any of its processes: Electron apps capture in a child process). Reads
//! state only; changes nothing and records no audio.
#[cfg(windows)]
pub fn capture_active(exe: &str) -> Result<bool, String> {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::{
        eCapture, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
        MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
    let err = |what: &str| format!("The microphone state could not be read ({what})");
    unsafe {
        // Each action runs on its own thread: initialise COM for it, and undo that when done.
        let inited = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let result = (|| -> Result<bool, String> {
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|_| err("devices"))?;
            let list = devices.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE).map_err(|_| err("devices"))?;
            for i in 0..list.GetCount().map_err(|_| err("devices"))? {
                let Ok(device) = list.Item(i) else { continue };
                let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
                let Ok(sessions) = manager.GetSessionEnumerator() else { continue };
                for j in 0..sessions.GetCount().unwrap_or(0) {
                    let Ok(session) = sessions.GetSession(j) else { continue };
                    if session.GetState().map(|s| s != AudioSessionStateActive).unwrap_or(true) {
                        continue;
                    }
                    let Ok(control) = session.cast::<IAudioSessionControl2>() else { continue };
                    let Ok(pid) = control.GetProcessId() else { continue };
                    if super::au05_hotkey::process_exe(pid).is_some_and(|e| e == exe) {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        })();
        if inited {
            CoUninitialize();
        }
        result
    }
}

#[cfg(not(windows))]
pub fn capture_active(_exe: &str) -> Result<bool, String> {
    Err("Windows only".into())
}
