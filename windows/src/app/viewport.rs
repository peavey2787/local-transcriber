//! Native-window modes for the persistent root viewport.

use eframe::egui::{self, ViewportCommand};

use crate::overlay::CARD_W;

use super::controller::LocalSttApp;

pub(crate) const CONTROL_VIEWPORT_POSITION: [f32; 2] = [0.0, 0.0];
pub(crate) const CONTROL_VIEWPORT_SIZE: [f32; 2] = [8.0, 8.0];

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
    close_button: bool,
}

impl RootViewportMode {
    fn from_state(settings_open: bool, notification_visible: bool) -> Self {
        if settings_open {
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
                close_button: false,
            },
            Self::Notification => ViewportChrome {
                title: "local-stt",
                transparent: true,
                decorations: false,
                mouse_passthrough: false,
                close_button: false,
            },
            Self::Settings => ViewportChrome {
                title: "local-stt settings",
                transparent: false,
                decorations: true,
                mouse_passthrough: false,
                close_button: true,
            },
        }
    }
}

impl LocalSttApp {
    pub(super) fn sync_root_viewport(&mut self, ctx: &egui::Context) {
        match RootViewportMode::from_state(self.settings.open, self.overlay.is_visible()) {
            RootViewportMode::Control => configure_control_viewport(ctx),
            RootViewportMode::Notification => configure_notification_viewport(self, ctx),
            RootViewportMode::Settings => configure_settings_viewport(self, ctx),
        }
    }
}

fn apply_chrome(ctx: &egui::Context, chrome: ViewportChrome) {
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Title(chrome.title.to_string()));
    ctx.send_viewport_cmd(ViewportCommand::Transparent(chrome.transparent));
    ctx.send_viewport_cmd(ViewportCommand::Decorations(chrome.decorations));
    ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(chrome.mouse_passthrough));
    ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
    ctx.send_viewport_cmd(ViewportCommand::EnableButtons {
        close: chrome.close_button,
        minimized: false,
        maximize: false,
    });
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
}

fn configure_control_viewport(ctx: &egui::Context) {
    apply_chrome(ctx, RootViewportMode::Control.chrome());
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

    apply_chrome(ctx, RootViewportMode::Notification.chrome());
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(x, 70.0)));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
        overlay_width,
        app.overlay.desired_height(),
    )));
}

fn configure_settings_viewport(app: &mut LocalSttApp, ctx: &egui::Context) {
    let (position, size) = settings_viewport_geometry(monitor_size(ctx));

    apply_chrome(ctx, RootViewportMode::Settings.chrome());
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
    focus_settings(ctx, &mut app.settings.focus_pending);
}

fn focus_settings(ctx: &egui::Context, focus_pending: &mut bool) {
    if *focus_pending {
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        *focus_pending = false;
    }
}

fn monitor_size(ctx: &egui::Context) -> egui::Vec2 {
    ctx.input(|input| input.viewport().monitor_size)
        .unwrap_or(egui::vec2(1920.0, 1080.0))
}

fn settings_viewport_geometry(monitor: egui::Vec2) -> (egui::Pos2, egui::Vec2) {
    let width = SETTINGS_W.min((monitor.x - 24.0).max(360.0));
    let height = SETTINGS_H.min((monitor.y - 40.0).max(420.0));
    let x = ((monitor.x - width) * 0.5).max(0.0);
    let y = ((monitor.y - height) * 0.35).max(20.0);
    (egui::pos2(x, y), egui::vec2(width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chrome_commands(mode: RootViewportMode) -> Vec<ViewportCommand> {
        let context = egui::Context::default();
        let mut output = context.run(Default::default(), |ctx| {
            apply_chrome(ctx, mode.chrome());
        });
        output
            .viewport_output
            .remove(&egui::ViewportId::ROOT)
            .expect("root viewport output")
            .commands
    }

    #[test]
    fn settings_can_close_and_reopen_on_the_persistent_root() {
        let sequence = [true, false, true]
            .map(|settings_open| RootViewportMode::from_state(settings_open, false));

        assert_eq!(
            sequence,
            [
                RootViewportMode::Settings,
                RootViewportMode::Control,
                RootViewportMode::Settings,
            ]
        );
        assert!(!sequence[0].chrome().mouse_passthrough);
        assert!(sequence[0].chrome().decorations);
        assert!(sequence[0].chrome().close_button);
    }

    #[test]
    fn settings_and_notifications_have_independent_window_chrome() {
        let settings = RootViewportMode::from_state(true, true).chrome();
        let notification = RootViewportMode::from_state(false, true).chrome();

        assert!(!settings.transparent);
        assert!(settings.decorations);
        assert!(!settings.mouse_passthrough);
        assert!(notification.transparent);
        assert!(!notification.decorations);
        assert!(!notification.mouse_passthrough);
    }

    #[test]
    fn mode_switches_emit_native_interaction_commands() {
        let settings = chrome_commands(RootViewportMode::Settings);
        assert!(settings
            .iter()
            .any(|command| matches!(command, ViewportCommand::Transparent(false))));
        assert!(settings
            .iter()
            .any(|command| matches!(command, ViewportCommand::Decorations(true))));
        assert!(settings
            .iter()
            .any(|command| matches!(command, ViewportCommand::MousePassthrough(false))));
        assert!(settings
            .iter()
            .any(|command| matches!(command, ViewportCommand::EnableButtons { close: true, .. })));

        let notification = chrome_commands(RootViewportMode::Notification);
        assert!(notification
            .iter()
            .any(|command| matches!(command, ViewportCommand::Transparent(true))));
        assert!(notification
            .iter()
            .any(|command| matches!(command, ViewportCommand::Decorations(false))));
        assert!(notification
            .iter()
            .any(|command| matches!(command, ViewportCommand::MousePassthrough(false))));
    }

    #[test]
    fn opening_settings_requests_keyboard_focus_once() {
        let context = egui::Context::default();
        let mut focus_pending = true;
        let mut output = context.run(Default::default(), |ctx| {
            focus_settings(ctx, &mut focus_pending);
        });
        let commands = output
            .viewport_output
            .remove(&egui::ViewportId::ROOT)
            .expect("root viewport output")
            .commands;

        assert!(!focus_pending);
        assert!(commands
            .iter()
            .any(|command| matches!(command, ViewportCommand::Focus)));
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
