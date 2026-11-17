//! Unified Control core: settings (save on every change), the job scheduler (busy/idle for quit and installs), paths and
//! start-up entry. Copied from CoolerScreen's `core/` (commit in README.md); only the app-shell parts.
pub mod actions;
pub mod agent;
pub mod app;
pub mod assets;
pub mod audio_capture;
pub mod au05;
pub mod au05_hotkey;
pub mod au05_menu;
pub mod claude_sessions;
pub mod autostart;
pub mod buildinfo;
pub mod commands;
pub mod cooler;
pub mod cooler_controls;
pub mod cooler_health;
pub mod cooler_migration;
pub mod cooler_migration_audit;
pub mod cooler_transfer;
pub mod dock_runtime;
pub mod dock_service;
pub mod dock_worker;
pub mod error;
pub mod frame;
pub mod hardware;
pub mod hardware_bridge;
pub mod input_tester;
pub mod jobobject;
pub mod js;
pub mod key_render;
pub mod paths;
pub mod process;
pub mod profiles;
pub mod runtime_resources;
pub mod scheduler;
pub mod sensors;
pub mod settings;
pub mod usage;

pub use app::{App, Events};
pub use error::{CoreError, CoreResult};

pub mod shared_sources;
pub mod shared_usage;
pub mod sync;
