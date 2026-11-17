//! Find and focus an app's message box (its composer) with Windows UI Automation, before a Vibe Key action
//! types into it (user report 2026-11-17: with Claude's terminal panel focused, "Start dictation" sent Ctrl + D
//! into the terminal, which can close the shell; "Dictate & send" would also have sent Enter there).
//!
//! Only reads the window's accessibility tree and moves keyboard focus inside the app already in front; it sends
//! no keys. The message box is an editable element (Edit, or a Document that is not the page itself) whose name
//! or automation id names a prompt or message box, and not a terminal, search or rename field; with no such name,
//! the single editable element left counts. Focus is confirmed afterwards. Logs hold only the element's type and
//! a shortened name (its label, never its text).

/// What the composer search looks at for one element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub kind: &'static str,
    pub name: String,
    pub id: String,
}

impl Candidate {
    fn text(&self) -> String {
        format!("{} {} {}", self.kind, self.name, self.id).to_lowercase()
    }
    /// A short description for the log (label only, at most 40 characters of it).
    pub fn describe(&self) -> String {
        let short: String = self.name.chars().take(40).collect();
        let id: String = self.id.chars().take(40).collect();
        format!("{} \"{short}\" id \"{id}\"", self.kind)
    }
}

const NOT_COMPOSER: [&str; 7] = ["terminal", "search", "find", "filter", "rename", "console", "xterm"];
const COMPOSER: [&str; 6] = ["prompt", "message", "reply", "ask", "chat", "composer"];

/// Which candidate is the message box (index), or `None`. Pure, so it is tested without a desktop.
pub fn choose(candidates: &[Candidate]) -> Option<usize> {
    let allowed: Vec<usize> = (0..candidates.len())
        .filter(|&i| !NOT_COMPOSER.iter().any(|w| candidates[i].text().contains(w)))
        .collect();
    let named: Vec<usize> = allowed
        .iter()
        .copied()
        .filter(|&i| COMPOSER.iter().any(|w| format!("{} {}", candidates[i].name, candidates[i].id).to_lowercase().contains(w)))
        .collect();
    if let Some(&i) = named.first() {
        return Some(i);
    }
    // No named message box: a single editable field left (a Document needs a name: it may be the page).
    let edits: Vec<usize> = allowed.into_iter().filter(|&i| candidates[i].kind == "Edit").collect();
    (edits.len() == 1).then(|| edits[0])
}

#[cfg(windows)]
pub use native::focus;

