//! Device layer. Phase 0 has only the diagnostic log (copied from CoolerScreen's `device/log.rs`, where the scheduler and the
//! shell write); the dock driver lives in the `uc-ulanzi` repository and is added in phase 1.
pub mod log;

pub use log::log;
