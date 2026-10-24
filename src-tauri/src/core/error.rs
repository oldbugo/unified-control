//! The one error type commands return to the UI: `{ error, code, status, failedUsb?, ... }` (copied from CoolerScreen's
//! `core/error.rs`; `status` mirrors an HTTP status code).
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CoreError {
    #[serde(rename = "error")]
    pub message: String,
    pub code: Option<String>,
    pub status: u16,
    #[serde(rename = "failedUsb", skip_serializing_if = "std::ops::Not::not")]
    pub failed_usb: bool,
    /// Extra fields merged into the error body (versionId, maxFitLength, ...).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

pub type CoreResult<T> = Result<T, CoreError>;

impl CoreError {
    pub fn new(message: impl Into<String>) -> Self {
        CoreError { message: message.into(), code: None, status: 400, failed_usb: false, extra: Default::default() }
    }
    pub fn coded(code: &str, message: impl Into<String>) -> Self {
        CoreError { code: Some(code.into()), failed_usb: code == "failedUsb", ..Self::new(message) }
    }
    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }
    pub fn with(mut self, key: &str, v: impl Into<Value>) -> Self {
        self.extra.insert(key.into(), v.into());
        self
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(message).status(404)
    }
    pub fn cancelled() -> Self {
        Self::coded("cancelled", "Cancelled.")
    }
    pub fn superseded() -> Self {
        Self::coded("superseded", "Replaced by a newer request.")
    }
    pub fn is_cancelled(&self) -> bool {
        matches!(self.code.as_deref(), Some("cancelled") | Some("superseded"))
    }
    pub fn code_is(&self, c: &str) -> bool {
        self.code.as_deref() == Some(c)
    }
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.code {
            Some(c) => write!(f, "[{c}] {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        CoreError::new(e.to_string()).status(500)
    }
}
