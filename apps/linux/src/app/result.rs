//! Native clipboard/paste adapter for the shared result-delivery workflow.

use anyhow::{anyhow, Result};
use arboard::Clipboard;
use transcriber_ui::result_delivery::{self, ResultDeliveryHost};

use super::controller::LocalSttApp;

impl ResultDeliveryHost for LocalSttApp {
    fn config(&self) -> &transcriber_core::config::Config {
        &self.config
    }

    fn now(&self) -> f64 {
        LocalSttApp::now(self)
    }

    fn results_suppressed(&self) -> bool {
        self.settings.open || self.voice_commands.open
    }

    fn overlay(&mut self) -> &mut transcriber_ui::overlay::Overlay {
        &mut self.overlay
    }

    fn copy_to_clipboard(&mut self, text: &str) -> Result<()> {
        let mut clipboard = Clipboard::new()?;
        clipboard.set_text(text)?;
        Ok(())
    }

    fn paste_to_captured_target(&mut self, press_enter: bool) -> Result<String> {
        let target = self.paste_target.take().ok_or_else(|| {
            anyhow!("the original paste target is unavailable")
        })?;
        let backend = target.paste_ctrl_v()?;
        if press_enter {
            target.press_enter()?;
        }
        Ok(backend.to_string())
    }

    fn clear_paste_target(&mut self) {
        self.paste_target = None;
    }

    fn dismiss_overlay(&mut self) {
        self.overlay.dismiss();
    }
}

impl LocalSttApp {
    pub(super) fn present_transcription(&mut self, text: String) {
        result_delivery::present_transcription(self, text);
    }

    pub(super) fn present_transcription_failure(
        &mut self,
        partial_text: String,
        errors: Vec<String>,
    ) {
        result_delivery::present_transcription_failure(self, partial_text, errors);
    }

    pub(super) fn copy_edited_result(&mut self, text: String) {
        result_delivery::copy_edited_result(self, text);
    }
}
