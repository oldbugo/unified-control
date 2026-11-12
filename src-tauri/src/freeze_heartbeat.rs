use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const INTERVAL: Duration = Duration::from_secs(60);
const LOG_LIMIT: u64 = 256 * 1024;

/// Runs in the tray host, independently of the webview and device workers.
pub fn start(base: PathBuf, enabled: impl Fn() -> bool + Send + 'static) -> io::Result<()> {
    let started = unix_seconds();
    let pid = std::process::id();
    thread::Builder::new()
        .name("freeze-heartbeat".into())
        .spawn(move || {
            let mut reported_error = false;
            loop {
                if enabled() {
                    let result = append(&base, unix_seconds(), started, pid, LOG_LIMIT);
                    if result.is_err() && !reported_error {
                        uc_core::device::log("freeze heartbeat could not write its local log");
                    }
                    reported_error = result.is_err();
                }
                thread::sleep(INTERVAL);
            }
        })?;
    Ok(())
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn append(base: &Path, now: u64, started: u64, pid: u32, limit: u64) -> io::Result<()> {
    fs::create_dir_all(base)?;
    let log = base.join("freeze-heartbeat.log");
    let line = format!("{now} {started} {pid}\n");
    match fs::metadata(&log) {
        Ok(meta) if meta.len().saturating_add(line.len() as u64) > limit => {
            fs::rename(&log, base.join("freeze-heartbeat.previous.log"))?;
        }
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    let mut file = OpenOptions::new().create(true).append(true).open(log)?;
    file.write_all(line.as_bytes())?;
    file.sync_data()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_current_and_previous_samples_across_multiple_rotations() {
        let dir = tempfile::tempdir().unwrap();
        for now in 100..105 {
            append(dir.path(), now, 100, 12, 22).unwrap();
        }
        assert_eq!(
            fs::read_to_string(dir.path().join("freeze-heartbeat.log")).unwrap(),
            "104 100 12\n"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("freeze-heartbeat.previous.log")).unwrap(),
            "102 100 12\n103 100 12\n"
        );
    }
}
