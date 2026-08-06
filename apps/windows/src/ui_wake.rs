//! Thread-safe wake handle for events arriving outside the egui update loop.

use eframe::egui;
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Clone, Default)]
pub(crate) struct UiWake {
    context: Arc<Mutex<Option<egui::Context>>>,
}

impl UiWake {
    pub(crate) fn install(&self, context: &egui::Context) {
        *self.context.lock() = Some(context.clone());
    }

    pub(crate) fn request_repaint(&self) {
        if let Some(context) = self.context.lock().as_ref() {
            context.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_can_be_requested_before_egui_is_installed() {
        UiWake::default().request_repaint();
    }

    #[test]
    fn external_events_wake_the_root_context() {
        let context = egui::Context::default();
        let requested_viewports = Arc::new(Mutex::new(Vec::new()));
        let captured_viewports = requested_viewports.clone();
        context.set_request_repaint_callback(move |request| {
            captured_viewports.lock().push(request.viewport_id);
        });

        let wake = UiWake::default();
        wake.install(&context);
        wake.request_repaint();

        assert_eq!(
            requested_viewports.lock().last(),
            Some(&egui::ViewportId::ROOT)
        );
    }
}
