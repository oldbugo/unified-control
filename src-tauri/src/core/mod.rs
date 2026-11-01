//! Unified Control core: settings (save on every change), the job scheduler (busy/idle for quit and installs), paths and
//! start-up entry. Copied from CoolerScreen's `core/` (commit in README.md); only the app-shell parts.
pub mod actions;
pub mod app;
pub mod assets;
pub mod autostart;
pub mod buildinfo;
pub mod commands;
pub mod cooler;
pub mod cooler_health;
pub mod cooler_migration;
pub mod cooler_transfer;
pub mod dock_runtime;
pub mod dock_service;
pub mod error;
pub mod jobobject;
pub mod js;
pub mod key_render;
pub mod paths;
pub mod profiles;
pub mod runtime_resources;
pub mod scheduler;
pub mod sensors;
pub mod settings;
pub mod usage;

pub use app::{App, Events};
pub use error::{CoreError, CoreResult};
