//! Native window placement for the settings panel and transient overlay.

use eframe::egui::{self, ViewportCommand};

use crate::overlay::CARD_W;

use super::controller::LocalSttApp;

const SETTINGS_W: f32 = 720.0;
const SETTINGS_H: f32 = 780.0;

impl LocalSttApp {
    pub(super) fn sync_viewport(&mut self, ctx: &egui::Context) {
        let monitor = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1920.0, 1080.0));

        if self.settings.open {
            let width = SETTINGS_W.min((monitor.x - 24.0).max(360.0));
            let height = SETTINGS_H.min((monitor.y - 40.0).max(420.0));
            let x = ((monitor.x - width) * 0.5).max(0.0);
            let y = ((monitor.y - height) * 0.35).max(20.0);
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(x, y)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(width, height)));
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
            if self.settings.focus_pending {
                ctx.send_viewport_cmd(ViewportCommand::Focus);
                self.settings.focus_pending = false;
            }
            return;
        }

        if self.overlay.is_visible() {
            let overlay_width = CARD_W.min((monitor.x - 24.0).max(360.0));
            let x = ((monitor.x - overlay_width) * 0.5).max(0.0);
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(x, 70.0)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
                overlay_width,
                self.overlay.desired_height(),
            )));
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        } else {
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(-32000.0, -32000.0)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(8.0, 8.0)));
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
        }
    }
}
