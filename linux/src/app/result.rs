//! Clipboard, auto-paste, and editable-result presentation.

use arboard::Clipboard;

use super::controller::LocalSttApp;

impl LocalSttApp {
    pub(super) fn present_transcription(&mut self, text: String) {
        let now = self.now();
        if text.is_empty() {
            self.paste_target = None;
            self.present_no_speech(now);
            return;
        }

        let text = prepare_transcription_text(text, self.config.append_trailing_space);
        log::info!(
            "transcription completed ({} characters)",
            text.chars().count()
        );
        match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(&text)) {
            Ok(()) if self.config.auto_paste => self.finish_auto_paste(now),
            Ok(()) => {
                if self.config.show_result_notifications {
                    self.overlay.show_result(
                        text,
                        true,
                        "Copied to clipboard",
                        now,
                        self.config.notification_seconds(),
                    );
                } else {
                    self.overlay.dismiss();
                }
            }
            Err(error) if self.config.auto_paste => {
                self.paste_target = None;
                self.show_result_error(
                    format!(
                        "Could not copy the transcription, so auto-paste was not attempted: {error}"
                    ),
                    now,
                );
            }
            Err(error) => {
                if self.config.show_result_notifications {
                    self.overlay.show_result(
                        text,
                        true,
                        format!("Clipboard error: {error}"),
                        now,
                        self.config.notification_seconds(),
                    );
                } else {
                    self.overlay.dismiss();
                }
            }
        }
    }

    pub(super) fn present_transcription_failure(
        &mut self,
        partial_text: String,
        errors: Vec<String>,
    ) {
        let now = self.now();
        let summary = format!(
            "Transcription failed for {} audio chunk(s).",
            errors.len()
        );
        for error in &errors {
            log::error!("{error}");
        }

        if !self.config.show_result_notifications {
            self.overlay.dismiss();
        } else if self.config.auto_paste || partial_text.is_empty() {
            self.overlay.show_notice(
                format!("{summary} Nothing was pasted."),
                false,
                now,
                self.config.notification_seconds(),
            );
        } else {
            self.overlay.show_result(
                partial_text,
                true,
                format!("Partial transcription — {summary} Review it, then click Copy / Done"),
                now,
                self.config.notification_seconds(),
            );
        }
    }

    pub(super) fn copy_edited_result(&mut self, text: String) {
        match Clipboard::new().and_then(|mut clipboard| clipboard.set_text(&text)) {
            Ok(()) => {
                log::info!("edited transcription copied to clipboard");
                self.overlay.dismiss();
            }
            Err(error) => self.overlay.show_notice(
                format!("Could not update the clipboard: {error}"),
                false,
                self.now(),
                self.config.notification_seconds(),
            ),
        }
    }

    fn finish_auto_paste(&mut self, now: f64) {
        let target = self.paste_target.take().unwrap_or_default();
        match target.paste_ctrl_v() {
            Ok(backend) => {
                log::info!("transcription auto-pasted with {backend}");
                if self.config.press_enter_after_paste {
                    if let Err(error) = target.press_enter() {
                        self.show_result_error(
                            format!("The transcription was pasted, but Enter could not be sent: {error}"),
                            now,
                        );
                        return;
                    }
                    log::info!("Enter sent after automatic paste");
                }
                // Direct insertion intentionally bypasses the editable result window.
                self.overlay.dismiss();
            }
            Err(error) => self.show_result_error(
                format!("The transcription was copied, but auto-paste failed: {error}"),
                now,
            ),
        }
    }

    fn present_no_speech(&mut self, now: f64) {
        log::info!("recording completed without detected speech");
        if !self.config.show_result_notifications {
            self.overlay.dismiss();
            return;
        }
        if self.config.auto_paste {
            self.overlay
                .show_notice(
                    "No speech was detected",
                    false,
                    now,
                    self.config.notification_seconds(),
                );
        } else {
            self.overlay
                .show_result(
                    String::new(),
                    false,
                    "No speech was detected",
                    now,
                    self.config.notification_seconds(),
                );
        }
    }

    fn show_result_error(&mut self, message: String, now: f64) {
        log::error!("result delivery failed: {message}");
        if self.config.show_result_notifications {
            self.overlay.show_notice(
                message,
                false,
                now,
                self.config.notification_seconds(),
            );
        } else {
            self.overlay.dismiss();
        }
    }
}

fn prepare_transcription_text(mut text: String, append_trailing_space: bool) -> String {
    if append_trailing_space
        && !text.is_empty()
        && !text.chars().last().is_some_and(char::is_whitespace)
    {
        text.push(' ');
    }
    text
}

#[cfg(test)]
mod delivery_tests {
    use super::prepare_transcription_text;

    #[test]
    fn optional_space_is_added_once_to_non_empty_text() {
        assert_eq!(prepare_transcription_text("hello".into(), true), "hello ");
        assert_eq!(prepare_transcription_text("hello ".into(), true), "hello ");
        assert_eq!(prepare_transcription_text(String::new(), true), "");
        assert_eq!(prepare_transcription_text("hello".into(), false), "hello");
    }
}
