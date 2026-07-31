//! Native presentation policy for the single persistent Windows root window.
//!
//! The root window is created once and never destroyed during normal operation.
//! Settings and notifications are mutually exclusive presentations of that same
//! borderless window. Geometry is updated only when presentation changes.

use eframe::egui::{self, ViewportCommand};

use crate::overlay::CARD_W;
use crate::platform::primary_display_size_points;

use super::controller::LocalSttApp;

pub(crate) const CONTROL_VIEWPORT_POSITION: [f32; 2] = [-32_000.0, -32_000.0];
pub(crate) const CONTROL_VIEWPORT_SIZE: [f32; 2] = [8.0, 8.0];
const WINDOW_EDGE_MARGIN: f32 = 24.0;
const NOTIFICATION_TOP_MARGIN: f32 = 48.0;
const MIN_WINDOW_WIDTH: f32 = 360.0;
const MIN_SETTINGS_HEIGHT: f32 = 420.0;
const SETTINGS_W: f32 = 720.0;
const SETTINGS_H: f32 = 780.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootViewportMode {
    Control,
    Notification,
    Settings,
}

#[derive(Debug, Default)]
pub(super) struct RootViewportState {
    applied_mode: Option<RootViewportMode>,
    notification_height: Option<u32>,
}

impl RootViewportState {
    fn mode_changed(&mut self, mode: RootViewportMode) -> bool {
        if self.applied_mode == Some(mode) {
            return false;
        }
        self.applied_mode = Some(mode);
        self.notification_height = None;
        true
    }

    fn notification_height_changed(&mut self, height: f32) -> bool {
        let rounded = height.max(1.0).round() as u32;
        if self.notification_height == Some(rounded) {
            return false;
        }
        self.notification_height = Some(rounded);
        true
    }
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
}

impl LocalSttApp {
    pub(super) fn sync_root_viewport(&mut self, ctx: &egui::Context) {
        let mode = RootViewportMode::from_state(
            self.settings_window.is_visible(),
            self.overlay.is_visible(),
        );
        let mode_changed = self.viewport.mode_changed(mode);

        match mode {
            RootViewportMode::Control if mode_changed => configure_control_viewport(ctx),
            RootViewportMode::Notification => {
                let height = self.overlay.desired_height();
                let height_changed = self.viewport.notification_height_changed(height);
                if mode_changed || height_changed {
                    configure_notification_viewport(height, ctx);
                }
            }
            RootViewportMode::Settings => {
                if mode_changed {
                    configure_settings_viewport(ctx);
                }
                if self.settings_window.take_focus_request() {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
            }
            RootViewportMode::Control => {}
        }
    }
}

fn configure_control_viewport(ctx: &egui::Context) {
    apply_borderless_window_policy(ctx, egui::WindowLevel::Normal);
    ctx.send_viewport_cmd(ViewportCommand::Title("local-stt".to_string()));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
        CONTROL_VIEWPORT_SIZE[0],
        CONTROL_VIEWPORT_SIZE[1],
    )));
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(
        CONTROL_VIEWPORT_POSITION[0],
        CONTROL_VIEWPORT_POSITION[1],
    )));
}

fn configure_notification_viewport(height: f32, ctx: &egui::Context) {
    let (position, size) = notification_viewport_geometry(display_size(), height);
    apply_borderless_window_policy(ctx, egui::WindowLevel::AlwaysOnTop);
    ctx.send_viewport_cmd(ViewportCommand::Title("local-stt notification".to_string()));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
}

fn configure_settings_viewport(ctx: &egui::Context) {
    let (position, size) = settings_viewport_geometry(display_size());
    apply_borderless_window_policy(ctx, egui::WindowLevel::AlwaysOnTop);
    ctx.send_viewport_cmd(ViewportCommand::Title("local-stt settings".to_string()));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
}

fn apply_borderless_window_policy(ctx: &egui::Context, level: egui::WindowLevel) {
    ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
    ctx.send_viewport_cmd(ViewportCommand::Transparent(true));
    ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(level));
}

fn notification_viewport_geometry(
    display: egui::Vec2,
    requested_height: f32,
) -> (egui::Pos2, egui::Vec2) {
    let width =
        CARD_W.min((display.x - WINDOW_EDGE_MARGIN * 2.0).max(MIN_WINDOW_WIDTH));
    let height =
        requested_height.min((display.y - WINDOW_EDGE_MARGIN * 2.0).max(1.0));
    let x = ((display.x - width) * 0.5).max(0.0);
    let y = NOTIFICATION_TOP_MARGIN.min((display.y - height).max(0.0));
    (egui::pos2(x, y), egui::vec2(width, height))
}

fn settings_viewport_geometry(display: egui::Vec2) -> (egui::Pos2, egui::Vec2) {
    let width =
        SETTINGS_W.min((display.x - WINDOW_EDGE_MARGIN * 2.0).max(MIN_WINDOW_WIDTH));
    let height = SETTINGS_H.min(
        (display.y - WINDOW_EDGE_MARGIN * 2.0).max(MIN_SETTINGS_HEIGHT),
    );
    let x = ((display.x - width) * 0.5).max(0.0);
    let y = ((display.y - height) * 0.5).max(0.0);
    (egui::pos2(x, y), egui::vec2(width, height))
}

fn display_size() -> egui::Vec2 {
    let [width, height] = primary_display_size_points();
    egui::vec2(width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_and_notifications_are_mutually_exclusive() {
        assert_eq!(
            RootViewportMode::from_state(true, true),
            RootViewportMode::Settings
        );
        assert_eq!(
            RootViewportMode::from_state(false, true),
            RootViewportMode::Notification
        );
        assert_eq!(
            RootViewportMode::from_state(false, false),
            RootViewportMode::Control
        );
    }

    #[test]
    fn viewport_commands_are_applied_only_on_state_changes() {
        let mut state = RootViewportState::default();
        assert!(state.mode_changed(RootViewportMode::Settings));
        assert!(!state.mode_changed(RootViewportMode::Settings));
        assert!(state.mode_changed(RootViewportMode::Control));
    }

    #[test]
    fn notification_resize_is_coalesced() {
        let mut state = RootViewportState::default();
        assert!(state.notification_height_changed(210.0));
        assert!(!state.notification_height_changed(210.2));
        assert!(state.notification_height_changed(360.0));
    }

    #[test]
    fn background_control_window_is_parked_off_screen() {
        assert!(CONTROL_VIEWPORT_POSITION.into_iter().all(|value| value < 0.0));
        assert!(CONTROL_VIEWPORT_SIZE.into_iter().all(|value| value > 0.0));
    }

    #[test]
    fn notification_is_top_centered() {
        let (position, size) =
            notification_viewport_geometry(egui::vec2(1920.0, 1080.0), 112.0);
        assert_eq!(size, egui::vec2(CARD_W, 112.0));
        assert_eq!(position, egui::pos2(580.0, NOTIFICATION_TOP_MARGIN));
    }

    #[test]
    fn settings_window_is_centered_on_both_axes() {
        let (position, size) = settings_viewport_geometry(egui::vec2(1920.0, 1080.0));
        assert_eq!(size, egui::vec2(SETTINGS_W, SETTINGS_H));
        assert_eq!(position, egui::pos2(600.0, 150.0));

        let (compact_position, compact_size) =
            settings_viewport_geometry(egui::vec2(640.0, 480.0));
        assert!(compact_size.x <= 592.0);
        assert!(compact_size.y <= 432.0);
        assert_eq!(compact_position.x, (640.0 - compact_size.x) * 0.5);
        assert_eq!(compact_position.y, (480.0 - compact_size.y) * 0.5);
    }
}
