use anyhow::{bail, Context, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::collections::HashSet;
use std::sync::Arc;
use std::thread;

use crate::config::VoiceCommand;

/// Produces a stable phrase key for ASR output and configured aliases.
pub fn normalize_phrase(value: &str) -> String {
    let mut normalized = String::new();
    let mut needs_separator = false;

    for character in value.chars() {
        if matches!(
            character,
            '\u{00ad}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}'
        ) {
            continue;
        }
        if character.is_alphanumeric() {
            if needs_separator && !normalized.is_empty() {
                normalized.push(' ');
            }
            for lowercase in character.to_lowercase() {
                normalized.push(lowercase);
            }
            needs_separator = false;
        } else {
            needs_separator = !normalized.is_empty();
        }
    }

    normalized
}

fn normalized_aliases(value: &str) -> impl Iterator<Item = String> + '_ {
    value
        .split(',')
        .map(normalize_phrase)
        .filter(|alias| !alias.is_empty())
}

pub fn matching_command(commands: &[VoiceCommand], spoken: &str) -> Option<VoiceCommand> {
    let normalized = normalize_phrase(spoken);
    if normalized.is_empty() {
        return None;
    }
    commands
        .iter()
        .find(|command| normalized_aliases(&command.phrase).any(|alias| alias == normalized))
        .cloned()
}

/// Bounded, user-visible output produced by one platform script runner.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScriptOutput {
    pub stdout: String,
    pub stderr: String,
}

impl ScriptOutput {
    pub fn is_empty(&self) -> bool {
        self.stdout.trim().is_empty() && self.stderr.trim().is_empty()
    }

    pub fn display_text(&self) -> String {
        let stdout = self.stdout.trim();
        let stderr = self.stderr.trim();
        match (stdout.is_empty(), stderr.is_empty()) {
            (false, false) => format!("{stdout}\n{stderr}"),
            (false, true) => stdout.to_string(),
            (true, false) => stderr.to_string(),
            (true, true) => String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandOutput {
    pub scripts: Vec<ScriptOutput>,
}

impl CommandOutput {
    pub fn display_text(&self) -> String {
        self.scripts
            .iter()
            .filter(|output| !output.is_empty())
            .map(ScriptOutput::display_text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Focused native boundary for allowed script formats and process launching.
pub trait ScriptRunner: Send + Sync {
    fn validate_script(&self, script: &str) -> Result<()>;
    fn run_script(&self, script: &str) -> Result<ScriptOutput>;
}

#[derive(Debug)]
pub struct CommandExecutionResult {
    pub phrase: String,
    pub result: Result<CommandOutput, String>,
}

pub struct CommandWorker {
    runner: Arc<dyn ScriptRunner>,
    result_tx: Sender<CommandExecutionResult>,
    result_rx: Receiver<CommandExecutionResult>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl CommandWorker {
    pub fn new(runner: Arc<dyn ScriptRunner>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (result_tx, result_rx) = unbounded();
        Self {
            runner,
            result_tx,
            result_rx,
            wake,
        }
    }

    pub fn validate(&self, commands: &[VoiceCommand]) -> Result<()> {
        validate_command_list(commands, self.runner.as_ref())
    }

    pub fn execute(&self, command: VoiceCommand) {
        let runner = self.runner.clone();
        let result_tx = self.result_tx.clone();
        let wake = self.wake.clone();
        thread::spawn(move || {
            let phrase = command.phrase.clone();
            let result =
                execute_command(&command, runner.as_ref()).map_err(|error| format!("{error:#}"));
            let _ = result_tx.send(CommandExecutionResult { phrase, result });
            wake();
        });
    }

    pub fn try_recv(&self) -> Option<CommandExecutionResult> {
        self.result_rx.try_recv().ok()
    }
}

pub fn validate_command_list(commands: &[VoiceCommand], runner: &dyn ScriptRunner) -> Result<()> {
    if commands.is_empty() {
        bail!("Add at least one command before enabling the feature");
    }

    let mut claimed_aliases = HashSet::new();
    for (command_index, command) in commands.iter().enumerate() {
        let aliases = normalized_aliases(&command.phrase).collect::<HashSet<_>>();
        if aliases.is_empty() {
            bail!(
                "Command {} needs at least one spoken word or phrase",
                command_index + 1
            );
        }
        for alias in aliases {
            if !claimed_aliases.insert(alias) {
                bail!("Voice-command phrases must not overlap between commands");
            }
        }
        if command.scripts.is_empty() {
            bail!("Command {:?} needs at least one script", command.phrase);
        }
        for script in &command.scripts {
            runner.validate_script(script)?;
        }
    }
    Ok(())
}

pub fn execute_command(command: &VoiceCommand, runner: &dyn ScriptRunner) -> Result<CommandOutput> {
    let mut output = CommandOutput::default();
    for (index, script) in command.scripts.iter().enumerate() {
        println!(
            "[local-stt] starting voice-command script {}/{}: {}",
            index + 1,
            command.scripts.len(),
            script
        );
        output.scripts.push(
            runner
                .run_script(script)
                .with_context(|| format!("run script {script}"))?,
        );
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AcceptScripts;

    impl ScriptRunner for AcceptScripts {
        fn validate_script(&self, _script: &str) -> Result<()> {
            Ok(())
        }

        fn run_script(&self, _script: &str) -> Result<ScriptOutput> {
            Ok(ScriptOutput::default())
        }
    }

    #[test]
    fn aliases_ignore_case_spacing_punctuation_and_invisible_characters() {
        let commands = vec![VoiceCommand {
            phrase: "Hello, hello there, start report".into(),
            scripts: vec!["report.py".into()],
        }];

        assert!(matching_command(&commands, "HELLO!").is_some());
        assert!(matching_command(&commands, "Hello\u{200b} there?").is_some());
        assert!(matching_command(&commands, "start report.").is_some());
        assert!(matching_command(&commands, "hello report").is_none());
    }

    #[test]
    fn overlapping_aliases_are_rejected() {
        let commands = vec![
            VoiceCommand {
                phrase: "Build App, compile".into(),
                scripts: vec!["one.py".into()],
            },
            VoiceCommand {
                phrase: "compile.".into(),
                scripts: vec!["two.py".into()],
            },
        ];

        assert!(validate_command_list(&commands, &AcceptScripts)
            .unwrap_err()
            .to_string()
            .contains("overlap"));
    }

    #[test]
    fn command_output_combines_nonempty_script_output() {
        let output = CommandOutput {
            scripts: vec![
                ScriptOutput {
                    stdout: "Hello World\n".into(),
                    stderr: String::new(),
                },
                ScriptOutput::default(),
            ],
        };
        assert_eq!(output.display_text(), "Hello World");
    }
}
