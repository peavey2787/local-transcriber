//! Thread-safe wake handle for events originating outside the egui loop.

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
}
