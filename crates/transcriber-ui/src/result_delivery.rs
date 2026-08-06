//! Shared transcription-result workflow over focused native delivery services.

use anyhow::Result;
use transcriber_core::config::Config;
use transcriber_core::workflow::{
    failure_presentation, failure_summary, prepare_transcription_text, FailurePresentation,
};

use crate::overlay::Overlay;

/// Native services required by the shared result presenter.
pub trait ResultDeliveryHost {
    fn config(&self) -> &Config;
    fn now(&self) -> f64;
    fn results_suppressed(&self) -> bool;
    fn overlay(&mut self) -> &mut Overlay;
    fn copy_to_clipboard(&mut self, text: &str) -> Result<()>;
    fn paste_to_captured_target(&mut self, press_enter: bool) -> Result<String>;
    fn clear_paste_target(&mut self);
    fn dismiss_overlay(&mut self);
}

pub fn present_transcription(host: &mut impl ResultDeliveryHost, text: String) {
    if host.results_suppressed() {
        host.clear_paste_target();
        host.dismiss_overlay();
        return;
    }

    let now = host.now();
    if text.is_empty() {
        host.clear_paste_target();
        present_no_speech(host, now);
        return;
    }

    let append_space = host.config().append_trailing_space;
    let text = prepare_transcription_text(text, append_space);
    log::info!(
        "transcription completed ({} characters)",
        text.chars().count()
    );

    let auto_paste = host.config().auto_paste;
    match host.copy_to_clipboard(&text) {
        Ok(()) if auto_paste => finish_auto_paste(host, now),
        Ok(()) => {
            if host.config().show_result_notifications {
                let duration = host.config().notification_seconds();
                host.overlay()
                    .show_result(text, true, "Copied to clipboard", now, duration);
            } else {
                host.dismiss_overlay();
            }
        }
        Err(error) if auto_paste => {
            host.clear_paste_target();
            show_result_error(
                host,
                format!(
                    "Could not copy the transcription, so auto-paste was not attempted: {error}"
                ),
                now,
            );
        }
        Err(error) => {
            if host.config().show_result_notifications {
                let duration = host.config().notification_seconds();
                host.overlay().show_result(
                    text,
                    true,
                    format!("Clipboard error: {error}"),
                    now,
                    duration,
                );
            } else {
                host.dismiss_overlay();
            }
        }
    }
}

pub fn present_transcription_failure(
    host: &mut impl ResultDeliveryHost,
    partial_text: String,
    errors: Vec<String>,
) {
    host.clear_paste_target();
    if host.results_suppressed() {
        host.dismiss_overlay();
        return;
    }

    let now = host.now();
    let summary = failure_summary(errors.len());
    for error in &errors {
        log::error!("{error}");
    }

    match failure_presentation(
        host.config().show_result_notifications,
        host.config().auto_paste,
        &partial_text,
    ) {
        FailurePresentation::Hidden => host.dismiss_overlay(),
        FailurePresentation::Notice => {
            let duration = host.config().notification_seconds();
            host.overlay().show_notice(
                format!("{summary} Nothing was pasted."),
                false,
                now,
                duration,
            );
        }
        FailurePresentation::EditablePartial => {
            let duration = host.config().notification_seconds();
            host.overlay().show_result(
                partial_text,
                true,
                format!("Partial transcription — {summary} Review it, then click Copy / Done"),
                now,
                duration,
            );
        }
    }
}

pub fn copy_edited_result(host: &mut impl ResultDeliveryHost, text: String) {
    match host.copy_to_clipboard(&text) {
        Ok(()) => {
            log::info!("edited transcription copied to clipboard");
            host.dismiss_overlay();
        }
        Err(error) => {
            let now = host.now();
            let duration = host.config().notification_seconds();
            host.overlay().show_notice(
                format!("Could not update the clipboard: {error}"),
                false,
                now,
                duration,
            );
        }
    }
}

fn finish_auto_paste(host: &mut impl ResultDeliveryHost, now: f64) {
    let press_enter = host.config().press_enter_after_paste;
    match host.paste_to_captured_target(press_enter) {
        Ok(backend) => {
            log::info!("transcription auto-pasted with {backend}");
            if press_enter {
                log::info!("Enter sent after automatic paste");
            }
            host.dismiss_overlay();
        }
        Err(error) => show_result_error(
            host,
            format!("The transcription was copied, but auto-paste failed: {error}"),
            now,
        ),
    }
}

fn present_no_speech(host: &mut impl ResultDeliveryHost, now: f64) {
    log::info!("recording completed without detected speech");
    if !host.config().show_result_notifications {
        host.dismiss_overlay();
        return;
    }

    let duration = host.config().notification_seconds();
    if host.config().auto_paste {
        host.overlay()
            .show_notice("No speech was detected", false, now, duration);
    } else {
        host.overlay().show_result(
            String::new(),
            false,
            "No speech was detected",
            now,
            duration,
        );
    }
}

fn show_result_error(host: &mut impl ResultDeliveryHost, message: String, now: f64) {
    log::error!("result delivery failed: {message}");
    if host.results_suppressed() {
        host.dismiss_overlay();
    } else if host.config().show_result_notifications {
        let duration = host.config().notification_seconds();
        host.overlay().show_notice(message, false, now, duration);
    } else {
        host.dismiss_overlay();
    }
}
