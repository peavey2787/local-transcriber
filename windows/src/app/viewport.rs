//! Presentation policy for the single persistent Windows root viewport.
//!
//! The native root window is never destroyed during normal operation. It moves
//! between three presentations: a tiny background control surface, the status
//! notification, and Settings. Reusing the same native window avoids the child
//! viewport create/close/recreate path that can stall eframe on Windows.

use eframe::egui::{self, ViewportCommand};

use crate::overlay::CARD_W;

use super::controller::LocalSttApp;

pub(crate) const CONTROL_VIEWPORT_POSITION: [f32; 2] = [0.0, 0.0];
pub(crate) const CONTROL_VIEWPORT_SIZE: [f32; 2] = [8.0, 8.0];
const WINDOW_EDGE_MARGIN: f32 = 24.0;
const MIN_WINDOW_WIDTH: f32 = 360.0;
const SETTINGS_W: f32 = 720.0;
const SETTINGS_H: f32 = 780.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootViewportMode {
    Control,
    Notification,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewportChrome {
    title: &'static str,
    transparent: bool,
    decorations: bool,
    mouse_passthrough: bool,
    resizable: bool,
    close: bool,
    minimized: bool,
    maximize: bool,
    level: egui::WindowLevel,
}

impl RootViewportMode {
    fn from_state(settings_visible: bool, notification_visible: bool) -> Self {
        if settings_visible {
            Self::Settings
        } else if notification_visible {
            Self::Notification
        } else {
            Self::Control
        }
    }

    fn chrome(self) -> ViewportChrome {
        match self {
            Self::Control => ViewportChrome {
                title: "local-stt",
                transparent: true,
                decorations: false,
                mouse_passthrough: true,
                resizable: false,
                close: false,
                minimized: false,
                maximize: false,
                level: egui::WindowLevel::AlwaysOnTop,
            },
            Self::Notification => ViewportChrome {
                title: "local-stt",
                transparent: true,
                decorations: false,
                mouse_passthrough: false,
                resizable: false,
                close: false,
                minimized: false,
                maximize: false,
                level: egui::WindowLevel::AlwaysOnTop,
            },
            Self::Settings => ViewportChrome {
                title: "local-stt settings",
                transparent: false,
                decorations: true,
                mouse_passthrough: false,
                resizable: false,
                close: true,
                minimized: true,
                maximize: false,
                level: egui::WindowLevel::Normal,
            },
        }
    }
}

impl LocalSttApp {
    pub(super) fn sync_root_viewport(&mut self, ctx: &egui::Context) {
        let mode = RootViewportMode::from_state(
            self.settings_window.is_visible(),
            self.overlay.is_visible(),
        );
        match mode {
            RootViewportMode::Control => configure_control_viewport(ctx),
            RootViewportMode::Notification => configure_notification_viewport(self, ctx),
            RootViewportMode::Settings => configure_settings_viewport(self, ctx),
        }
    }
}

fn apply_root_chrome(ctx: &egui::Context, chrome: ViewportChrome) {
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Title(chrome.title.to_string()));
    ctx.send_viewport_cmd(ViewportCommand::Transparent(chrome.transparent));
    ctx.send_viewport_cmd(ViewportCommand::Decorations(chrome.decorations));
    ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(chrome.mouse_passthrough));
    ctx.send_viewport_cmd(ViewportCommand::Resizable(chrome.resizable));
    ctx.send_viewport_cmd(ViewportCommand::EnableButtons {
        close: chrome.close,
        minimized: chrome.minimized,
        maximize: chrome.maximize,
    });
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(chrome.level));
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
    let width = CARD_W.min((monitor.x - WINDOW_EDGE_MARGIN).max(MIN_WINDOW_WIDTH));
    let position = egui::pos2(((monitor.x - width) * 0.5).max(0.0), 70.0);

    apply_root_chrome(ctx, RootViewportMode::Notification.chrome());
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
        width,
        app.overlay.desired_height(),
    )));
}

fn configure_settings_viewport(app: &mut LocalSttApp, ctx: &egui::Context) {
    let (position, size) = settings_viewport_geometry(monitor_size(ctx));
    apply_root_chrome(ctx, RootViewportMode::Settings.chrome());
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
    if app.settings_window.take_focus_request() {
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }
}

fn settings_viewport_geometry(monitor: egui::Vec2) -> (egui::Pos2, egui::Vec2) {
    let width = SETTINGS_W.min((monitor.x - WINDOW_EDGE_MARGIN).max(MIN_WINDOW_WIDTH));
    let height = SETTINGS_H.min((monitor.y - 40.0).max(420.0));
    let x = ((monitor.x - width) * 0.5).max(0.0);
    let y = ((monitor.y - height) * 0.35).max(20.0);
    (egui::pos2(x, y), egui::vec2(width, height))
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
    fn root_window_transitions_between_control_settings_and_notifications() {
        assert_eq!(
            RootViewportMode::from_state(false, false),
            RootViewportMode::Control
        );
        assert_eq!(
            RootViewportMode::from_state(true, false),
            RootViewportMode::Settings
        );
        assert_eq!(
            RootViewportMode::from_state(false, true),
            RootViewportMode::Notification
        );
    }

    #[test]
    fn settings_take_priority_without_creating_a_child_viewport() {
        assert_eq!(
            RootViewportMode::from_state(true, true),
            RootViewportMode::Settings
        );
        let commands = root_commands(RootViewportMode::Settings);
        assert!(commands
            .iter()
            .any(|command| matches!(command, ViewportCommand::Decorations(true))));
        assert!(commands.iter().any(|command| matches!(
            command,
            ViewportCommand::EnableButtons { close: true, .. }
        )));
        assert!(commands.iter().any(|command| matches!(
            command,
            ViewportCommand::WindowLevel(egui::WindowLevel::Normal)
        )));
    }

    #[test]
    fn control_viewport_stays_alive_and_click_through() {
        assert!(CONTROL_VIEWPORT_POSITION
            .into_iter()
            .all(|value| value >= 0.0));
        assert!(CONTROL_VIEWPORT_SIZE.into_iter().all(|value| value > 0.0));
        assert!(RootViewportMode::Control.chrome().mouse_passthrough);
    }

    #[test]
    fn notification_viewport_remains_interactive_and_borderless() {
        let commands = root_commands(RootViewportMode::Notification);
        assert!(commands
            .iter()
            .any(|command| matches!(command, ViewportCommand::MousePassthrough(false))));
        assert!(commands
            .iter()
            .any(|command| matches!(command, ViewportCommand::Decorations(false))));
    }

    #[test]
    fn settings_window_is_centered_and_bounded_to_the_monitor() {
        let (position, size) = settings_viewport_geometry(egui::vec2(1920.0, 1080.0));
        assert_eq!(size, egui::vec2(SETTINGS_W, SETTINGS_H));
        assert_eq!(position.x, 600.0);
        assert!(position.y >= 20.0);

        let (_, compact_size) = settings_viewport_geometry(egui::vec2(640.0, 480.0));
        assert!(compact_size.x <= 616.0);
        assert!(compact_size.y <= 440.0);
    }
}
