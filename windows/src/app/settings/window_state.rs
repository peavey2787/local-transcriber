//! Visibility and focus state for the independent Settings viewport.

#[derive(Debug, Default)]
pub(in crate::app) struct SettingsWindowState {
    visible: bool,
    focus_requested: bool,
}

impl SettingsWindowState {
    /// Present the Settings viewport and request focus.
    ///
    /// Returns `true` only when the viewport transitions from hidden to shown.
    pub(in crate::app) fn show(&mut self) -> bool {
        let became_visible = !self.visible;
        self.visible = true;
        self.focus_requested = true;
        became_visible
    }

    /// Stop presenting the Settings viewport without affecting the root app.
    pub(in crate::app) fn hide(&mut self) {
        self.visible = false;
        self.focus_requested = false;
    }

    pub(in crate::app) fn is_visible(&self) -> bool {
        self.visible
    }

    pub(in crate::app) fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.focus_requested)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_viewport_can_be_shown_hidden_and_shown_again() {
        let mut state = SettingsWindowState::default();

        assert!(!state.is_visible());
        assert!(state.show());
        assert!(state.is_visible());
        assert!(state.take_focus_request());

        state.hide();
        assert!(!state.is_visible());
        assert!(!state.take_focus_request());

        assert!(state.show());
        assert!(state.is_visible());
        assert!(state.take_focus_request());
    }

    #[test]
    fn repeated_show_requests_focus_without_reinitializing_form_state() {
        let mut state = SettingsWindowState::default();

        assert!(state.show());
        assert!(!state.show());
        assert!(state.is_visible());
        assert!(state.take_focus_request());
    }
}
