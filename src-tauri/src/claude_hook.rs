//! `UnifiedControl.exe --claude-hook <event>`: run by Claude Code hooks (see core/claude_sessions.rs).
//!
//! Reads the hook's stdin JSON and exactly two environment variables (`CLAUDE_CODE_HOST_SESSION_ID`, the
//! desktop session id, and `CLAUDE_CODE_SESSION_ID`), and sends only the fields built by
//! `claude_sessions::hook_payload` to the running UC over its local pipe. No other environment variable is
//! read (the environment holds tokens and the user's email). It prints nothing (SessionStart and
//! UserPromptSubmit stdout would reach Claude's context), writes no file, never opens a window, and always
//! exits 0 within about 200 ms, so Claude is never slowed or blocked. If UC is not running, nothing happens.
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uc_core::core::claude_sessions;

const STDIN_LIMIT: u64 = 256 * 1024;

pub fn run(args: &[String]) -> i32 {
    let event = args.iter().position(|a| a == claude_sessions::HOOK_FLAG).and_then(|i| args.get(i + 1)).cloned();
    let _ = std::panic::catch_unwind(move || {
        let Some(event) = event else { return };
        // Read stdin on a helper thread so a stdin that never closes cannot hold the hook.
        let (tx, rx) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new().spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::stdin().take(STDIN_LIMIT).read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
        let input = rx.recv_timeout(Duration::from_millis(80)).unwrap_or_default();
        let stdin = serde_json::from_slice(&input).unwrap_or(serde_json::Value::Null);
        let desktop = std::env::var("CLAUDE_CODE_HOST_SESSION_ID").ok();
        let code = std::env::var("CLAUDE_CODE_SESSION_ID").ok();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        if let Some(payload) = claude_sessions::hook_payload(&event, &stdin, desktop.as_deref(), code.as_deref(), now) {
            crate::agent_transport::notify(
                serde_json::json!({"method": "claude_hook", "params": payload}),
                Duration::from_millis(120),
            );
        }
    });
    0
}
