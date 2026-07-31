//! Placement and interaction policy for the persistent root viewport.

use eframe::egui::{self, ViewportCommand};

use crate::overlay::CARD_W;

use super::controller::LocalSttApp;

pub(crate) const CONTROL_VIEWPORT_POSITION: [f32; 2] = [0.0, 0.0];
pub(crate) const CONTROL_VIEWPORT_SIZE: [f32; 2] = [8.0, 8.0];
pub(in crate::app) const WINDOW_EDGE_MARGIN: f32 = 24.0;
const WINDOW_GAP: f32 = 24.0;
const MIN_PARALLEL_WINDOW_WIDTH: f32 = 360.0;

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
    fn from_notification_visibility(notification_visible: bool) -> Self {
        if notification_visible {
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
        match RootViewportMode::from_notification_visibility(self.overlay.is_visible()) {
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
    let overlay_width = if app.settings.open {
        coexisting_window_width(monitor.x, CARD_W)
    } else {
        CARD_W.min((monitor.x - WINDOW_EDGE_MARGIN).max(MIN_PARALLEL_WINDOW_WIDTH))
    };
    let position = notification_position(monitor, overlay_width, app.settings.open);

    apply_root_chrome(ctx, RootViewportMode::Notification.chrome());
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
        overlay_width,
        app.overlay.desired_height(),
    )));
}

pub(in crate::app) fn coexisting_window_width(monitor_width: f32, preferred_width: f32) -> f32 {
    let available_per_window =
        ((monitor_width - (WINDOW_EDGE_MARGIN * 2.0) - WINDOW_GAP) * 0.5)
            .max(MIN_PARALLEL_WINDOW_WIDTH);
    preferred_width.min(available_per_window)
}

fn notification_position(
    monitor: egui::Vec2,
    notification_width: f32,
    settings_open: bool,
) -> egui::Pos2 {
    if settings_open {
        egui::pos2(
            (monitor.x - notification_width - WINDOW_EDGE_MARGIN).max(0.0),
            WINDOW_EDGE_MARGIN,
        )
    } else {
        egui::pos2(((monitor.x - notification_width) * 0.5).max(0.0), 70.0)
    }
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
    fn settings_never_suppress_the_notification_viewport() {
        assert_eq!(
            RootViewportMode::from_notification_visibility(true),
            RootViewportMode::Notification
        );
        let commands = root_commands(RootViewportMode::Notification);
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

    #[test]
    fn settings_and_notifications_are_positioned_side_by_side() {
        let monitor = egui::vec2(1920.0, 1080.0);
        let width = 560.0;
        let without_settings = notification_position(monitor, width, false);
        let with_settings = notification_position(monitor, width, true);

        assert_eq!(without_settings, egui::pos2(680.0, 70.0));
        assert_eq!(with_settings, egui::pos2(1336.0, 24.0));
    }

    #[test]
    fn parallel_windows_share_narrow_monitors_without_forcing_exclusion() {
        assert_eq!(coexisting_window_width(1366.0, 760.0), 647.0);
        assert_eq!(coexisting_window_width(1920.0, 760.0), 760.0);
    }
}
