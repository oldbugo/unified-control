//! Named cross-device actions only. No shell, raw device protocol or arbitrary
//! worker command may be stored in a dock profile or executed by this adapter.
use super::{
    actions::CoolerAction,
    cooler::Cooler,
    error::{CoreError, CoreResult},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Theme {
    id: String,
    name: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Brightness {
    reported: Option<u8>,
    requested: Option<u8>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ControlState {
    themes: Vec<Theme>,
    current_theme_id: Option<String>,
    embedded_theme_id: Option<String>,
    live_enabled: bool,
    running: bool,
    connected: bool,
    paused: bool,
    device_hold: bool,
    system_hold: bool,
    brightness: Brightness,
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}
fn validate(value: Value) -> CoreResult<Value> {
    let state: ControlState = serde_json::from_value(value).map_err(|_| {
        CoreError::coded(
            "cooler_control_state",
            "The cooler control state is unavailable.",
        )
    })?;
    let mut ids = HashSet::new();
    if state.themes.len() > 4096
        || state.themes.iter().any(|theme| {
            !valid_id(&theme.id)
                || !ids.insert(theme.id.clone())
                || theme.name.chars().count() > 256
                || theme.name.chars().any(char::is_control)
        })
        || [&state.current_theme_id, &state.embedded_theme_id]
            .into_iter()
            .flatten()
            .any(|id| !valid_id(id))
        || [state.brightness.reported, state.brightness.requested]
            .into_iter()
            .flatten()
            .any(|value| value > 100)
    {
        return Err(CoreError::coded(
            "cooler_control_state",
            "The cooler control state is unavailable.",
        ));
    }
    let mut value = serde_json::to_value(state)
        .map_err(|_| CoreError::new("The cooler control state is unavailable."))?;
    value["available"] = json!(true);
    Ok(value)
}
pub fn unavailable() -> Value {
    json!({"available":false,"themes":[],"currentThemeId":null,"embeddedThemeId":null,
        "liveEnabled":false,"running":false,"connected":false,"paused":false,"deviceHold":false,"systemHold":false,
        "brightness":{"reported":null,"requested":null}})
}
/// Cached/library inspection only: never enables a device or reads hardware.
pub fn state(cooler: &Cooler) -> CoreResult<Value> {
    let status = cooler.status();
    if status["enabled"] != true || status["state"] != "running" {
        return Ok(unavailable());
    }
    validate(cooler.controls_state()?)
}
/// Admission/serialization is owned by App and the cooler's device scheduler.
/// A failure is returned to the original key press; it is never retried here.
pub fn execute(cooler: &Cooler, action: CoolerAction) -> CoreResult<()> {
    action.validate().map_err(CoreError::new)?;
    let result = cooler.dock_action(action)?;
    validate(result)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_state_is_bounded_typed_and_keeps_reported_brightness_distinct() {
        let mut state = unavailable();
        state.as_object_mut().unwrap().remove("available");
        state["themes"] = json!([{"id":"theme-a","name":"Alpha"}]);
        state["brightness"] = json!({"reported":null,"requested":35});
        let validated = validate(state.clone()).unwrap();
        assert!(validated["brightness"]["reported"].is_null());
        assert_eq!(validated["brightness"]["requested"], 35);
        state["themes"][0]["path"] = json!("forbidden");
        assert!(validate(state.clone()).is_err());
        state["themes"][0].as_object_mut().unwrap().remove("path");
        state["themes"][0]["id"] = json!("../theme");
        assert!(validate(state.clone()).is_err());
        state["themes"][0]["id"] = json!("theme-a");
        state["brightness"]["reported"] = json!(101);
        assert!(validate(state).is_err());
    }
}