#[cfg(windows)]
mod native {
    use super::{choose, Candidate};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants, UIA_ControlTypePropertyId,
        UIA_DocumentControlTypeId, UIA_EditControlTypeId,
    };

    /// Find the message box in the window `hwnd` (the app in front), focus it and confirm the focus is there.
    /// Returns its description (for the log), or why not.
    pub fn focus(hwnd: isize) -> Result<String, String> {
        let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let result = unsafe { find_and_focus(HWND(hwnd as *mut _)) };
        if init.is_ok() {
            unsafe { CoUninitialize() };
        }
        result
    }

    unsafe fn editable(uia: &IUIAutomation, root: &IUIAutomationElement) -> Result<Vec<(Candidate, IUIAutomationElement)>, String> {
        let err = |e: windows::core::Error| format!("UI Automation error ({:#x})", e.code().0);
        let edit = unsafe { uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_EditControlTypeId.0)) }.map_err(err)?;
        let doc = unsafe { uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_DocumentControlTypeId.0)) }.map_err(err)?;
        let either = unsafe { uia.CreateOrCondition(&edit, &doc) }.map_err(err)?;
        let all = unsafe { root.FindAll(TreeScope_Descendants, &either) }.map_err(err)?;
        let mut out = Vec::new();
        for i in 0..unsafe { all.Length() }.unwrap_or(0).min(200) {
            let Ok(e) = (unsafe { all.GetElement(i) }) else { continue };
            let usable = unsafe { e.CurrentIsEnabled() }.is_ok_and(|b| b.as_bool())
                && unsafe { e.CurrentIsKeyboardFocusable() }.is_ok_and(|b| b.as_bool())
                && !unsafe { e.CurrentIsOffscreen() }.is_ok_and(|b| b.as_bool());
            if !usable {
                continue;
            }
            let kind = if unsafe { e.CurrentControlType() }.is_ok_and(|t| t == UIA_EditControlTypeId) { "Edit" } else { "Document" };
            let name = unsafe { e.CurrentName() }.map(|b| b.to_string()).unwrap_or_default();
            let id = unsafe { e.CurrentAutomationId() }.map(|b| b.to_string()).unwrap_or_default();
            out.push((Candidate { kind, name, id }, e));
        }
        Ok(out)
    }

    unsafe fn find_and_focus(hwnd: HWND) -> Result<String, String> {
        let uia: IUIAutomation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| format!("UI Automation unavailable ({:#x})", e.code().0))?;
        let root = unsafe { uia.ElementFromHandle(hwnd) }.map_err(|e| format!("window not readable ({:#x})", e.code().0))?;
        // A Chromium/Electron app builds its accessibility tree when first asked: look once more after a moment.
        let mut found = unsafe { editable(&uia, &root) }?;
        if choose(&found.iter().map(|(c, _)| c.clone()).collect::<Vec<_>>()).is_none() {
            std::thread::sleep(std::time::Duration::from_millis(300));
            found = unsafe { editable(&uia, &root) }?;
        }
        let candidates: Vec<Candidate> = found.iter().map(|(c, _)| c.clone()).collect();
        let Some(i) = choose(&candidates) else {
            let seen: Vec<String> = candidates.iter().take(8).map(Candidate::describe).collect();
            return Err(format!("no message box among {} editable elements [{}]", candidates.len(), seen.join(", ")));
        };
        let (candidate, element) = &found[i];
        unsafe { element.SetFocus() }.map_err(|e| format!("{} could not be focused ({:#x})", candidate.describe(), e.code().0))?;
        // Confirm: the focused element is the message box or inside it.
        let end = std::time::Instant::now() + std::time::Duration::from_millis(400);
        loop {
            if let Ok(mut f) = unsafe { uia.GetFocusedElement() } {
                let walker = unsafe { uia.ControlViewWalker() }.ok();
                for _ in 0..6 {
                    if unsafe { uia.CompareElements(&f, element) }.is_ok_and(|b| b.as_bool()) {
                        return Ok(candidate.describe());
                    }
                    let Some(parent) = walker.as_ref().and_then(|w| unsafe { w.GetParentElement(&f) }.ok()) else { break };
                    f = parent;
                }
            }
            if std::time::Instant::now() >= end {
                return Err(format!("focus did not move to {}", candidate.describe()));
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(kind: &'static str, name: &str, id: &str) -> Candidate {
        Candidate { kind, name: name.into(), id: id.into() }
    }
    #[test]
    fn the_message_box_is_chosen_never_the_terminal() {
        // The reported case: Claude's terminal panel (xterm's input) next to the prompt box.
        let both = [c("Edit", "Terminal input", ""), c("Edit", "Write your prompt to Claude", "")];
        assert_eq!(choose(&both), Some(1));
        assert_eq!(choose(&[c("Edit", "Terminal input", "")]), None, "only a terminal: nothing");
        assert_eq!(choose(&[c("Edit", "Search sessions", ""), c("Document", "Claude", "")]), None, "the page is not a message box");
        assert_eq!(choose(&[c("Document", "", "message-input")]), Some(0), "an automation id counts");
        assert_eq!(choose(&[c("Edit", "", "")]), Some(0), "a single unnamed field");
        assert_eq!(choose(&[c("Edit", "", ""), c("Edit", "Rename", "")]), Some(0));
        assert_eq!(choose(&[c("Edit", "", ""), c("Edit", "", "")]), None, "two unnamed fields: not sure");
        assert_eq!(choose(&[]), None);
        assert!(c("Edit", &"x".repeat(90), "").describe().len() < 70, "the log keeps a short label");
    }
}
