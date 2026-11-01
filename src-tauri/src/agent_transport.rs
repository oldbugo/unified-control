//! Local, same-user RPC between an MCP adapter and the running tray app.
//! This module never opens a window, reads app-data files or touches a device.
//! The pipe is protected with an explicit current-user/SYSTEM DACL and rejects
//! remote clients. Debug/UC_HOME scopes never join the installed app's pipe.
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::Arc;

pub type Handler = Arc<dyn Fn(Value) -> Value + Send + Sync>;
const MAX_REQUEST: usize = 1024 * 1024;
const MAX_RESPONSE: usize = 12 * 1024 * 1024;

fn failure(code: &str, message: &str) -> Value {
    json!({"ok":false,"error":{"code":code,"message":message}})
}

fn valid_request(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == 2
            && value["params"].is_object()
            && value["method"].as_str().is_some_and(|method| {
                !method.is_empty()
                    && method.len() <= 80
                    && method.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            })
    })
}

struct BoundedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn encode(value: &Value, limit: usize) -> Result<Vec<u8>, ()> {
    let mut output = BoundedBytes {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut output, value).map_err(|_| ())?;
    Ok(output.bytes)
}

/// Request-only CLI mode. The caller closes stdin after one JSON request.
/// This is called before constructing Tauri or its single-instance plugin.
pub fn run_request() -> i32 {
    let mut input = Vec::new();
    let result = match std::io::stdin()
        .take((MAX_REQUEST + 1) as u64)
        .read_to_end(&mut input)
    {
        Ok(_) if input.len() <= MAX_REQUEST => match serde_json::from_slice::<Value>(&input) {
            Ok(value) if valid_request(&value) => request(value),
            _ => failure("invalid_request", "The local agent request is invalid"),
        },
        _ => failure(
            "invalid_request",
            "The local agent request is too large or unreadable",
        ),
    };
    let (output, ok) = match encode(&result, MAX_RESPONSE) {
        Ok(output) => (output, result["ok"] == true),
        Err(_) => (br#"{"ok":false,"error":{"code":"response_limit","message":"The local agent result exceeds its size limit"}}"#.to_vec(), false),
    };
    let mut stdout = std::io::stdout().lock();
    if stdout
        .write_all(&output)
        .and_then(|_| stdout.write_all(b"\n"))
        .and_then(|_| stdout.flush())
        .is_err()
    {
        return 1;
    }
    if ok {
        0
    } else {
        1
    }
}

#[cfg(windows)]
pub use windows::Server;

#[cfg(windows)]
pub fn start(handler: Handler) -> Result<Server, String> {
    windows::start(handler)
}

#[cfg(windows)]
fn request(value: Value) -> Value {
    windows::request(value)
}

#[cfg(not(windows))]
pub struct Server;
#[cfg(not(windows))]
impl Server {
    pub fn shutdown(&mut self) {}
}
#[cfg(not(windows))]
pub fn start(_handler: Handler) -> Result<Server, String> {
    Err("Local agent access requires Windows".into())
}
#[cfg(not(windows))]
fn request(_value: Value) -> Value {
    failure("unsupported", "Local agent access requires Windows")
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    type Handle = *mut c_void;
    const INVALID_HANDLE: Handle = -1isize as Handle;
    const ERROR_IO_PENDING: u32 = 997;
    const ERROR_PIPE_CONNECTED: u32 = 535;
    const ERROR_PIPE_BUSY: u32 = 231;
    const WAIT_OBJECT_0: u32 = 0;
    const WAIT_TIMEOUT: u32 = 258;
    const TOKEN_QUERY: u32 = 0x0008;
    const GENERIC_READ_WRITE: u32 = 0xc0000000;
    const FILE_FLAG_OVERLAPPED: u32 = 0x40000000;
    const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x00080000;
    const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x00000008;
    const MAX_CLIENTS: usize = 8;
    const IO_TIMEOUT: Duration = Duration::from_secs(5);
    const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

    #[repr(C)]
    struct SecurityAttributes {
        length: u32,
        descriptor: *mut c_void,
        inherit: i32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: Handle,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct TokenUser {
        sid: *mut c_void,
        attributes: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CloseHandle(handle: Handle) -> i32;
        fn GetLastError() -> u32;
        fn GetCurrentProcess() -> Handle;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
        fn CreateNamedPipeW(
            name: *const u16,
            open_mode: u32,
            pipe_mode: u32,
            instances: u32,
            out_size: u32,
            in_size: u32,
            timeout: u32,
            security: *const SecurityAttributes,
        ) -> Handle;
        fn ConnectNamedPipe(pipe: Handle, overlapped: *mut Overlapped) -> i32;
        fn DisconnectNamedPipe(pipe: Handle) -> i32;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *const SecurityAttributes,
            creation: u32,
            flags: u32,
            template: Handle,
        ) -> Handle;
        fn WaitNamedPipeW(name: *const u16, timeout: u32) -> i32;
        fn GetNamedPipeServerProcessId(pipe: Handle, pid: *mut u32) -> i32;
        fn ReadFile(
            file: Handle,
            bytes: *mut c_void,
            length: u32,
            transferred: *mut u32,
            overlapped: *mut Overlapped,
        ) -> i32;
        fn WriteFile(
            file: Handle,
            bytes: *const c_void,
            length: u32,
            transferred: *mut u32,
            overlapped: *mut Overlapped,
        ) -> i32;
        fn CreateEventW(
            security: *const SecurityAttributes,
            manual: i32,
            initial: i32,
            name: *const u16,
        ) -> Handle;
        fn WaitForSingleObject(handle: Handle, timeout: u32) -> u32;
        fn GetOverlappedResult(
            file: Handle,
            overlapped: *mut Overlapped,
            transferred: *mut u32,
            wait: i32,
        ) -> i32;
        fn CancelIoEx(file: Handle, overlapped: *mut Overlapped) -> i32;
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        fn GetTokenInformation(
            token: Handle,
            class: i32,
            info: *mut c_void,
            length: u32,
            returned: *mut u32,
        ) -> i32;
        fn ConvertSidToStringSidW(sid: *const c_void, string: *mut *mut u16) -> i32;
        fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
            string: *const u16,
            revision: u32,
            descriptor: *mut *mut c_void,
            size: *mut u32,
        ) -> i32;
        #[cfg(test)]
        fn GetSecurityInfo(
            handle: Handle,
            object_type: i32,
            information: u32,
            owner: *mut *mut c_void,
            group: *mut *mut c_void,
            dacl: *mut *mut c_void,
            sacl: *mut *mut c_void,
            descriptor: *mut *mut c_void,
        ) -> u32;
        #[cfg(test)]
        fn ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor: *const c_void,
            revision: u32,
            information: u32,
            string: *mut *mut u16,
            length: *mut u32,
        ) -> i32;
    }

    struct OwnedHandle(Handle);
    unsafe impl Send for OwnedHandle {}
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    fn owned(handle: Handle) -> Result<OwnedHandle, ()> {
        if handle.is_null() || handle == INVALID_HANDLE {
            Err(())
        } else {
            Ok(OwnedHandle(handle))
        }
    }
    struct LocalMemory(*mut c_void);
    impl Drop for LocalMemory {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    fn user_sid(process: Handle) -> Result<String, ()> {
        let mut token = std::ptr::null_mut();
        if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
            return Err(());
        }
        let token = owned(token)?;
        let mut length = 0;
        unsafe {
            GetTokenInformation(token.0, 1, std::ptr::null_mut(), 0, &mut length);
        }
        if length == 0 || length > 64 * 1024 {
            return Err(());
        }
        let mut bytes = vec![0u8; length as usize];
        if unsafe {
            GetTokenInformation(token.0, 1, bytes.as_mut_ptr().cast(), length, &mut length)
        } == 0
        {
            return Err(());
        }
        let user = unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<TokenUser>()) };
        let mut text = std::ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(user.sid, &mut text) } == 0 {
            return Err(());
        }
        let memory = LocalMemory(text.cast());
        let mut count = 0;
        while count < 256 && unsafe { *text.add(count) } != 0 {
            count += 1;
        }
        if count == 256 {
            return Err(());
        }
        let value = String::from_utf16(unsafe { std::slice::from_raw_parts(text, count) })
            .map_err(|_| ())?;
        drop(memory);
        if !value.starts_with("S-1-")
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b'S' || b == b'-')
        {
            return Err(());
        }
        Ok(value)
    }
    fn endpoint_for(sid: &str, debug: bool, home: Option<&std::path::Path>) -> String {
        let scope = crate::instance_id::scope(debug, home).unwrap_or_else(|| "installed".into());
        format!(r"\\.\pipe\UnifiedControl-Agent-v1-{sid}-{scope}")
    }
    fn identity() -> Result<(String, String), ()> {
        let sid = user_sid(unsafe { GetCurrentProcess() })?;
        let name = endpoint_for(
            &sid,
            cfg!(debug_assertions),
            crate::instance_id::env_home().as_deref(),
        );
        Ok((sid, name))
    }
    fn descriptor(sid: &str) -> Result<LocalMemory, ()> {
        // Protected DACL: no inherited/Everyone/anonymous ACEs.
        let sddl = wide(&format!("D:P(A;;GA;;;SY)(A;;GA;;;{sid})"));
        let mut descriptor = std::ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(());
        }
        Ok(LocalMemory(descriptor))
    }
    fn create_pipe(name: &str, sid: &str, first: bool) -> Result<OwnedHandle, ()> {
        let descriptor = descriptor(sid)?;
        let attributes = SecurityAttributes {
            length: std::mem::size_of::<SecurityAttributes>() as u32,
            descriptor: descriptor.0,
            inherit: 0,
        };
        owned(unsafe {
            CreateNamedPipeW(
                wide(name).as_ptr(),
                3 | FILE_FLAG_OVERLAPPED
                    | if first {
                        FILE_FLAG_FIRST_PIPE_INSTANCE
                    } else {
                        0
                    },
                PIPE_REJECT_REMOTE_CLIENTS,
                16,
                65536,
                65536,
                1000,
                &attributes,
            )
        })
    }

    /// An overlapped operation always cancels AND drains before its memory or
    /// buffer is dropped, including timeout and server shutdown paths.
    fn complete(
        file: Handle,
        event: &OwnedHandle,
        overlapped: &mut Overlapped,
        immediate: i32,
        transferred: &mut u32,
        deadline: Instant,
        stop: Option<&AtomicBool>,
    ) -> Result<(), ()> {
        if immediate != 0 {
            return Ok(());
        }
        if unsafe { GetLastError() } != ERROR_IO_PENDING {
            return Err(());
        }
        loop {
            if Instant::now() >= deadline || stop.is_some_and(|s| s.load(Ordering::Acquire)) {
                unsafe {
                    CancelIoEx(file, overlapped);
                    GetOverlappedResult(file, overlapped, transferred, 1);
                }
                return Err(());
            }
            match unsafe { WaitForSingleObject(event.0, 25) } {
                WAIT_OBJECT_0 => {
                    return if unsafe { GetOverlappedResult(file, overlapped, transferred, 0) } != 0
                    {
                        Ok(())
                    } else {
                        Err(())
                    }
                }
                WAIT_TIMEOUT => {}
                _ => {
                    unsafe {
                        CancelIoEx(file, overlapped);
                        GetOverlappedResult(file, overlapped, transferred, 1);
                    }
                    return Err(());
                }
            }
        }
    }
    fn operation_event() -> Result<OwnedHandle, ()> {
        owned(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) })
    }
    fn connect(pipe: &OwnedHandle, stop: &AtomicBool) -> Result<(), ()> {
        let event = operation_event()?;
        let mut overlapped = Overlapped {
            event: event.0,
            ..Default::default()
        };
        let immediate = unsafe { ConnectNamedPipe(pipe.0, &mut overlapped) };
        if immediate == 0 && unsafe { GetLastError() } == ERROR_PIPE_CONNECTED {
            return Ok(());
        }
        // Listening has no overall deadline; shutdown cancels the pending connect.
        let mut transferred = 0;
        complete(
            pipe.0,
            &event,
            &mut overlapped,
            immediate,
            &mut transferred,
            Instant::now() + Duration::from_secs(365 * 86400),
            Some(stop),
        )
    }
    fn transfer(
        pipe: &OwnedHandle,
        bytes: &mut [u8],
        write: bool,
        deadline: Instant,
        stop: Option<&AtomicBool>,
    ) -> Result<(), ()> {
        let mut offset = 0;
        while offset < bytes.len() {
            if Instant::now() >= deadline || stop.is_some_and(|s| s.load(Ordering::Acquire)) {
                return Err(());
            }
            let event = operation_event()?;
            let mut overlapped = Overlapped {
                event: event.0,
                ..Default::default()
            };
            let mut transferred = 0;
            let length = (bytes.len() - offset).min(65536) as u32;
            let immediate = unsafe {
                if write {
                    WriteFile(
                        pipe.0,
                        bytes[offset..].as_ptr().cast(),
                        length,
                        &mut transferred,
                        &mut overlapped,
                    )
                } else {
                    ReadFile(
                        pipe.0,
                        bytes[offset..].as_mut_ptr().cast(),
                        length,
                        &mut transferred,
                        &mut overlapped,
                    )
                }
            };
            complete(
                pipe.0,
                &event,
                &mut overlapped,
                immediate,
                &mut transferred,
                deadline,
                stop,
            )?;
            if transferred == 0 {
                return Err(());
            }
            offset += transferred as usize;
        }
        Ok(())
    }
    fn read_frame(
        pipe: &OwnedHandle,
        limit: usize,
        deadline: Instant,
        stop: Option<&AtomicBool>,
    ) -> Result<Vec<u8>, ()> {
        let mut header = [0; 4];
        transfer(pipe, &mut header, false, deadline, stop)?;
        let length = u32::from_le_bytes(header) as usize;
        if length == 0 || length > limit {
            return Err(());
        }
        let mut bytes = vec![0; length];
        transfer(pipe, &mut bytes, false, deadline, stop)?;
        Ok(bytes)
    }
    fn write_frame(
        pipe: &OwnedHandle,
        mut bytes: Vec<u8>,
        deadline: Instant,
        stop: Option<&AtomicBool>,
    ) -> Result<(), ()> {
        let mut header = (bytes.len() as u32).to_le_bytes();
        transfer(pipe, &mut header, true, deadline, stop)?;
        transfer(pipe, &mut bytes, true, deadline, stop)
    }
    fn open_client(name: &str, sid: &str, timeout: Duration) -> Result<OwnedHandle, ()> {
        let deadline = Instant::now() + timeout;
        let name = wide(name);
        loop {
            // IDENTIFICATION prevents a server from impersonating this caller.
            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_READ_WRITE,
                    0,
                    std::ptr::null(),
                    3,
                    FILE_FLAG_OVERLAPPED | 0x00100000 | 0x00010000,
                    std::ptr::null_mut(),
                )
            };
            if handle != INVALID_HANDLE {
                let pipe = owned(handle)?;
                let mut pid = 0;
                if unsafe { GetNamedPipeServerProcessId(pipe.0, &mut pid) } == 0 {
                    return Err(());
                }
                let process = owned(unsafe { OpenProcess(0x1000, 0, pid) })?;
                if user_sid(process.0)? != sid {
                    return Err(());
                }
                return Ok(pipe);
            }
            if unsafe { GetLastError() } != ERROR_PIPE_BUSY || Instant::now() >= deadline {
                return Err(());
            }
            unsafe {
                WaitNamedPipeW(name.as_ptr(), 50);
            }
        }
    }
    fn request_at(name: &str, sid: &str, value: Value) -> Value {
        let Ok(pipe) = open_client(name, sid, Duration::from_secs(2)) else {
            return failure(
                "app_unavailable",
                "Unified Control is not available; open the app from Windows Start",
            );
        };
        let Ok(bytes) = encode(&value, MAX_REQUEST) else {
            return failure(
                "request_limit",
                "The local agent request exceeds its size limit",
            );
        };
        if write_frame(&pipe, bytes, Instant::now() + IO_TIMEOUT, None).is_err() {
            return failure(
                "connection_lost",
                "The local agent connection was interrupted",
            );
        }
        match read_frame(&pipe, MAX_RESPONSE, Instant::now() + RESPONSE_TIMEOUT, None).and_then(|bytes| serde_json::from_slice(&bytes).map_err(|_| ())) {
            Ok(result) => {
                // Let the server close without discarding buffered response
                // bytes. This acknowledgement is bounded, unlike a flush.
                let _ = transfer(&pipe, &mut [1], true, Instant::now() + IO_TIMEOUT, None);
                result
            },
            Err(_) => failure("connection_lost", "The local agent response was interrupted or timed out; read the current state before retrying changes"),
        }
    }
    pub(super) fn request(value: Value) -> Value {
        match identity() {
            Ok((sid, name)) => request_at(&name, &sid, value),
            Err(_) => failure(
                "identity_unavailable",
                "The local Windows user identity is unavailable",
            ),
        }
    }

    pub struct Server {
        stop: Arc<AtomicBool>,
        listener: Option<JoinHandle<()>>,
    }
    impl Server {
        pub fn shutdown(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(listener) = self.listener.take() {
                let _ = listener.join();
            }
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.shutdown();
        }
    }
    struct CountGuard(Arc<AtomicUsize>);
    impl Drop for CountGuard {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::AcqRel);
        }
    }

    fn serve_client(
        pipe: OwnedHandle,
        handler: Handler,
        stop: Arc<AtomicBool>,
        count: Arc<AtomicUsize>,
    ) {
        let _count = CountGuard(count);
        let result = match read_frame(&pipe, MAX_REQUEST, Instant::now() + IO_TIMEOUT, Some(&stop))
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).map_err(|_| ()))
        {
            Ok(value) if valid_request(&value) && !stop.load(Ordering::Acquire) => handler(value),
            _ => failure(
                "invalid_request",
                "The local agent request is invalid, too large or incomplete",
            ),
        };
        let bytes = encode(&result, MAX_RESPONSE).unwrap_or_else(|_| {
            encode(
                &failure(
                    "response_limit",
                    "The local agent result exceeds its size limit",
                ),
                MAX_RESPONSE,
            )
            .unwrap()
        });
        if write_frame(&pipe, bytes, Instant::now() + IO_TIMEOUT, Some(&stop)).is_ok() {
            let _ = transfer(
                &pipe,
                &mut [0],
                false,
                Instant::now() + IO_TIMEOUT,
                Some(&stop),
            );
        }
        // Do not FlushFileBuffers: it can wait forever for a client to read.
    }
    fn start_at(name: String, sid: String, handler: Handler) -> Result<Server, String> {
        let first = create_pipe(&name, &sid, true)
            .map_err(|_| "The local agent connection is already in use or unavailable")?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let listener = thread::Builder::new()
            .name("uc-agent-listener".into())
            .spawn(move || {
                let count = Arc::new(AtomicUsize::new(0));
                let mut pipe = first;
                while !thread_stop.load(Ordering::Acquire) {
                    if connect(&pipe, &thread_stop).is_err() {
                        break;
                    }
                    // Keep one instance alive at all times, preventing another
                    // server from claiming this name between short-lived requests.
                    let Ok(next) = create_pipe(&name, &sid, false) else {
                        break;
                    };
                    let connected = std::mem::replace(&mut pipe, next);
                    if count.load(Ordering::Acquire) >= MAX_CLIENTS {
                        continue;
                    }
                    count.fetch_add(1, Ordering::AcqRel);
                    let handler = handler.clone();
                    let stop = thread_stop.clone();
                    let count_for_thread = count.clone();
                    if thread::Builder::new()
                        .name("uc-agent-request".into())
                        .spawn(move || serve_client(connected, handler, stop, count_for_thread))
                        .is_err()
                    {
                        count.fetch_sub(1, Ordering::AcqRel);
                    }
                }
                unsafe {
                    DisconnectNamedPipe(pipe.0);
                }
            })
            .map_err(|_| "The local agent listener could not be started".to_string())?;
        Ok(Server {
            stop,
            listener: Some(listener),
        })
    }
    pub(super) fn start(handler: Handler) -> Result<Server, String> {
        let (sid, name) =
            identity().map_err(|_| "The local Windows user identity is unavailable")?;
        start_at(name, sid, handler)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        fn isolated() -> (String, String) {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let sid = user_sid(unsafe { GetCurrentProcess() }).unwrap();
            let name = format!(
                r"\\.\pipe\UnifiedControl-Agent-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            (sid, name)
        }
        #[test]
        fn transport_scopes_isolate_installed_debug_and_explicit_homes() {
            let sid = "S-1-5-21-1";
            let release = endpoint_for(sid, false, None);
            assert!(release.ends_with("-installed"));
            assert_ne!(release, endpoint_for(sid, true, None));
            assert_ne!(
                release,
                endpoint_for(sid, false, Some(std::path::Path::new("C:/scratch")))
            );
            assert_ne!(release, endpoint_for("S-1-5-21-2", false, None));
            assert_eq!(
                endpoint_for(sid, true, Some(std::path::Path::new("C:/Scratch/"))),
                endpoint_for(sid, true, Some(std::path::Path::new(r"c:\scratch")))
            );
        }
        #[test]
        fn transport_actual_pipe_dacl_is_protected_and_only_user_and_system() {
            let (sid, name) = isolated();
            let pipe = create_pipe(&name, &sid, true).unwrap();
            let mut descriptor = std::ptr::null_mut();
            assert_eq!(
                unsafe {
                    GetSecurityInfo(
                        pipe.0,
                        6,
                        4,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        &mut descriptor,
                    )
                },
                0
            );
            let descriptor = LocalMemory(descriptor);
            let mut text = std::ptr::null_mut();
            let mut length = 0;
            assert_ne!(
                unsafe {
                    ConvertSecurityDescriptorToStringSecurityDescriptorW(
                        descriptor.0,
                        1,
                        4,
                        &mut text,
                        &mut length,
                    )
                },
                0
            );
            let _text_memory = LocalMemory(text.cast());
            let actual = String::from_utf16(unsafe {
                std::slice::from_raw_parts(text, length as usize - 1)
            })
            .unwrap();
            assert!(actual.starts_with("D:P"));
            assert_eq!(actual.matches("(A;").count(), 2);
            assert!(actual.contains(";;;SY)"));
            assert!(actual.contains(&format!(";;;{sid})")));
        }
        #[test]
        fn transport_roundtrip_first_instance_guard_and_no_host_failure() {
            let (sid, name) = isolated();
            assert_eq!(
                request_at(&name, &sid, json!({"method":"status","params":{}}))["error"]["code"],
                "app_unavailable"
            );
            let handler: Handler = Arc::new(|value| json!({"ok":true,"result":value["params"]}));
            let mut server = start_at(name.clone(), sid.clone(), handler.clone()).unwrap();
            assert!(start_at(name.clone(), sid.clone(), handler).is_err());
            for i in 0..12 {
                assert_eq!(
                    request_at(
                        &name,
                        &sid,
                        json!({"method":"status","params":{"fixture":i}})
                    ),
                    json!({"ok":true,"result":{"fixture":i}})
                );
            }
            let started = Instant::now();
            server.shutdown();
            assert!(started.elapsed() < Duration::from_secs(1));
            assert_eq!(
                request_at(&name, &sid, json!({"method":"status","params":{}}))["error"]["code"],
                "app_unavailable"
            );
        }
        #[test]
        fn transport_bounds_partial_clients_and_shutdown_cancels_io() {
            let (sid, name) = isolated();
            let calls = Arc::new(AtomicUsize::new(0));
            let counter = calls.clone();
            let mut server = start_at(
                name.clone(),
                sid.clone(),
                Arc::new(move |_| {
                    counter.fetch_add(1, Ordering::Relaxed);
                    json!({"ok":true})
                }),
            )
            .unwrap();
            let partial = open_client(&name, &sid, Duration::from_secs(1)).unwrap();
            let timeout_started = Instant::now();
            assert!(read_frame(
                &partial,
                MAX_RESPONSE,
                Instant::now() + Duration::from_millis(40),
                None
            )
            .is_err());
            assert!(timeout_started.elapsed() < Duration::from_millis(500));
            let malformed = open_client(&name, &sid, Duration::from_secs(1)).unwrap();
            write_frame(
                &malformed,
                b"not-json".to_vec(),
                Instant::now() + IO_TIMEOUT,
                None,
            )
            .unwrap();
            let response: Value = serde_json::from_slice(
                &read_frame(&malformed, MAX_RESPONSE, Instant::now() + IO_TIMEOUT, None).unwrap(),
            )
            .unwrap();
            assert_eq!(response["error"]["code"], "invalid_request");
            let oversized = open_client(&name, &sid, Duration::from_secs(1)).unwrap();
            let mut header = ((MAX_REQUEST + 1) as u32).to_le_bytes();
            transfer(
                &oversized,
                &mut header,
                true,
                Instant::now() + IO_TIMEOUT,
                None,
            )
            .unwrap();
            let response: Value = serde_json::from_slice(
                &read_frame(&oversized, MAX_RESPONSE, Instant::now() + IO_TIMEOUT, None).unwrap(),
            )
            .unwrap();
            assert_eq!(response["error"]["code"], "invalid_request");
            assert_eq!(calls.load(Ordering::Relaxed), 0);
            let started = Instant::now();
            server.shutdown();
            assert!(started.elapsed() < Duration::from_secs(1));
            drop(partial);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_request_shape_and_json_size_are_bounded() {
        assert!(valid_request(&json!({"method":"get_status","params":{}})));
        for value in [
            json!({"method":"get_status"}),
            json!({"method":"get_status","params":null}),
            json!({"method":"Get-Process","params":{}}),
            json!({"method":"status","params":{},"extra":true}),
        ] {
            assert!(!valid_request(&value));
        }
        assert!(encode(&json!({"fixture":"x".repeat(50)}), 10).is_err());
        assert_eq!(
            serde_json::from_slice::<Value>(&encode(&json!({"fixture":1}), 64).unwrap()).unwrap(),
            json!({"fixture":1})
        );
    }
}
