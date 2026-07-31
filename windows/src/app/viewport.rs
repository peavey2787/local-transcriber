//! Native presentation policy for the single persistent Windows root window.
//!
//! The root window is created once and never destroyed during normal operation.
//! Settings and notifications are mutually exclusive presentations of that same
//! window. Native geometry is updated only when the presentation changes, which
//! avoids repeatedly rebuilding Windows window chrome on every egui frame.

use eframe::egui::{self, ViewportCommand};

use crate::overlay::CARD_W;

use super::controller::LocalSttApp;

pub(crate) const CONTROL_VIEWPORT_POSITION: [f32; 2] = [-32_000.0, -32_000.0];
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
    ctx.send_viewport_cmd(ViewportCommand::Title("local-stt".to_string()));
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(egui::pos2(
        CONTROL_VIEWPORT_POSITION[0],
        CONTROL_VIEWPORT_POSITION[1],
    )));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(
        CONTROL_VIEWPORT_SIZE[0],
        CONTROL_VIEWPORT_SIZE[1],
    )));
}

fn configure_notification_viewport(height: f32, ctx: &egui::Context) {
    let monitor = monitor_size(ctx);
    let width = CARD_W.min((monitor.x - WINDOW_EDGE_MARGIN).max(MIN_WINDOW_WIDTH));
    let position = egui::pos2(((monitor.x - width) * 0.5).max(0.0), 70.0);

    ctx.send_viewport_cmd(ViewportCommand::Title("local-stt notification".to_string()));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(
        egui::WindowLevel::AlwaysOnTop,
    ));
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(egui::vec2(width, height)));
}

fn configure_settings_viewport(ctx: &egui::Context) {
    let (position, size) = settings_viewport_geometry(monitor_size(ctx));
    ctx.send_viewport_cmd(ViewportCommand::Title("local-stt settings".to_string()));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::WindowLevel(
        egui::WindowLevel::AlwaysOnTop,
    ));
    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(position));
    ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
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
