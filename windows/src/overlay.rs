//! Floating visual status overlay.

use egui::{self, Color32, CornerRadius, Frame, RichText, Sense, Stroke, Vec2};

pub const CARD_W: f32 = 760.0;
pub const CARD_H: f32 = 112.0;
pub const CARD_H_RESULT: f32 = 380.0;

const BG: Color32 = Color32::from_rgb(0x0E, 0x0F, 0x0F);
const BORDER: Color32 = Color32::from_rgb(0x2A, 0x2A, 0x2A);
const TEXT: Color32 = Color32::from_rgb(0xE8, 0xDC, 0xC8);
const SUBTEXT: Color32 = Color32::from_rgb(0x8A, 0x8A, 0x8A);
const ACCENT: Color32 = Color32::from_rgb(0xD6, 0x9E, 0x54);
const RED: Color32 = Color32::from_rgb(0xD7, 0x5D, 0x5D);
const GREEN: Color32 = Color32::from_rgb(0x70, 0xC4, 0x87);

#[derive(Debug, Clone, PartialEq)]
pub enum OverlayState {
    Hidden,
    Loading {
        message: String,
    },
    Listening,
    Processing,
    Notice {
        message: String,
        ok: bool,
    },
    Result {
        text: String,
        ok: bool,
        footer: String,
        interacted: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayAction {
    CopyDone(String),
}

pub struct Overlay {
    pub state: OverlayState,
    pub rms: f32,
    pub alpha: f32,
    dismiss_at: Option<f64>,
    phase: f32,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            state: OverlayState::Hidden,
            rms: 0.0,
            alpha: 0.0,
            dismiss_at: None,
            phase: 0.0,
        }
    }
}

impl Overlay {
    pub fn show_loading(&mut self, message: impl Into<String>) {
        self.state = OverlayState::Loading {
            message: message.into(),
        };
        self.dismiss_at = None;
    }

    pub fn show_listening(&mut self) {
        self.state = OverlayState::Listening;
        self.dismiss_at = None;
    }

    pub fn show_processing(&mut self) {
        self.state = OverlayState::Processing;
        self.dismiss_at = None;
    }

    pub fn show_notice(&mut self, message: impl Into<String>, ok: bool, now: f64, seconds: f64) {
        self.state = OverlayState::Notice {
            message: message.into(),
            ok,
        };
        self.dismiss_at = Some(now + seconds);
    }

    pub fn show_persistent_notice(&mut self, message: impl Into<String>, ok: bool) {
        self.state = OverlayState::Notice {
            message: message.into(),
            ok,
        };
        self.dismiss_at = None;
    }

    pub fn show_result(
        &mut self,
        text: String,
        ok: bool,
        footer: impl Into<String>,
        now: f64,
        seconds: f64,
    ) {
        self.state = OverlayState::Result {
            text,
            ok,
            footer: footer.into(),
            interacted: false,
        };
        self.dismiss_at = Some(now + seconds);
    }

    pub fn dismiss(&mut self) {
        self.state = OverlayState::Hidden;
        self.dismiss_at = None;
    }

    pub fn dismiss_immediately(&mut self) {
        self.state = OverlayState::Hidden;
        self.dismiss_at = None;
        self.alpha = 0.0;
        self.rms = 0.0;
    }

    pub fn is_visible(&self) -> bool {
        !matches!(&self.state, OverlayState::Hidden) || self.alpha > 0.01
    }

    pub fn desired_height(&self) -> f32 {
        match &self.state {
            OverlayState::Result { .. } => CARD_H_RESULT,
            _ => CARD_H,
        }
    }

