//! Independent Settings viewport lifecycle and geometry.

use eframe::egui;

use super::super::super::controller::LocalSttApp;
use super::super::super::viewport::{coexisting_window_width, WINDOW_EDGE_MARGIN};

const SETTINGS_VIEWPORT_ID: &str = "local-stt-settings";
const SETTINGS_WINDOW_TITLE: &str = "local-stt settings";
const SETTINGS_W: f32 = 720.0;
const SETTINGS_H: f32 = 780.0;

impl LocalSttApp {
    pub(in crate::app) fn render_settings(&mut self, root_ctx: &egui::Context) {
        if !self.settings_window.is_visible() {
            return;
        }

        let (position, size) =
            settings_viewport_geometry(monitor_size(root_ctx), self.overlay.is_visible());
        let builder = egui::ViewportBuilder::default()
            .with_title(SETTINGS_WINDOW_TITLE)
            .with_icon(self.app_icon.clone())
            .with_position(position)
            .with_inner_size(size)
            .with_resizable(false)
            .with_taskbar(true)
            .with_close_button(true)
            .with_always_on_top();

        let close_requested = root_ctx.show_viewport_immediate(
            settings_viewport_id(),
            builder,
            |settings_ctx, _class| {
                if self.settings_window.take_focus_request() {
                    settings_ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }

                self.draw_settings(settings_ctx);
                settings_ctx.input(|input| input.viewport().close_requested())
            },
        );

        if close_requested {
            // Stop presenting this child viewport. The persistent root viewport,
            // tray, hotkey, notifications, and workers remain alive. A later
            // Settings command recreates the child with the same stable ID.
            self.close_settings();
            root_ctx.request_repaint_of(egui::ViewportId::ROOT);
        }
    }
}

fn settings_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of(SETTINGS_VIEWPORT_ID)
}

fn monitor_size(ctx: &egui::Context) -> egui::Vec2 {
    ctx.input(|input| input.viewport().monitor_size)
        .unwrap_or(egui::vec2(1920.0, 1080.0))
}

fn settings_viewport_geometry(
    monitor: egui::Vec2,
    notification_visible: bool,
) -> (egui::Pos2, egui::Vec2) {
    let width = if notification_visible {
        coexisting_window_width(monitor.x, SETTINGS_W)
    } else {
        SETTINGS_W.min((monitor.x - WINDOW_EDGE_MARGIN).max(360.0))
    };
    let height = SETTINGS_H.min((monitor.y - 40.0).max(420.0));
    let x = if notification_visible {
        WINDOW_EDGE_MARGIN
    } else {
        ((monitor.x - width) * 0.5).max(0.0)
    };
    let y = ((monitor.y - height) * 0.35).max(20.0);
    (egui::pos2(x, y), egui::vec2(width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_window_is_not_the_root_event_loop_window() {
        assert_ne!(settings_viewport_id(), egui::ViewportId::ROOT);
    }

    #[test]
    fn settings_window_is_centered_and_bounded_to_the_monitor() {
        let (position, size) =
            settings_viewport_geometry(egui::vec2(1920.0, 1080.0), false);
        assert_eq!(size, egui::vec2(SETTINGS_W, SETTINGS_H));
        assert_eq!(position.x, 600.0);
        assert!(position.y >= 20.0);

        let (_, compact_size) =
            settings_viewport_geometry(egui::vec2(640.0, 480.0), false);
        assert!(compact_size.x <= 616.0);
        assert!(compact_size.y <= 440.0);
    }

    #[test]
    fn visible_notification_places_settings_on_the_left() {
        let (position, size) =
            settings_viewport_geometry(egui::vec2(1366.0, 768.0), true);

        assert_eq!(position.x, WINDOW_EDGE_MARGIN);
        assert_eq!(size.x, 647.0);
    }
}
