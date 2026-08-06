//! Native root-window close handling for the background tray process.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CloseDecision {
    None,
    CancelAndHide,
    ContinueAfterCancel,
    Exit,
}

#[derive(Debug, Default)]
pub(super) struct WindowLifecycle {
    exit_requested: bool,
    close_cancelled: bool,
}

impl WindowLifecycle {
    pub(super) fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    pub(super) fn decide(&mut self, close_requested: bool) -> CloseDecision {
        if !close_requested {
            self.close_cancelled = false;
            return CloseDecision::None;
        }
        if self.exit_requested {
            return CloseDecision::Exit;
        }
        if self.close_cancelled {
            return CloseDecision::ContinueAfterCancel;
        }

        self.close_cancelled = true;
        CloseDecision::CancelAndHide
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_native_close_is_cancelled_and_hides_the_visible_surface() {
        let mut lifecycle = WindowLifecycle::default();
        assert_eq!(
            lifecycle.decide(true),
            CloseDecision::CancelAndHide
        );
    }

    #[test]
    fn repeated_close_flag_does_not_starve_the_event_loop() {
        let mut lifecycle = WindowLifecycle::default();
        assert_eq!(lifecycle.decide(true), CloseDecision::CancelAndHide);
        assert_eq!(
            lifecycle.decide(true),
            CloseDecision::ContinueAfterCancel
        );
    }

    #[test]
    fn cleared_close_flag_rearms_the_next_close_request() {
        let mut lifecycle = WindowLifecycle::default();
        assert_eq!(lifecycle.decide(true), CloseDecision::CancelAndHide);
        assert_eq!(lifecycle.decide(false), CloseDecision::None);
        assert_eq!(lifecycle.decide(true), CloseDecision::CancelAndHide);
    }

    #[test]
    fn tray_quit_allows_the_native_window_to_close() {
        let mut lifecycle = WindowLifecycle::default();
        lifecycle.request_exit();
        assert_eq!(lifecycle.decide(true), CloseDecision::Exit);
    }
}
