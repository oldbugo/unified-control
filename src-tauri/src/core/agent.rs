//! Agent-neutral, validated host operations. No device actions, shell execution,
//! authentication files or direct client-side storage access are exposed here.
use super::{
    app::App,
    error::{CoreError, CoreResult},
    profiles::{Face, Key, Page, ProfileState},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Condvar, Mutex,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const METHODS: &[&str] = &[
    "get_status",
    "get_layout",
    "list_monitors",
    "preview_changes",
    "apply_changes",
    "get_job",
    "undo_changes",
    "render_page",
];
const LEASE_WAIT: Duration = Duration::from_secs(8);
#[derive(Default)]
struct Control {
    editor_present: bool,
    lease: Option<Lease>,
}
struct Lease {
    id: String,
    saved: Option<bool>,
}
struct Undo {
    token: String,
    revision: String,
    state: ProfileState,
}
#[derive(Default)]
pub struct AgentState {
    serial: Mutex<()>,
    control: Mutex<Control>,
    changed: Condvar,
    undo: Mutex<Option<Undo>>,
}
fn token(prefix: &str) -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{prefix}-{:x}-{at:x}-{:x}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}
fn invalid(message: &str) -> CoreError {
    CoreError::coded("invalid-arguments", message)
}
fn busy() -> CoreError {
    CoreError::coded(
        "busy",
        "The editor or another operation is busy; retry when it has finished.",
    )
    .status(409)
}
fn conflict() -> CoreError {
    CoreError::coded(
        "revision_conflict",
        "Dock layout changed; read the latest revision and retry",
    )
    .status(409)
}
fn parse<T: for<'de> Deserialize<'de>>(value: Value) -> CoreResult<T> {
    serde_json::from_value(value)
        .map_err(|_| invalid("Arguments do not match this operation's schema."))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Scope {
    profile_id: Option<String>,
    page_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Changes {
    expected_revision: String,
    ops: Vec<Operation>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UndoRequest {
    expected_revision: String,
    undo_token: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JobRequest {
    job_id: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Side {
    A,
    B,
}
#[derive(Clone, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
enum Operation {
    #[serde(rename = "set_face")]
    SetFace {
        #[serde(rename = "profileId")]
        profile_id: String,
        #[serde(rename = "pageId")]
        page_id: String,
        key: usize,
        face: Side,
        value: Face,
    },
    #[serde(rename = "set_action")]
    SetAction {
        #[serde(rename = "profileId")]
        profile_id: String,
        #[serde(rename = "pageId")]
        page_id: String,
        key: usize,
        action: String,
        value: String,
    },
    #[serde(rename = "set_key")]
    SetKey {
        #[serde(rename = "profileId")]
        profile_id: String,
        #[serde(rename = "pageId")]
        page_id: String,
        key: usize,
        value: Key,
    },
    #[serde(rename = "rename_page")]
    RenamePage {
        #[serde(rename = "profileId")]
        profile_id: String,
        #[serde(rename = "pageId")]
        page_id: String,
        name: String,
    },
    #[serde(rename = "create_page")]
    CreatePage {
        #[serde(rename = "profileId")]
        profile_id: String,
        name: String,
    },
}
fn page_mut<'a>(
    state: &'a mut ProfileState,
    profile_id: &str,
    page_id: &str,
) -> CoreResult<&'a mut Page> {
    state
        .profiles
        .iter_mut()
        .find(|p| p.id == profile_id)
        .and_then(|p| p.pages.iter_mut().find(|p| p.id == page_id))
        .ok_or_else(|| invalid("The requested profile or page does not exist."))
}
fn validate_action(action: &str, value: &str) -> CoreResult<()> {
    if ["none", "page", "folder", "back"].contains(&action) {
        return Ok(());
    }
    super::actions::plan_action(&super::dock_runtime::PendingAction {
        kind: action.into(),
        value: value.into(),
    })
    .map(|_| ())
    .map_err(|_| invalid("The key action configuration is invalid."))
}
fn patch(before: &ProfileState, ops: &[Operation]) -> CoreResult<(ProfileState, Value)> {
    if ops.is_empty() || ops.len() > 64 {
        return Err(invalid("Supply between 1 and 64 layout operations."));
    }
    let mut state = before.clone();
    let mut summary = Vec::new();
    let mut created = Vec::new();
    for op in ops {
        let key_index = |number: usize| {
            number
                .checked_sub(1)
                .filter(|n| *n < 14)
                .ok_or_else(|| invalid("Key numbers must be between 1 and 14."))
        };
        match op {
            Operation::SetFace {
                profile_id,
                page_id,
                key,
                face,
                value,
            } => {
                let key_value =
                    &mut page_mut(&mut state, profile_id, page_id)?.keys[key_index(*key)?];
                match face {
                    Side::A => key_value.a = value.clone(),
                    Side::B => key_value.b = value.clone(),
                }
                summary.push(json!({"op":"set_face","profileId":profile_id,"pageId":page_id,"key":key,"face":match face{Side::A=>"a",Side::B=>"b"}}));
            }
            Operation::SetAction {
                profile_id,
                page_id,
                key,
                action,
                value,
            } => {
                validate_action(action, value)?;
                let key_value =
                    &mut page_mut(&mut state, profile_id, page_id)?.keys[key_index(*key)?];
                key_value.action = action.clone();
                key_value.value = value.clone();
                summary.push(
                    json!({"op":"set_action","profileId":profile_id,"pageId":page_id,"key":key}),
                );
            }
            Operation::SetKey {
                profile_id,
                page_id,
                key,
                value,
            } => {
                validate_action(&value.action, &value.value)?;
                page_mut(&mut state, profile_id, page_id)?.keys[key_index(*key)?] = value.clone();
                summary.push(
                    json!({"op":"set_key","profileId":profile_id,"pageId":page_id,"key":key}),
                );
            }
            Operation::RenamePage {
                profile_id,
                page_id,
                name,
            } => {
                page_mut(&mut state, profile_id, page_id)?.name = name.clone();
                summary.push(json!({"op":"rename_page","profileId":profile_id,"pageId":page_id}));
            }
            Operation::CreatePage { profile_id, name } => {
                let profile = state
                    .profiles
                    .iter_mut()
                    .find(|p| p.id == *profile_id)
                    .ok_or_else(|| invalid("The requested profile does not exist."))?;
                let page = Page::new(name);
                created.push(json!({"profileId":profile_id,"pageId":page.id,"name":page.name}));
                profile.pages.push(page);
                summary.push(json!({"op":"create_page","profileId":profile_id}));
            }
        }
    }
    state.validate().map_err(|_| {
        invalid("The proposed layout violates a profile, page, face or action constraint.")
    })?;
    Ok((
        state,
        json!({"operations":summary,"createdPages":created,"physicalActionsExecuted":false}),
    ))
}
fn metadata(state: &ProfileState) -> Value {
    json!(state.profiles.iter().map(|p|json!({"id":p.id,"name":p.name,"pages":p.pages.iter().map(|pg|json!({"id":pg.id,"name":pg.name})).collect::<Vec<_>>()})).collect::<Vec<_>>())
}
fn scoped<'a>(
    state: &'a ProfileState,
    scope: &Scope,
) -> CoreResult<(&'a super::profiles::Profile, &'a Page)> {
    let profile = state
        .profiles
        .iter()
        .find(|p| {
            p.id == scope
                .profile_id
                .as_deref()
                .unwrap_or(&state.current_profile)
        })
        .ok_or_else(|| invalid("The requested profile does not exist."))?;
    let page_id = scope
        .page_id
        .as_deref()
        .unwrap_or(if profile.id == state.current_profile {
            &state.current_page
        } else {
            &profile.pages[0].id
        });
    let page = profile
        .pages
        .iter()
        .find(|p| p.id == page_id)
        .ok_or_else(|| invalid("The requested page does not exist."))?;
    Ok((profile, page))
}
struct EditorLease<'a> {
    app: &'a App,
    id: String,
}
impl Drop for EditorLease<'_> {
    fn drop(&mut self) {
        let mut control = self.app.agent.control.lock().unwrap();
        if control
            .lease
            .as_ref()
            .is_some_and(|lease| lease.id == self.id)
        {
            control.lease = None;
            self.app.agent.changed.notify_all();
        }
        drop(control);
        self.app
            .events
            .agent("released", json!({"leaseId":self.id}));
    }
}
impl App {
    fn agent_enabled(&self) -> bool {
        self.settings.load().0["ui"]["agentAccess"]
            .as_bool()
            .unwrap_or(false)
    }
    pub fn agent_busy(&self) -> bool {
        self.agent.control.lock().unwrap().lease.is_some()
    }
    pub fn agent_begin_close(&self) -> CoreResult<()> {
        let control = self.agent.control.lock().unwrap();
        if control.lease.is_some() {
            return Err(busy());
        }
        self.closing.store(true, Ordering::SeqCst);
        Ok(())
    }
    pub fn agent_editor_present(&self, present: bool) -> CoreResult<Value> {
        let mut control = self.agent.control.lock().unwrap();
        control.editor_present = present;
        if !present {
            if let Some(lease) = &mut control.lease {
                lease.saved = Some(false);
            }
            self.agent.changed.notify_all();
        }
        Ok(json!({"present":present}))
    }
    pub fn agent_editor_ready(&self, lease_id: String, saved: bool) -> CoreResult<Value> {
        let mut control = self.agent.control.lock().unwrap();
        let lease = control
            .lease
            .as_mut()
            .filter(|lease| lease.id == lease_id)
            .ok_or_else(|| {
                CoreError::coded("stale-lease", "This editor request is no longer current.")
                    .status(409)
            })?;
        if lease.saved.is_some() {
            return Err(CoreError::coded(
                "stale-lease",
                "The editor already answered this request.",
            )
            .status(409));
        }
        lease.saved = Some(saved);
        self.agent.changed.notify_all();
        Ok(json!({"accepted":true}))
    }
    fn agent_lease(&self) -> CoreResult<EditorLease<'_>> {
        let mut control = self.agent.control.lock().unwrap();
        if self.closing.load(Ordering::SeqCst) || control.lease.is_some() {
            return Err(busy());
        }
        let id = token("lease");
        let present = control.editor_present;
        control.lease = Some(Lease {
            id: id.clone(),
            saved: if present { None } else { Some(true) },
        });
        drop(control);
        let guard = EditorLease {
            app: self,
            id: id.clone(),
        };
        if present {
            self.events.agent("prepare", json!({"leaseId":id}));
            let control = self.agent.control.lock().unwrap();
            let (control, timeout) = self
                .agent
                .changed
                .wait_timeout_while(control, LEASE_WAIT, |c| {
                    c.lease
                        .as_ref()
                        .is_some_and(|lease| lease.id == id && lease.saved.is_none())
                })
                .unwrap();
            let saved = control
                .lease
                .as_ref()
                .filter(|lease| lease.id == id)
                .and_then(|lease| lease.saved);
            drop(control);
            if timeout.timed_out() || saved != Some(true) {
                return Err(busy());
            }
        }
        if !self.agent_enabled() {
            return Err(CoreError::coded(
                "agent-disabled",
                "Agent access is disabled.",
            ));
        }
        Ok(guard)
    }
    /// The sole application entry point used by an attached agent adapter.
    pub fn agent_call(&self, method: &str, params: Value) -> CoreResult<Value> {
        if !params.is_object()
            || serde_json::to_vec(&params).map_or(true, |v| v.len() > 1024 * 1024)
        {
            return Err(invalid("Arguments must be an object no larger than 1 MiB."));
        }
        if method == "get_status" {
            let _: Empty = parse(params)?;
            let enabled = self.agent_enabled();
            let mut status = json!({"protocolVersion":1,"appVersion":self.app_version,"agentAccess":enabled,"capabilities":{"methods":METHODS,"layoutWrites":enabled,"physicalActions":false,"directStorageAccess":false}});
            if enabled {
                status["agentBusy"] = json!(self.agent_busy());
                let (state, revision) = self
                    .profiles
                    .state_with_revision()
                    .map_err(|_| CoreError::coded("profiles", "Dock layout is unavailable."))?;
                let dock = self.dock_state();
                let cooler = self.cooler_state();
                status["revision"] = json!(revision);
                status["profiles"] = metadata(&state);
                status["dock"] = json!({"enabled":dock["enabled"],"state":dock["state"],"currentProfile":dock["currentProfile"],"currentPage":dock["currentPage"],"usageStatus":dock["usageStatus"],"timing":dock["timing"]});
                status["cooler"] = cooler_status(&cooler);
            }
            return Ok(status);
        }
        if !self.agent_enabled() {
            return Err(CoreError::coded(
                "agent-disabled",
                "Enable Agent access in Unified Control before using this operation.",
            )
            .status(403));
        }
        if !METHODS.contains(&method) {
            return Err(CoreError::coded(
                "method-not-found",
                "This agent operation is not supported.",
            )
            .status(404));
        }
        match method {
            "get_layout" => {
                let scope: Scope = parse(params)?;
                let (state, revision) = self.agent_snapshot()?;
                let (profile, page) = scoped(&state, &scope)?;
                Ok(
                    json!({"revision":revision,"profileId":profile.id,"profileName":profile.name,"pageId":page.id,"pageName":page.name,"keys":page.keys,"profiles":metadata(&state)}),
                )
            }
            "list_monitors" => {
                let _: Empty = parse(params)?;
                let runtime = self.runtime.get().ok_or_else(|| {
                    CoreError::coded("runtime-unavailable", "The app runtime is unavailable.")
                })?;
                if runtime.fake {
                    return Ok(json!([{"key":"fake:display-1","label":"Simulated display"}]));
                }
                serde_json::to_value(
                    super::actions::list_monitors(&runtime.resource_dir).map_err(|_| {
                        CoreError::coded(
                            "monitor-unavailable",
                            "Monitor discovery is unavailable; use Retry monitor detection in UC.",
                        )
                    })?,
                )
                .map_err(|_| invalid("Monitor information is unavailable."))
            }
            "preview_changes" => {
                let changes: Changes = parse(params)?;
                let (before, revision) = self.agent_snapshot()?;
                if changes.expected_revision != revision {
                    return Err(conflict());
                }
                let (state, diff) = patch(&before, &changes.ops)?;
                Ok(json!({"revision":revision,"state":state,"diff":diff,"saved":false}))
            }
            "apply_changes" => self.agent_apply(parse(params)?),
            "undo_changes" => self.agent_undo(parse(params)?),
            "get_job" => {
                let request: JobRequest = parse(params)?;
                if request.job_id.len() > 128 {
                    return Err(invalid("Invalid job ID."));
                }
                let job = self
                    .sched
                    .job(&request.job_id)
                    .ok_or_else(|| CoreError::not_found("No such job."))?;
                Ok(
                    json!({"id":job.id,"status":job.status,"state":job.state,"progress":job.progress,"finishedAt":job.finished_at,"failed":job.error.is_some()}),
                )
            }
            "render_page" => {
                let scope: Scope = parse(params)?;
                let (state, revision) = self.agent_snapshot()?;
                let (profile, page) = scoped(&state, &scope)?;
                let mut frames = Vec::new();
                for (index, key) in page.keys.iter().enumerate() {
                    let mut input = super::key_render::RenderInputs {
                        icon: key.a.icon.clone(),
                        ..Default::default()
                    };
                    if let (Some(runtime), Some(asset)) = (self.runtime.get(), &key.a.asset) {
                        input.picture = runtime.assets.get(asset).ok();
                    }
                    let frame = super::key_render::render_face(
                        &key.a.kind,
                        &key.a.text,
                        index == 13,
                        &input,
                    )
                    .map_err(|_| {
                        CoreError::coded(
                            "render-unavailable",
                            "The page preview could not be rendered.",
                        )
                    })?;
                    frames.push(json!({"key":index+1,"width":frame.width,"height":frame.height,"png":frame.png,"unknown":frame.unknown}));
                }
                Ok(
                    json!({"revision":revision,"profileId":profile.id,"pageId":page.id,"face":"a","previewMode":"static-no-account-or-sensor-read","frames":frames}),
                )
            }
            _ => unreachable!(),
        }
    }
    fn agent_snapshot(&self) -> CoreResult<(ProfileState, String)> {
        self.profiles
            .state_with_revision()
            .map_err(|_| CoreError::coded("profiles", "Dock layout is unavailable."))
    }
    fn agent_apply(&self, changes: Changes) -> CoreResult<Value> {
        let _serial = self.agent.serial.try_lock().map_err(|_| busy())?;
        let lease = self.agent_lease()?;
        let (before, revision) = self.agent_snapshot()?;
        if revision != changes.expected_revision {
            return Err(conflict());
        }
        let (state, diff) = patch(&before, &changes.ops)?;
        let result = self.save_profiles_if_revision(state, revision)?;
        let revision = result["revision"].as_str().unwrap().to_string();
        let undo_token = token("undo");
        *self.agent.undo.lock().unwrap() = Some(Undo {
            token: undo_token.clone(),
            revision: revision.clone(),
            state: before,
        });
        self.events.agent(
            "committed",
            json!({"leaseId":lease.id,"state":result["state"],"revision":revision}),
        );
        Ok(
            json!({"state":result["state"],"revision":revision,"diff":diff,"undoToken":undo_token,"saved":true,"physicalActionsExecuted":false}),
        )
    }
    fn agent_undo(&self, request: UndoRequest) -> CoreResult<Value> {
        let _serial = self.agent.serial.try_lock().map_err(|_| busy())?;
        let lease = self.agent_lease()?;
        let state = {
            let undo = self.agent.undo.lock().unwrap();
            let undo = undo
                .as_ref()
                .filter(|undo| undo.token == request.undo_token)
                .ok_or_else(|| {
                    CoreError::coded(
                        "undo-unavailable",
                        "This undo token is not the last agent write.",
                    )
                })?;
            if undo.revision != request.expected_revision {
                return Err(conflict());
            }
            undo.state.clone()
        };
        let result = self.save_profiles_if_revision(state, request.expected_revision)?;
        *self.agent.undo.lock().unwrap() = None;
        self.events.agent(
            "committed",
            json!({"leaseId":lease.id,"state":result["state"],"revision":result["revision"]}),
        );
        Ok(
            json!({"state":result["state"],"revision":result["revision"],"saved":true,"physicalActionsExecuted":false}),
        )
    }
}

