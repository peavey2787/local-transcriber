//! Placement of the persistent root viewport used for transient overlays.

use eframe::egui::{self, ViewportCommand};

use crate::overlay::CARD_W;

use super::controller::LocalSttApp;

impl LocalSttApp {
    pub(super) fn sync_root_viewport(&mut self, ctx: &egui::Context) {
        let monitor = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1920.0, 1080.0));

        if self.settings.open {
            park_root_viewport(ctx);
            return;
        }

        if self.overlay.is_visible() {
            let overlay_width = CARD_W.min((monitor.x - 24.0).max(360.0));
            let x = ((monitor.x - overlay_width) * 0.5).max(0.0);
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(x, 70.0)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
                overlay_width,
                self.overlay.desired_height(),
            )));
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        } else {
            park_root_viewport(ctx);
        }
    }
}

fn park_root_viewport(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(
        -32000.0, -32000.0,
    )));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(8.0, 8.0)));
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
}
