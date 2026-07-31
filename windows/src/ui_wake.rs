//! Thread-safe root-viewport wake handle for events outside the egui loop.

use eframe::egui;
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Clone, Default)]
pub(crate) struct UiWake {
    context: Arc<Mutex<Option<egui::Context>>>,
}

impl UiWake {
    pub(crate) fn install(&self, context: &egui::Context) {
        let mut installed = self.context.lock();
        if installed.is_none() {
            *installed = Some(context.clone());
        }
    }

    pub(crate) fn request_root_repaint(&self) {
        if let Some(context) = self.context.lock().as_ref() {
            context.request_repaint_of(egui::ViewportId::ROOT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_can_be_requested_before_egui_is_installed() {
        UiWake::default().request_root_repaint();
    }

    #[test]
    fn external_events_target_root_even_when_a_child_was_current() {
        let context = egui::Context::default();
        let requested_viewports = Arc::new(Mutex::new(Vec::new()));
        let captured_viewports = requested_viewports.clone();
        context.set_request_repaint_callback(move |request| {
            captured_viewports.lock().push(request.viewport_id);
        });

        let child_viewport = egui::ViewportId::from_hash_of("settings-child");
        let child_input = egui::RawInput {
            viewport_id: child_viewport,
            viewports: [
                (egui::ViewportId::ROOT, Default::default()),
                (child_viewport, Default::default()),
            ]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        let _ = context.run(child_input, |_ctx| {});
        requested_viewports.lock().clear();

        let wake = UiWake::default();
        wake.install(&context);
        wake.request_root_repaint();

        assert_eq!(
            requested_viewports.lock().last(),
            Some(&egui::ViewportId::ROOT)
        );
    }
}
