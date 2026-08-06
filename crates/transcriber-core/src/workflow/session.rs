//! Pure recording-session state and ordered chunk aggregation.

use std::collections::BTreeMap;

use crate::audio::SAMPLE_RATE;

pub const LIVE_CHUNK_SECONDS: u32 = 10;
pub const LIVE_CHUNK_SAMPLES: usize =
    (SAMPLE_RATE as usize) * (LIVE_CHUNK_SECONDS as usize);
pub const MIN_FINAL_CHUNK_SAMPLES: usize = (SAMPLE_RATE as usize) * 3 / 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingPurpose {
    Transcription,
    VoiceCommand,
}

pub struct LiveSession {
    pub id: u64,
    pub purpose: RecordingPurpose,
    pub next_chunk_id: usize,
    pub in_flight: usize,
    pub done: BTreeMap<usize, Result<String, String>>,
    pub expected: Option<usize>,
    pub finishing: bool,
}

impl LiveSession {
    pub fn new(id: u64, purpose: RecordingPurpose) -> Self {
        Self {
            id,
            purpose,
            next_chunk_id: 0,
            in_flight: 0,
            done: BTreeMap::new(),
            expected: None,
            finishing: false,
        }
    }

    pub fn schedule_chunk(&mut self) -> usize {
        let chunk_id = self.next_chunk_id;
        self.next_chunk_id += 1;
        self.in_flight += 1;
        chunk_id
    }

    pub fn mark_queue_failure(&mut self, chunk_id: usize, message: String) {
        self.in_flight = self.in_flight.saturating_sub(1);
        self.done.insert(chunk_id, Err(message));
    }

    pub fn finish_schedule(&mut self) -> (usize, usize) {
        let chunk_id = self.schedule_chunk();
        self.finishing = true;
        let expected = chunk_id + 1;
        self.expected = Some(expected);
        (chunk_id, expected)
    }

    pub fn complete_chunk(&mut self, chunk_id: usize, result: Result<String, String>) {
        self.in_flight = self.in_flight.saturating_sub(1);
        self.done.insert(chunk_id, result);
    }

    pub fn all_done(&self) -> bool {
        self.expected
            .is_some_and(|expected| self.done.len() >= expected && self.in_flight == 0)
    }

    pub fn joined(&self) -> String {
        self.done
            .values()
            .filter_map(|result| result.as_ref().ok())
            .filter(|text| !text.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn errors(&self) -> Vec<String> {
        self.done
            .iter()
            .filter_map(|(chunk_id, result)| {
                result
                    .as_ref()
                    .err()
                    .map(|error| format!("chunk #{chunk_id}: {error}"))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_preserves_chunk_order_and_reports_errors() {
        let mut session = LiveSession::new(7, RecordingPurpose::Transcription);
        session.done.insert(2, Ok("third".into()));
        session.done.insert(0, Ok("first".into()));
        session.done.insert(1, Err("decoder failed".into()));

        assert_eq!(session.joined(), "first third");
        assert_eq!(session.errors(), vec!["chunk #1: decoder failed".to_string()]);
    }

    #[test]
    fn session_is_complete_only_after_expected_work_finishes() {
        let mut session = LiveSession::new(9, RecordingPurpose::Transcription);
        session.finishing = true;
        session.expected = Some(2);
        session.in_flight = 1;
        session.done.insert(0, Ok(String::new()));
        assert!(!session.all_done());

        session.in_flight = 0;
        session.done.insert(1, Ok(String::new()));
        assert!(session.all_done());
    }
}
