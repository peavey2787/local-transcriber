//! Pure transcription-result preparation and presentation decisions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailurePresentation {
    Hidden,
    Notice,
    EditablePartial,
}

pub fn prepare_transcription_text(mut text: String, append_trailing_space: bool) -> String {
    if append_trailing_space
        && !text.is_empty()
        && !text.chars().last().is_some_and(char::is_whitespace)
    {
        text.push(' ');
    }
    text
}

pub fn failure_presentation(
    show_notifications: bool,
    auto_paste: bool,
    partial_text: &str,
) -> FailurePresentation {
    if !show_notifications {
        FailurePresentation::Hidden
    } else if auto_paste || partial_text.is_empty() {
        FailurePresentation::Notice
    } else {
        FailurePresentation::EditablePartial
    }
}

pub fn failure_summary(error_count: usize) -> String {
    format!("Transcription failed for {error_count} audio chunk(s).")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_space_is_added_once_to_non_empty_text() {
        assert_eq!(prepare_transcription_text("hello".into(), true), "hello ");
        assert_eq!(prepare_transcription_text("hello ".into(), true), "hello ");
        assert_eq!(prepare_transcription_text(String::new(), true), "");
        assert_eq!(prepare_transcription_text("hello".into(), false), "hello");
    }

    #[test]
    fn partial_results_are_editable_only_when_delivery_is_manual() {
        assert_eq!(
            failure_presentation(true, false, "partial"),
            FailurePresentation::EditablePartial
        );
        assert_eq!(
            failure_presentation(true, true, "partial"),
            FailurePresentation::Notice
        );
        assert_eq!(
            failure_presentation(false, false, "partial"),
            FailurePresentation::Hidden
        );
    }
}
