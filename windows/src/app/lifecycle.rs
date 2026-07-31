//! Native-window close handling for the background tray process.

#[derive(Debug, Default)]
pub(super) struct WindowLifecycle {
    exit_requested: bool,
}

impl WindowLifecycle {
    pub(super) fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    pub(super) fn should_cancel_close(&self, close_requested: bool) -> bool {
        close_requested && !self.exit_requested
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unexpected_window_close_keeps_background_services_alive() {
        assert!(WindowLifecycle::default().should_cancel_close(true));
    }

    #[test]
    fn tray_quit_allows_the_native_window_to_close() {
        let mut lifecycle = WindowLifecycle::default();
        lifecycle.request_exit();
        assert!(!lifecycle.should_cancel_close(true));
    }

    #[test]
    fn missing_close_request_needs_no_action() {
        assert!(!WindowLifecycle::default().should_cancel_close(false));
    }
}