    pub fn tick(&mut self, now: f64, dt: f32) {
        let target = if matches!(&self.state, OverlayState::Hidden) {
            0.0
        } else {
            0.95
        };
        let step = 0.12;
        if self.alpha < target {
            self.alpha = (self.alpha + step).min(target);
        } else if self.alpha > target {
            self.alpha = (self.alpha - step).max(target);
        }
        self.phase += dt * 8.0;
        if let Some(t) = self.dismiss_at {
            if now >= t {
                self.dismiss();
            }
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) -> Option<OverlayAction> {
        let (status, accent) = self.status_visuals();
        let mut action = None;
        let mut pin_result = false;

        Frame::NONE
            .fill(BORDER)
            .corner_radius(CornerRadius::same(18))
            .inner_margin(1.0)
            .show(ui, |ui| {
                Frame::NONE
                    .fill(BG)
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(egui::Margin::symmetric(18, 12))
                    .show(ui, |ui| {
                        self.draw_status_row(ui, &status, accent);
                        let result = Self::draw_result_panel(&mut self.state, ui);
                        action = result.0;
                        pin_result = result.1;
                    });
            });

        if pin_result {
            self.dismiss_at = None;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.dismiss();
        }
        action
    }

    fn status_visuals(&self) -> (String, Color32) {
        match &self.state {
            OverlayState::Hidden => (String::new(), SUBTEXT),
            OverlayState::Loading { message } => (message.clone(), ACCENT),
            OverlayState::Listening => ("Recording — press the hotkey again to stop".into(), RED),
            OverlayState::Processing => ("Transcribing…".into(), ACCENT),
            OverlayState::Notice { message, ok } => {
                (message.clone(), if *ok { GREEN } else { RED })
            }
            OverlayState::Result { ok, .. } => (
                (if *ok { "Done" } else { "Nothing heard" }).into(),
                if *ok { GREEN } else { RED },
            ),
        }
    }

    fn draw_status_row(&mut self, ui: &mut egui::Ui, status: &str, accent: Color32) {
        let available = ui.available_width();
        let meter_width = 64.0;
        let text_width = (available - meter_width - 104.0).max(140.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("●").size(18.0).color(accent));
            ui.add_space(8.0);
            ui.add_sized(
                [text_width, 58.0],
                egui::Label::new(RichText::new(status).size(15.0).color(TEXT)).wrap(),
            );
            ui.add_space(8.0);
            self.draw_bars(ui, accent);
        });
    }

    fn draw_result_panel(
        state: &mut OverlayState,
        ui: &mut egui::Ui,
    ) -> (Option<OverlayAction>, bool) {
        let OverlayState::Result {
            text,
            ok,
            footer,
            interacted,
        } = state
        else {
            return (None, false);
        };

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(8.0);

        let mut pin_result = false;
        if *ok {
            Frame::NONE
                .fill(Color32::from_rgb(0x16, 0x17, 0x17))
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(10.0)
                .show(ui, |ui| {
                    let editor_width = ui.available_width();
                    let response = ui.add_sized(
                        [editor_width, 170.0],
                        egui::TextEdit::multiline(text)
                            .desired_width(f32::INFINITY)
                            .desired_rows(7)
                            .hint_text("Edit the transcription here"),
                    );
                    if response.clicked() || response.has_focus() || response.changed() {
                        *interacted = true;
                        pin_result = true;
                    }
                });
        } else {
            let message_width = ui.available_width();
            ui.add_sized(
                [message_width, 130.0],
                egui::Label::new(
                    RichText::new("No speech was detected.")
                        .size(14.0)
                        .color(SUBTEXT),
                )
                .wrap(),
            );
        }

        ui.add_space(8.0);
        let mut action = None;
        ui.horizontal(|ui| {
            let footer_text = if *interacted {
                "Editing — click Copy / Done to update the clipboard"
            } else {
                footer.as_str()
            };
            let label_width = (ui.available_width() - 120.0).max(100.0);
            ui.add_sized(
                [label_width, 28.0],
                egui::Label::new(
                    RichText::new(format!("{footer_text}  ·  Esc to close"))
                        .size(11.0)
                        .color(SUBTEXT),
                )
                .wrap(),
            );
            if *ok && ui.button("Copy / Done").clicked() {
                action = Some(OverlayAction::CopyDone(text.clone()));
            }
        });
        (action, pin_result)
    }

    fn draw_bars(&mut self, ui: &mut egui::Ui, color: Color32) {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(64.0, 42.0), Sense::hover());
        let painter = ui.painter_at(rect);
        let rms = (self.rms * 6.0).clamp(0.0, 1.0);
        for i in 0..7 {
            let t = match &self.state {
                OverlayState::Listening => {
                    let jitter = ((self.phase + i as f32) * 3.1).sin() * 0.04;
                    (rms + (i as f32 - 3.0) * 0.06 + jitter).clamp(0.0, 1.0)
                }
                OverlayState::Loading { .. } | OverlayState::Processing => {
                    ((self.phase + i as f32 * 0.8).sin() * 0.5 + 0.5).clamp(0.0, 1.0)
                }
                _ => 0.15,
            };
            let h = 7.0 + t * 30.0;
            let x = rect.left() + 2.0 + i as f32 * 8.5;
            let y = rect.center().y - h * 0.5;
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(5.5, h)),
                CornerRadius::same(2),
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untouched_result_uses_supplied_duration() {
        let mut overlay = Overlay::default();
        overlay.show_result("hello".into(), true, "Copied to clipboard", 10.0, 3.0);
        overlay.tick(13.1, 0.016);
        assert!(matches!(overlay.state, OverlayState::Hidden));
    }

    #[test]
    fn interacted_result_stays_open() {
        let mut overlay = Overlay::default();
        overlay.show_result("hello".into(), true, "Copied to clipboard", 10.0, 6.0);
        if let OverlayState::Result { interacted, .. } = &mut overlay.state {
            *interacted = true;
        }
        overlay.dismiss_at = None;
        overlay.tick(100.0, 0.016);
        assert!(matches!(overlay.state, OverlayState::Result { .. }));
    }

    #[test]
    fn immediate_dismiss_removes_all_visible_notification_state() {
        let mut overlay = Overlay::default();
        overlay.show_notice("ready", true, 0.0, 30.0);
        overlay.alpha = 0.95;
        overlay.rms = 0.75;

        overlay.dismiss_immediately();

        assert!(matches!(overlay.state, OverlayState::Hidden));
        assert_eq!(overlay.alpha, 0.0);
        assert_eq!(overlay.rms, 0.0);
        assert!(!overlay.is_visible());
    }

    #[test]
    fn persistent_notice_does_not_expire() {
        let mut overlay = Overlay::default();
        overlay.show_persistent_notice("Choose another shortcut", false);
        overlay.tick(10_000.0, 0.016);
        assert!(matches!(overlay.state, OverlayState::Notice { .. }));
    }

    #[test]
    fn result_reserves_room_for_the_editor_and_action_row() {
        let mut overlay = Overlay::default();
        overlay.show_result("hello".into(), true, "Copied to clipboard", 10.0, 3.0);
        assert_eq!(overlay.desired_height(), CARD_H_RESULT);
        assert!(overlay.desired_height() >= 380.0);
    }
}
