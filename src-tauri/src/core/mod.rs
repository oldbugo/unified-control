//! Unified Control core: settings (save on every change), the job scheduler (busy/idle for quit and installs), paths and
//! start-up entry. Copied from CoolerScreen's `core/` (commit in README.md); only the app-shell parts.
pub mod app;
pub mod autostart;
pub mod buildinfo;
pub mod commands;
pub mod error;
pub mod jobobject;
pub mod js;
pub mod paths;
pub mod scheduler;
pub mod settings;

pub use app::{App, Events};
pub use error::{CoreError, CoreResult};
