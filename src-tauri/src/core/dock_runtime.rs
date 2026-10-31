//! Validated profiles -> rendered PNGs -> transport-independent dock. No native
//! transport is constructed here. External actions are returned as inert data.
use super::{
    key_render::{render_face, FrameCache, RenderInputs, RenderedFace},
    profiles::{Face, ProfileState},
};
use ulanzi_dock::{
    dock::{Dock, Transport},
    page::{build_partial_page_zip, PageSpec},
    protocol::{ButtonAction, ButtonKind, Event},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingAction {
    pub kind: String,
    pub value: String,
}

pub struct PageRuntime {
    state: ProfileState,
    faces_b: [bool; 14],
    down: [bool; 14],
    back: Vec<String>,
    cache: FrameCache,
    full_update: bool,
}
impl PageRuntime {
    pub fn new(state: ProfileState) -> Result<Self, String> {
        state.validate().map_err(|e| e.to_string())?;
        Ok(Self {
            state,
            faces_b: [false; 14],
            down: [false; 14],
            back: Vec::new(),
            cache: FrameCache::default(),
            full_update: true,
        })
    }
    pub fn state(&self) -> &ProfileState {
        &self.state
    }
    pub fn face(&self, index: usize) -> Result<&Face, String> {
        let key = self
            .state
            .active_page()
            .and_then(|page| page.keys.get(index))
            .ok_or("Display index is invalid")?;
        Ok(if self.faces_b[index] { &key.b } else { &key.a })
    }
    pub fn invalidate_images(&mut self) {
        self.cache.clear();
        self.full_update = true;
    }
    pub fn reset_input(&mut self) {
        self.down = [false; 14];
    }
    pub fn update_state(&mut self, state: ProfileState) -> Result<(), String> {
        state.validate().map_err(|e| e.to_string())?;
        if self.state.current_profile != state.current_profile
            || self.state.current_page != state.current_page
        {
            self.faces_b = [false; 14];
            self.back.clear();
        }
        self.state = state;
        self.invalidate_images();
        Ok(())
    }
    fn navigate(&mut self, page: String) {
        self.state.current_page = page;
        self.faces_b = [false; 14];
        self.invalidate_images();
    }
    /// One physical press flips A/B once. Repeated press reports are ignored
    /// until release, including when the first press navigated to another page.
    pub fn handle_event(&mut self, event: &Event) -> Result<Option<PendingAction>, String> {
        let Event::Button(button) = event else {
            return Ok(None);
        };
        let index = button.index as usize;
        if index >= 14 || button.kind != ButtonKind::Button {
            return Ok(None);
        }
        if button.action == ButtonAction::Release {
            self.down[index] = false;
            return Ok(None);
        }
        if button.action != ButtonAction::Press || self.down[index] {
            return Ok(None);
        }
        self.down[index] = true;
        self.faces_b[index] = !self.faces_b[index];
        let key = self.state.active_page().unwrap().keys[index].clone();
        match key.action.as_str() {
            "none" => Ok(None),
            "page" => {
                self.back.clear();
                self.navigate(key.value);
                Ok(None)
            }
            "folder" => {
                if self.back.len() >= 32 {
                    return Err("Folder navigation limit reached".into());
                }
                self.back.push(self.state.current_page.clone());
                self.navigate(key.value);
                Ok(None)
            }
            "back" => {
                if let Some(page) = self.back.pop() {
                    self.navigate(page)
                }
                Ok(None)
            }
            _ => Ok(Some(PendingAction {
                kind: key.action,
                value: key.value,
            })),
        }
    }
    /// Successful unchanged frames produce no packets. A failed write leaves no
    /// accepted cache state; reconnect must use a new transport and a full page.
    pub fn render_and_send<T: Transport>(
        &mut self,
        dock: &mut Dock<T>,
        inputs: &RenderInputs,
    ) -> Result<usize, String> {
        self.render_and_send_with(dock, inputs, |index, face, input| {
            render_face(&face.kind, &face.text, index == 13, input)
        })
    }
    pub fn render_and_send_with<
        T: Transport,
        F: FnMut(usize, &Face, &RenderInputs) -> Result<RenderedFace, String>,
    >(
        &mut self,
        dock: &mut Dock<T>,
        inputs: &RenderInputs,
        render: F,
    ) -> Result<usize, String> {
        self.render_and_send_guarded(dock, inputs, render, || true)
    }
    /// Admission is rechecked after slow rendering, immediately before transfer.
    pub fn render_and_send_guarded<
        T: Transport,
        F: FnMut(usize, &Face, &RenderInputs) -> Result<RenderedFace, String>,
        G: FnMut() -> bool,
    >(
        &mut self,
        dock: &mut Dock<T>,
        inputs: &RenderInputs,
        mut render: F,
        mut admit: G,
    ) -> Result<usize, String> {
        let mut spec = PageSpec::new();
        let mut frames = Vec::with_capacity(14);
        let mut changed = Vec::new();
        for index in 0..14 {
            let face = self.face(index)?;
            let frame = render(index, face, inputs)?;
            frames.push(frame);
        }
        let now = super::scheduler::now_ms();
        for (index, frame) in frames.iter_mut().enumerate() {
            frame.guard_delivery(now)?;
            if self.cache.needs_update(index, &frame.png)? {
                changed.push(index)
            }
            if index == 13 {
                spec.wide_png = Some(frame.png.clone())
            } else {
                spec.set_key(index, None, frame.png.clone())
            }
        }
        if !self.full_update && changed.is_empty() {
            return Ok(0);
        }
        if !admit() {
            return Ok(0);
        }
        let result = if self.full_update {
            dock.set_page(&spec)
        } else {
            let zip = build_partial_page_zip(&spec, &changed).map_err(|e| e.to_string())?;
            if !admit() {
                return Ok(0);
            }
            dock.update_buttons_zip(&zip)
        };
        if let Err(error) = result {
            self.invalidate_images();
            return Err(error.to_string());
        }
        for (index, frame) in frames.iter().enumerate() {
            self.cache.changed(index, &frame.png)?;
        }
        let count = if self.full_update { 14 } else { changed.len() };
        self.full_update = false;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::profiles::{Page, ProfileStore};
    use std::{io, time::Duration};
    use ulanzi_dock::{
        fake::FakeDock,
        protocol::{ButtonEvent, Packet},
    };
    fn event(index: u8, action: ButtonAction) -> Event {
        Event::Button(ButtonEvent {
            state: 0,
            index,
            kind: ButtonKind::Button,
            action,
        })
    }
    #[test]
    fn exactly_two_faces_and_single_action_per_press() {
        let mut state = ProfileState::default();
        let key = &mut state.profiles[0].pages[0].keys[0];
        key.a.text = "A".into();
        key.b.text = "B".into();
        key.action = "hotkey".into();
        key.value = "Ctrl + M".into();
        let mut runtime = PageRuntime::new(state).unwrap();
        assert_eq!(runtime.face(0).unwrap().text, "A");
        assert_eq!(
            runtime
                .handle_event(&event(0, ButtonAction::Press))
                .unwrap()
                .unwrap()
                .kind,
            "hotkey"
        );
        assert_eq!(runtime.face(0).unwrap().text, "B");
        assert!(runtime
            .handle_event(&event(0, ButtonAction::Press))
            .unwrap()
            .is_none());
        runtime
            .handle_event(&event(0, ButtonAction::Release))
            .unwrap();
        runtime
            .handle_event(&event(0, ButtonAction::Press))
            .unwrap();
        assert_eq!(runtime.face(0).unwrap().text, "A");
        assert!(runtime
            .handle_event(&event(255, ButtonAction::Press))
            .unwrap()
            .is_none());
    }
    #[test]
    fn folders_and_back_preserve_press_latch_across_page_change() {
        let mut state = ProfileState::default();
        let first = state.current_page.clone();
        let mut folder = Page::new("Folder");
        state.profiles[0].pages[0].keys[0].action = "folder".into();
        state.profiles[0].pages[0].keys[0].value = folder.id.clone();
        folder.keys[0].action = "back".into();
        let target = folder.id.clone();
        state.profiles[0].pages.push(folder);
        let mut runtime = PageRuntime::new(state).unwrap();
        runtime
            .handle_event(&event(0, ButtonAction::Press))
            .unwrap();
        assert_eq!(runtime.state().current_page, target);
        runtime
            .handle_event(&event(0, ButtonAction::Press))
            .unwrap();
        assert_eq!(runtime.state().current_page, target);
        runtime
            .handle_event(&event(0, ButtonAction::Release))
            .unwrap();
        runtime
            .handle_event(&event(0, ButtonAction::Press))
            .unwrap();
        assert_eq!(runtime.state().current_page, first);
    }
    #[cfg(windows)]
    #[test]
    fn expired_account_png_is_replaced_before_batch_send() {
        let dir = tempfile::tempdir().unwrap();
        let mut dock = Dock::new(FakeDock::with_out_dir(dir.path()));
        let mut runtime = PageRuntime::new(ProfileState::default()).unwrap();
        runtime
            .render_and_send_with(&mut dock, &RenderInputs::default(), |index, _, input| {
                let mut frame = render_face(
                    "text",
                    if index == 0 { "90%" } else { "+" },
                    index == 13,
                    input,
                )?;
                if index == 0 {
                    frame.valid_until_ms = Some(1)
                }
                Ok(frame)
            })
            .unwrap();
        let expected = render_face("reset", "", false, &RenderInputs::default()).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("key_00.png")).unwrap(),
            expected.png
        );
    }
    #[cfg(windows)]
    #[test]
    fn saved_profile_to_png_to_fake_transport_and_partial_update() {
        let temp = tempfile::tempdir().unwrap();
        let store = ProfileStore::new(temp.path());
        let mut state = store.load().unwrap();
        state.profiles[0].pages[0].keys[0].a.text = "A".into();
        state.profiles[0].pages[0].keys[0].b.text = "B".into();
        store.save(&state).unwrap();
        let mut runtime = PageRuntime::new(ProfileStore::new(temp.path()).load().unwrap()).unwrap();
        let mut dock = Dock::new(FakeDock::new());
        assert_eq!(
            runtime
                .render_and_send(&mut dock, &RenderInputs::default())
                .unwrap(),
            14
        );
        assert_eq!(
            runtime
                .render_and_send(&mut dock, &RenderInputs::default())
                .unwrap(),
            0
        );
        runtime
            .handle_event(&event(0, ButtonAction::Press))
            .unwrap();
        assert_eq!(
            runtime
                .render_and_send(&mut dock, &RenderInputs::default())
                .unwrap(),
            1
        );
        assert_eq!(dock.transport().partial_updates_received, 1);
        assert!(dock.transport().errors.is_empty());
        assert_eq!(
            dock.transport()
                .last_manifest
                .as_ref()
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            14
        );
        struct Broken;
        impl Transport for Broken {
            fn write(&mut self, _: &Packet) -> io::Result<()> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn read(&mut self, _: Duration) -> io::Result<Option<Packet>> {
                Ok(None)
            }
        }
        runtime
            .handle_event(&event(0, ButtonAction::Release))
            .unwrap();
        runtime
            .handle_event(&event(0, ButtonAction::Press))
            .unwrap();
        assert!(runtime
            .render_and_send(&mut Dock::new(Broken), &RenderInputs::default())
            .is_err());
        let mut reconnected = Dock::new(FakeDock::new());
        assert_eq!(
            runtime
                .render_and_send(&mut reconnected, &RenderInputs::default())
                .unwrap(),
            14
        );
    }
}
