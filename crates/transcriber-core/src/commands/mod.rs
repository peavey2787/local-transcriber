//! Voice-command matching, validation, and ordered execution workflow.

mod voice;

pub use voice::{
    execute_command, matching_command, normalize_phrase, validate_command_list,
    CommandExecutionResult, CommandWorker, ScriptRunner,
};
