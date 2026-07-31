//! Placement and interaction policy for the persistent root viewport.

use eframe::egui::{self, ViewportCommand};

use crate::overlay::CARD_W;

use super::controller::LocalSttApp;

pub(crate) const CONTROL_VIEWPORT_POSITION: [f32; 2] = [0.0, 0.0];
pub(crate) const CONTROL_VIEWPORT_SIZE: [f32; 2] = [8.0, 8.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootViewportMode {
    Control,
    Notification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewportChrome {
    transparent: bool,
    mouse_passthrough: bool,
}

impl RootViewportMode {
    fn from_state(settings_open: bool, notification_visible: bool) -> Self {
        if !settings_open && notification_visible {
            Self::Notification
        } else {
            Self::Control
        }
    }

    fn chrome(self) -> ViewportChrome {
        match self {
            Self::Control => ViewportChrome {
                transparent: true,
                mouse_passthrough: true,
            },
            Self::Notification => ViewportChrome {
                transparent: true,
                mouse_passthrough: false,
            },
        }
    }
}

impl LocalSttApp {
    pub(super) fn sync_root_viewport(&mut self, ctx: &egui::Context) {
        match RootViewportMode::from_state(self.settings.open, self.overlay.is_visible()) {
            RootViewportMode::Control => configure_control_viewport(ctx),
            RootViewportMode::Notification => configure_notification_viewport(self, ctx),
        }
    }
}

fn apply_root_chrome(ctx: &egui::Context, chrome: ViewportChrome) {
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Title("local-stt".to_string()));
    ctx.send_viewport_cmd(ViewportCommand::Transparent(chrome.transparent));
    ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
    ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(chrome.mouse_passthrough));
    ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
    ctx.send_viewport_cmd(ViewportCommand::EnableButtons {
        close: false,
        minimized: false,
        maximize: false,
    });
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
}

fn configure_control_viewport(ctx: &egui::Context) {
    apply_root_chrome(ctx, RootViewportMode::Control.chrome());
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(
        CONTROL_VIEWPORT_POSITION[0],
        CONTROL_VIEWPORT_POSITION[1],
    )));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
        CONTROL_VIEWPORT_SIZE[0],
        CONTROL_VIEWPORT_SIZE[1],
    )));
}

fn configure_notification_viewport(app: &LocalSttApp, ctx: &egui::Context) {
    let monitor = monitor_size(ctx);
    let overlay_width = CARD_W.min((monitor.x - 24.0).max(360.0));
    let x = ((monitor.x - overlay_width) * 0.5).max(0.0);

    apply_root_chrome(ctx, RootViewportMode::Notification.chrome());
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(x, 70.0)));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
        overlay_width,
        app.overlay.desired_height(),
    )));
}

fn monitor_size(ctx: &egui::Context) -> egui::Vec2 {
    ctx.input(|input| input.viewport().monitor_size)
        .unwrap_or(egui::vec2(1920.0, 1080.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_commands(mode: RootViewportMode) -> Vec<ViewportCommand> {
        let context = egui::Context::default();
        let mut output = context.run(Default::default(), |ctx| {
            apply_root_chrome(ctx, mode.chrome());
        });
        output
            .viewport_output
            .remove(&egui::ViewportId::ROOT)
            .expect("root viewport output")
            .commands
    }

    #[test]
    fn settings_never_repurpose_or_close_the_root_viewport() {
        assert_eq!(
            RootViewportMode::from_state(true, true),
            RootViewportMode::Control
        );
        let commands = root_commands(RootViewportMode::Control);
        assert!(commands
            .iter()
            .any(|command| matches!(command, ViewportCommand::Decorations(false))));
        assert!(commands.iter().any(|command| matches!(
            command,
            ViewportCommand::EnableButtons { close: false, .. }
        )));
    }

    #[test]
    fn notification_mode_is_interactive_without_native_window_controls() {
        let commands = root_commands(RootViewportMode::Notification);
        assert!(commands
            .iter()
            .any(|command| matches!(command, ViewportCommand::MousePassthrough(false))));
        assert!(commands
            .iter()
            .any(|command| matches!(command, ViewportCommand::Decorations(false))));
    }

    #[test]
    fn control_viewport_is_on_screen_and_click_through() {
        assert!(CONTROL_VIEWPORT_POSITION
            .into_iter()
            .all(|value| value >= 0.0));
        assert!(CONTROL_VIEWPORT_SIZE.into_iter().all(|value| value > 0.0));
        assert!(RootViewportMode::Control.chrome().mouse_passthrough);
    }
}