fn cooler_status(cooler: &Value) -> Value {
    json!({"enabled":cooler["enabled"],"state":cooler["state"],"busy":cooler["busy"],"engineVersion":cooler["engineVersion"],
        "diagnosticsEnabled":cooler["diagnosticsEnabled"]==true,
        "lastHealthSample":super::cooler_health::agent_health(&cooler["health"]),
        "migration":migration_status(&cooler["migration"])})
}

fn migration_status(value: &Value) -> Value {
    let state = value["state"]
        .as_str()
        .filter(|v| {
            [
                "complete",
                "attention",
                "checking",
                "not_checked",
                "not_required",
            ]
            .contains(v)
        })
        .unwrap_or("not_checked");
    let code = value["code"]
        .as_str()
        .filter(|v| {
            [
                "verified",
                "incomplete",
                "changed_during_audit",
                "source_missing",
                "audit_failed",
                "receipt_write_failed",
                "migration_setup_failed",
                "new_setup",
                "sources_pending",
                "verification_cancelled",
            ]
            .contains(v)
        })
        .unwrap_or("not_checked");
    let mut result = json!({"state":state,"code":code,"verified":state=="complete"&&value["verified"]==true,
        "migrationVersion":value["migrationVersion"].as_u64().unwrap_or(0),"originalKept":value["originalKept"]==true});
    if let Some(timestamp) = value["checkedAt"].as_str().filter(|s| {
        s.len() == 24
            && s.is_ascii()
            && s.as_bytes()[4] == b'-'
            && s.as_bytes()[7] == b'-'
            && s.as_bytes()[10] == b'T'
            && s.ends_with('Z')
            && s.bytes()
                .all(|b| b.is_ascii_digit() || b"-T:.Z".contains(&b))
    }) {
        result["checkedAt"] = json!(timestamp);
    }
    for section in ["library", "gallery"] {
        if value[section].is_object() {
            let mut counts = json!({});
            for key in [
                "sourceFiles",
                "targetFiles",
                "sourceBytes",
                "targetBytes",
                "missingFiles",
                "additionalFiles",
                "identicalFiles",
                "changedRecords",
                "changedMediaFiles",
                "changedPreviews",
                "originals",
                "themes",
                "versions",
            ] {
                counts[key] = json!(value[section][key].as_u64().unwrap_or(0));
            }
            result[section] = counts;
        }
    }
    if value["references"].is_object() {
        for key in ["checked", "oldDependencies", "missing", "external"] {
            result["references"][key] = json!(value["references"][key].as_u64().unwrap_or(0));
        }
    }
    for key in ["unreadableRecords", "unstableFiles"] {
        if let Some(count) = value[key].as_u64() {
            result[key] = json!(count);
        }
    }
    if value["settings"].is_object() {
        result["settings"] = json!({"sourcePresent":value["settings"]["sourcePresent"]==true,"targetPresent":value["settings"]["targetPresent"]==true,"changedSections":value["settings"]["changedSections"].as_u64().unwrap_or(0)});
    }
    if value["lyricsOffsets"].is_object() {
        result["lyricsOffsets"] = json!({"sourcePresent":value["lyricsOffsets"]["sourcePresent"]==true,"targetPresent":value["lyricsOffsets"]["targetPresent"]==true,"changed":value["lyricsOffsets"]["changed"].as_u64().unwrap_or(0)});
    }
    if let Some(count) = value["companionChanged"].as_u64() {
        result["companionChanged"] = json!(count);
    }
    if value["sourcePlugins"].is_object() {
        result["sourcePlugins"] = json!({"imported":value["sourcePlugins"]["imported"]==true,"folderPresent":value["sourcePlugins"]["folderPresent"]==true,"copiedFilesAtImport":value["sourcePlugins"]["copiedFilesAtImport"].as_u64().unwrap_or(0)});
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Weak};
    fn app() -> (tempfile::TempDir, Arc<App>) {
        let dir = tempfile::tempdir().unwrap();
        let app = App::new(dir.path(), Arc::new(super::super::app::NoEvents), "fixture");
        app.settings
            .update(|s| s["ui"]["agentAccess"] = json!(true))
            .unwrap();
        (dir, app)
    }
    fn changes(app: &App, key: usize, text: &str) -> Value {
        let layout = app.agent_call("get_layout", json!({})).unwrap();
        json!({"expectedRevision":layout["revision"],"ops":[{"op":"set_face","profileId":layout["profileId"],"pageId":layout["pageId"],"key":key,"face":"b","value":{"kind":"text","text":text}}]})
    }
    #[test]
    fn agent_off_only_exposes_version_and_capabilities() {
        let (_dir, app) = app();
        app.settings_update(&json!({"agentAccess":false})).unwrap();
        let status = app.agent_call("get_status", json!({})).unwrap();
        assert_eq!(status["agentAccess"], false);
        assert!(status.get("profiles").is_none());
        assert!(status.get("dock").is_none());
        assert!(status.get("cooler").is_none());
        assert_eq!(
            app.agent_call("get_layout", json!({}))
                .unwrap_err()
                .code
                .as_deref(),
            Some("agent-disabled")
        );
        assert!(app
            .agent_call("get_status", json!({"unknown":true}))
            .is_err());
    }

    #[test]
    fn migration_status_only_exposes_finite_cached_counts() {
        let value = json!({"state":"complete","verified":true,"code":"verified","migrationVersion":2,
            "originalKept":true,"checkedAt":"2026-11-03T11:00:00.000Z","privatePath":"secret",
            "library":{"sourceFiles":286,"targetFiles":286,"raw":"secret"},
            "references":{"oldDependencies":0,"details":"secret"},
            "sourcePlugins":{"imported":true,"folderPresent":true,"copiedFilesAtImport":0,"path":"secret"}});
        let public = migration_status(&value);
        assert_eq!(public["verified"], true);
        assert_eq!(public["library"]["sourceFiles"], 286);
        assert!(!public.to_string().contains("secret"));
        assert_eq!(
            migration_status(&json!({"state":"secret","verified":true,"checkedAt":"secret"}))
                ["verified"],
            false
        );
        assert!(migration_status(&json!({"checkedAt":"secret"}))
            .get("checkedAt")
            .is_none());
    }
    #[test]
    fn cooler_status_keeps_current_worker_separate_from_cached_health_and_read_is_inert() {
        let source = json!({"enabled":true,"state":"starting","busy":false,"engineVersion":"4.18.1",
            "pid":123,"message":"secret-worker-error","diagnosticsEnabled":true,
            "health":{"state":"failed","code":"descriptor_failure","lastSampleAt":"2026-11-02T02:00:00.000Z",
                "pnpSampledAt":"2026-11-02T01:59:40.000Z","worker":{"state":"running","ready":true,"generation":1},
                "live":{"running":true,"connected":false,"connectionState":"failedUsb","code":"descriptor_failure"},
                "pnp":{"available":true,"normalNodes":0,"problemCodes":[43],"exactMatchedDescriptorCount":1}}});
        let projected = cooler_status(&source);
        assert_eq!(projected["state"], "starting");
        assert_eq!(projected["lastHealthSample"]["worker"]["state"], "running");
        assert_eq!(projected["lastHealthSample"]["code"], "descriptor_failure");
        assert!(!projected.to_string().contains("secret"));
        assert!(projected.get("pid").is_none());
        let (dir, app) = app();
        let before = app.settings.load();
        let before_bytes = std::fs::read(app.settings.file()).unwrap();
        let first = app.agent_call("get_status", json!({})).unwrap();
        let second = app.agent_call("get_status", json!({})).unwrap();
        assert_eq!(first["cooler"], second["cooler"]);
        assert!(
            app.cooler.get().is_none(),
            "status must not start a worker or diagnostics"
        );
        assert!(
            app.runtime.get().is_none(),
            "status must not start account readers"
        );
        assert!(!dir.path().join("devices/cooler").exists());
        assert_eq!(app.settings.load(), before);
        assert_eq!(std::fs::read(app.settings.file()).unwrap(), before_bytes);
        app.settings_update(&json!({"agentAccess":false})).unwrap();
        assert!(app
            .agent_call("get_status", json!({}))
            .unwrap()
            .get("cooler")
            .is_none());
    }
    #[test]
    fn timing_status_is_numerical_and_access_gated_without_activating_sources() {
        let (dir, app) = app();
        let runtime = super::super::runtime_resources::RuntimeResources::new(
            dir.path(),
            dir.path().join("missing-resources"),
            true,
        );
        let controller = Arc::new(super::super::dock_service::Controller::new(
            app.profiles.clone(),
            runtime.clone(),
            true,
        ));
        assert!(app.dock.set(controller.clone()).is_ok());
        let status = app.agent_call("get_status", json!({})).unwrap();
        let timing = &status["dock"]["timing"];
        fn numerical(value: &Value) -> bool {
            match value {
                Value::Null | Value::Number(_) => true,
                Value::Object(fields) => fields.values().all(numerical),
                _ => false,
            }
        }
        assert!(timing.is_object() && numerical(timing));
        assert_eq!(timing["navigationsStarted"], 0);
        assert_eq!(status["dock"]["enabled"], false);
        assert_eq!(runtime.usage_status()["reader"]["code"], "inactive");
        app.settings_update(&json!({"agentAccess":false})).unwrap();
        assert!(app
            .agent_call("get_status", json!({}))
            .unwrap()
            .get("dock")
            .is_none());
        controller.shutdown();
    }
    #[test]
    fn preview_is_inert_apply_is_atomic_and_undo_requires_latest_revision() {
        let (_dir, app) = app();
        let (before, revision) = app.profiles.state_with_revision().unwrap();
        let request = changes(&app, 14, "Wide B");
        let preview = app.agent_call("preview_changes", request.clone()).unwrap();
        assert_eq!(preview["saved"], false);
        assert_eq!(
            app.profiles.state_with_revision().unwrap(),
            (before.clone(), revision.clone())
        );
        let applied = app.agent_call("apply_changes", request.clone()).unwrap();
        assert_ne!(applied["revision"], revision);
        assert_eq!(
            applied["state"]["profiles"][0]["pages"][0]["keys"][13]["b"]["text"],
            "Wide B"
        );
        assert_eq!(applied["physicalActionsExecuted"], false);
        assert_eq!(
            app.agent_call("apply_changes", request)
                .unwrap_err()
                .code
                .as_deref(),
            Some("revision_conflict")
        );
        let undone = app
            .agent_call(
                "undo_changes",
                json!({"expectedRevision":applied["revision"],"undoToken":applied["undoToken"]}),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_value::<ProfileState>(undone["state"].clone()).unwrap(),
            before
        );
        assert!(app
            .agent_call(
                "undo_changes",
                json!({"expectedRevision":undone["revision"],"undoToken":applied["undoToken"]})
            )
            .is_err());
    }
    #[test]
    fn later_ui_save_rejects_agent_undo_and_old_ui_revision() {
        let (_dir, app) = app();
        let original = app.profiles_state().unwrap();
        let applied = app
            .agent_call("apply_changes", changes(&app, 1, "Agent"))
            .unwrap();
        let mut state = app.profiles.state().unwrap();
        state.profiles[0].pages[0].name = "User edit".into();
        let saved = app
            .save_profiles_if_revision(state, applied["revision"].as_str().unwrap().into())
            .unwrap();
        assert_eq!(
            app.agent_call(
                "undo_changes",
                json!({"expectedRevision":applied["revision"],"undoToken":applied["undoToken"]})
            )
            .unwrap_err()
            .code
            .as_deref(),
            Some("revision_conflict")
        );
        assert_eq!(app.profiles_state().unwrap()["revision"], saved["revision"]);
        let old: ProfileState = serde_json::from_value(original["state"].clone()).unwrap();
        assert_eq!(
            app.save_profiles_if_revision(old, original["revision"].as_str().unwrap().into())
                .unwrap_err()
                .code
                .as_deref(),
            Some("revision_conflict")
        );
        assert_eq!(
            app.profiles.state().unwrap().profiles[0].pages[0].name,
            "User edit"
        );
    }
    #[test]
    fn invalid_or_oversized_operations_never_partially_save() {
        let (_dir, app) = app();
        let before = app.profiles_state().unwrap();
        let mut request = changes(&app, 1, "valid first");
        let mut invalid_op = request["ops"][0].clone();
        invalid_op["key"] = json!(0);
        request["ops"].as_array_mut().unwrap().push(invalid_op);
        assert!(app.agent_call("apply_changes", request).is_err());
        assert_eq!(app.profiles_state().unwrap(), before);
        let mut request = changes(&app, 15, "invalid");
        assert!(app.agent_call("apply_changes", request.clone()).is_err());
        request["ops"][0]["key"] = json!(1);
        request["ops"][0]["extra"] = json!(true);
        assert!(app.agent_call("preview_changes", request).is_err());
        let mut request = changes(&app, 1, "bounded");
        request["ops"] = json!(vec![request["ops"][0].clone(); 65]);
        assert!(app.agent_call("preview_changes", request).is_err());
        assert!(app.agent_call("execute_action", json!({})).is_err());
        assert_eq!(app.profiles_state().unwrap(), before);
    }
    #[test]
    fn create_page_returns_server_id_and_preserves_current_page() {
        let (_dir, app) = app();
        let layout = app.agent_call("get_layout", json!({})).unwrap();
        let applied=app.agent_call("apply_changes",json!({"expectedRevision":layout["revision"],"ops":[{"op":"create_page","profileId":layout["profileId"],"name":"New page"}]})).unwrap();
        let created = &applied["diff"]["createdPages"][0];
        assert!(created["pageId"].as_str().unwrap().starts_with("page-"));
        assert_eq!(applied["state"]["currentPage"], layout["pageId"]);
        assert_eq!(
            applied["state"]["profiles"][0]["pages"][1]["keys"]
                .as_array()
                .unwrap()
                .len(),
            14
        );
    }
    struct EditorEvents {
        app: Mutex<Weak<App>>,
        events: Mutex<Vec<String>>,
        accept: bool,
        flush_edit: bool,
    }
    impl super::super::app::Events for EditorEvents {
        fn agent(&self, event: &str, payload: Value) {
            self.events.lock().unwrap().push(event.into());
            if event == "prepare" {
                let app = self.app.lock().unwrap().upgrade().unwrap();
                assert!(app.busy_for_quit());
                assert!(app.agent_begin_close().is_err());
                if self.flush_edit {
                    let mut state = app.profiles.state().unwrap();
                    state.profiles[0].pages[0].name = "Flushed UI edit".into();
                    app.save_profiles(state).unwrap();
                }
                app.agent_editor_ready(payload["leaseId"].as_str().unwrap().into(), self.accept)
                    .unwrap();
            }
        }
    }
    fn editor_app(
        accept: bool,
        flush_edit: bool,
    ) -> (tempfile::TempDir, Arc<App>, Arc<EditorEvents>) {
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(EditorEvents {
            app: Mutex::new(Weak::new()),
            events: Mutex::new(Vec::new()),
            accept,
            flush_edit,
        });
        let app = App::new(dir.path(), events.clone(), "fixture");
        *events.app.lock().unwrap() = Arc::downgrade(&app);
        app.settings
            .update(|s| s["ui"]["agentAccess"] = json!(true))
            .unwrap();
        app.agent_editor_present(true).unwrap();
        (dir, app, events)
    }
    #[test]
    fn editor_lease_success_emits_commit_and_release_and_blocks_quit() {
        let (_dir, app, events) = editor_app(true, false);
        app.agent_call("apply_changes", changes(&app, 1, "Agent"))
            .unwrap();
        assert_eq!(
            *events.events.lock().unwrap(),
            ["prepare", "committed", "released"]
        );
        assert!(!app.agent_busy());
        assert!(app.agent_editor_ready("late-lease".into(), true).is_err());
    }
    #[test]
    fn refused_editor_and_revision_changed_during_flush_release_without_clobbering() {
        let (_dir, app, events) = editor_app(false, false);
        let before = app.profiles_state().unwrap();
        assert_eq!(
            app.agent_call("apply_changes", changes(&app, 1, "Agent"))
                .unwrap_err()
                .code
                .as_deref(),
            Some("busy")
        );
        assert_eq!(app.profiles_state().unwrap(), before);
        assert_eq!(*events.events.lock().unwrap(), ["prepare", "released"]);
        assert!(!app.agent_busy());
        let (_dir, app, events) = editor_app(true, true);
        assert_eq!(
            app.agent_call("apply_changes", changes(&app, 1, "Agent"))
                .unwrap_err()
                .code
                .as_deref(),
            Some("revision_conflict")
        );
        assert_eq!(
            app.profiles.state().unwrap().profiles[0].pages[0].name,
            "Flushed UI edit"
        );
        assert_eq!(*events.events.lock().unwrap(), ["prepare", "released"]);
        assert!(!app.agent_busy());
    }
}
