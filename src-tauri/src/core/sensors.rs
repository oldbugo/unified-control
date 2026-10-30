//! Caller-driven metrics; no polling thread, process, service or device writes.
//! CPU/RAM logic adapted from CoolerScreen b68d2e8 live/metrics.rs; NVML loading
//! from live/sensors.rs, reduced to utilization with every return code checked.
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Needs {
    pub cpu: bool,
    pub gpu: bool,
    pub ram: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Readings {
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
    pub ram: Option<f32>,
}
pub trait Source {
    fn cpu_times(&mut self) -> Option<(u64, u64)>;
    fn memory_bytes(&mut self) -> Option<(u64, u64)>;
    fn gpu_percent(&mut self, now: Duration) -> Option<f32>;
    fn release_gpu(&mut self) {}
}
/// At most one sample per second, only for requested features. The first CPU
/// observation is unknown: a second reading is needed to measure a real delta.
pub struct Sampler<S: Source = NativeSource> {
    source: S,
    previous_cpu: Option<(u64, u64)>,
    last_at: Option<Duration>,
    cache: Readings,
}
impl Sampler<NativeSource> {
    pub fn new() -> Self {
        Self::with_source(NativeSource::default())
    }
}
impl Default for Sampler<NativeSource> {
    fn default() -> Self {
        Self::new()
    }
}
impl<S: Source> Sampler<S> {
    pub fn with_source(source: S) -> Self {
        Self {
            source,
            previous_cpu: None,
            last_at: None,
            cache: Readings::default(),
        }
    }
    /// `now` is monotonic elapsed time. Turning a feature off releases its source
    /// immediately. Turning it on waits at most one second for the next sample.
    pub fn sample(&mut self, needs: Needs, now: Duration) -> Readings {
        if !needs.cpu {
            self.previous_cpu = None;
            self.cache.cpu = None;
        }
        if !needs.gpu {
            self.source.release_gpu();
            self.cache.gpu = None;
        }
        if !needs.ram {
            self.cache.ram = None;
        }
        if needs == Needs::default() {
            self.last_at = None;
            return Readings::default();
        }
        if self.last_at.is_some_and(|at| now < at) {
            self.previous_cpu = None;
            self.last_at = None;
            self.cache = Readings::default();
        }
        if self
            .last_at
            .is_some_and(|at| now.saturating_sub(at) < Duration::from_secs(1))
        {
            return self.cache;
        }
        self.last_at = Some(now);
        if needs.cpu {
            let current = self.source.cpu_times();
            self.cache.cpu = match (self.previous_cpu, current) {
                (Some((idle0, total0)), Some((idle, total))) if idle >= idle0 && total > total0 => {
                    let (idle, total) = (idle - idle0, total - total0);
                    if idle <= total {
                        Some((100. * (1. - idle as f64 / total as f64)) as f32)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            self.previous_cpu = current;
        }
        if needs.ram {
            self.cache.ram = self.source.memory_bytes().and_then(|(used, total)| {
                if total > 0 && used <= total {
                    Some((used as f64 / total as f64 * 100.) as f32)
                } else {
                    None
                }
            });
        }
        if needs.gpu {
            self.cache.gpu = self
                .source
                .gpu_percent(now)
                .filter(|v| v.is_finite() && (0.0..=100.).contains(v));
        }
        self.cache
    }
}

#[derive(Default)]
pub struct NativeSource {
    #[cfg(windows)]
    gpu: Option<nvidia::Nvml>,
    #[cfg(windows)]
    retry_at: Option<Duration>,
}
impl Source for NativeSource {
    fn cpu_times(&mut self) -> Option<(u64, u64)> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::{Foundation::FILETIME, System::Threading::GetSystemTimes};
            let z = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };
            let (mut idle, mut kernel, mut user) = (z, z, z);
            if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
                return None;
            }
            let f = |v: FILETIME| u64::from(v.dwHighDateTime) << 32 | u64::from(v.dwLowDateTime);
            Some((f(idle), f(kernel).checked_add(f(user))?))
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
    fn memory_bytes(&mut self) -> Option<(u64, u64)> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::SystemInformation::{
                GlobalMemoryStatusEx, MEMORYSTATUSEX,
            };
            let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
            m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 {
                return None;
            }
            Some((m.ullTotalPhys.checked_sub(m.ullAvailPhys)?, m.ullTotalPhys))
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
    fn gpu_percent(&mut self, _now: Duration) -> Option<f32> {
        #[cfg(windows)]
        {
            if self.gpu.is_none() {
                if self.retry_at.is_some_and(|at| _now < at) {
                    return None;
                }
                self.gpu = nvidia::Nvml::load();
                if self.gpu.is_none() {
                    self.retry_at = Some(_now.saturating_add(Duration::from_secs(30)));
                    return None;
                }
            }
            let value = self.gpu.as_ref().and_then(|g| g.read());
            if value.is_none() {
                self.gpu = None;
                self.retry_at = Some(_now.saturating_add(Duration::from_secs(30)));
            }
            value
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
    fn release_gpu(&mut self) {
        #[cfg(windows)]
        {
            self.gpu = None;
            self.retry_at = None;
        }
    }
}

#[cfg(windows)]
mod nvidia {
    use std::ffi::c_void;
    use windows_sys::Win32::{
        Foundation::{FreeLibrary, HMODULE},
        System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32},
    };
    struct Library(HMODULE);
    // The module and NVML handle are owned by a single caller-driven sampler.
    unsafe impl Send for Library {}
    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }
    #[repr(C)]
    struct Utilization {
        gpu: u32,
        memory: u32,
    }
    type UtilFn = unsafe extern "C" fn(*mut c_void, *mut Utilization) -> i32;
    type ShutdownFn = unsafe extern "C" fn() -> i32;
    pub struct Nvml {
        _library: Library,
        device: *mut c_void,
        util: UtilFn,
        shutdown: ShutdownFn,
    }
    unsafe impl Send for Nvml {}
    impl Nvml {
        pub fn load() -> Option<Self> {
            // Restrict lookup to the installed Windows driver directory. Never
            // load a DLL from the app folder, current directory or PATH.
            let name: Vec<u16> = "nvml.dll".encode_utf16().chain(Some(0)).collect();
            let lib = Library(unsafe {
                LoadLibraryExW(
                    name.as_ptr(),
                    std::ptr::null_mut(),
                    LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            });
            if lib.0.is_null() {
                return None;
            }
            unsafe {
                let init: unsafe extern "C" fn() -> i32 =
                    std::mem::transmute(GetProcAddress(lib.0, b"nvmlInit_v2\0".as_ptr())?);
                let shutdown: ShutdownFn =
                    std::mem::transmute(GetProcAddress(lib.0, b"nvmlShutdown\0".as_ptr())?);
                let get_device: unsafe extern "C" fn(u32, *mut *mut c_void) -> i32 =
                    std::mem::transmute(GetProcAddress(
                        lib.0,
                        b"nvmlDeviceGetHandleByIndex_v2\0".as_ptr(),
                    )?);
                let util: UtilFn = std::mem::transmute(GetProcAddress(
                    lib.0,
                    b"nvmlDeviceGetUtilizationRates\0".as_ptr(),
                )?);
                if init() != 0 {
                    return None;
                }
                let mut device = std::ptr::null_mut();
                if get_device(0, &mut device) != 0 || device.is_null() {
                    shutdown();
                    return None;
                }
                Some(Self {
                    _library: lib,
                    device,
                    util,
                    shutdown,
                })
            }
        }
        pub fn read(&self) -> Option<f32> {
            let mut u = Utilization { gpu: 0, memory: 0 };
            if unsafe { (self.util)(self.device, &mut u) } != 0 || u.gpu > 100 {
                return None;
            }
            Some(u.gpu as f32)
        }
    }
    impl Drop for Nvml {
        fn drop(&mut self) {
            unsafe {
                (self.shutdown)();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    struct Fake {
        calls: Rc<Cell<usize>>,
        cpu: Vec<Option<(u64, u64)>>,
        gpu: Option<f32>,
    }
    impl Source for Fake {
        fn cpu_times(&mut self) -> Option<(u64, u64)> {
            self.calls.set(self.calls.get() + 1);
            self.cpu.remove(0)
        }
        fn memory_bytes(&mut self) -> Option<(u64, u64)> {
            self.calls.set(self.calls.get() + 1);
            Some((3, 4))
        }
        fn gpu_percent(&mut self, _: Duration) -> Option<f32> {
            self.calls.set(self.calls.get() + 1);
            self.gpu
        }
    }
    #[test]
    fn demand_throttle_real_delta_and_disabled_sources() {
        let calls = Rc::new(Cell::new(0));
        let mut sampler = Sampler::with_source(Fake {
            calls: calls.clone(),
            cpu: vec![Some((100, 200)), Some((120, 300))],
            gpu: None,
        });
        assert_eq!(
            sampler.sample(Needs::default(), Duration::ZERO),
            Readings::default()
        );
        assert_eq!(calls.get(), 0);
        let needs = Needs {
            cpu: true,
            gpu: true,
            ram: true,
        };
        let first = sampler.sample(needs, Duration::ZERO);
        assert_eq!(first.cpu, None);
        assert_eq!(first.ram, Some(75.));
        assert_eq!(first.gpu, None);
        sampler.sample(needs, Duration::from_millis(500));
        assert_eq!(calls.get(), 3);
        assert_eq!(sampler.sample(needs, Duration::from_secs(1)).cpu, Some(80.));
        assert_eq!(calls.get(), 6);
        assert_eq!(
            sampler.sample(Needs::default(), Duration::from_secs(1)),
            Readings::default()
        );
        assert_eq!(calls.get(), 6);
    }
    #[test]
    fn source_failure_and_counter_reset_stay_unknown() {
        let mut sampler = Sampler::with_source(Fake {
            calls: Rc::new(Cell::new(0)),
            cpu: vec![Some((100, 200)), None, Some((10, 20)), Some((30, 25))],
            gpu: Some(f32::NAN),
        });
        for n in 0..4 {
            let r = sampler.sample(
                Needs {
                    cpu: true,
                    gpu: true,
                    ram: false,
                },
                Duration::from_secs(n),
            );
            assert!(r.cpu.is_none());
            assert!(r.gpu.is_none());
        }
    }
}
