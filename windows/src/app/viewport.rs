//! Placement and interaction policy for the persistent root viewport.

use eframe::egui::{self, ViewportCommand};

use crate::overlay::CARD_W;

use super::controller::LocalSttApp;

pub(crate) const CONTROL_VIEWPORT_POSITION: [f32; 2] = [0.0, 0.0];
pub(crate) const CONTROL_VIEWPORT_SIZE: [f32; 2] = [8.0, 8.0];

impl LocalSttApp {
    pub(super) fn sync_root_viewport(&mut self, ctx: &egui::Context) {
        let monitor = ctx
            .input(|i| i.viewport().monitor_size)
            .unwrap_or(egui::vec2(1920.0, 1080.0));

        if self.settings.open {
            configure_control_viewport(ctx);
            return;
        }

        if self.overlay.is_visible() {
            let overlay_width = CARD_W.min((monitor.x - 24.0).max(360.0));
            let x = ((monitor.x - overlay_width) * 0.5).max(0.0);
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(false));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(x, 70.0)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
                overlay_width,
                self.overlay.desired_height(),
            )));
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        } else {
            configure_control_viewport(ctx);
        }
    }
}

fn configure_control_viewport(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(true));
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(
        CONTROL_VIEWPORT_POSITION[0],
        CONTROL_VIEWPORT_POSITION[1],
    )));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
        CONTROL_VIEWPORT_SIZE[0],
        CONTROL_VIEWPORT_SIZE[1],
    )));
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
}

#[cfg(test)]
mod tests {
    use super::{CONTROL_VIEWPORT_POSITION, CONTROL_VIEWPORT_SIZE};

    #[test]
    fn control_viewport_is_on_screen_and_serviceable() {
        assert!(CONTROL_VIEWPORT_POSITION
            .into_iter()
            .all(|value| value >= 0.0));
        assert!(CONTROL_VIEWPORT_SIZE.into_iter().all(|value| value > 0.0));
    }
}
