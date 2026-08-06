//! Safe phrase matching and sequential local script execution for voice commands.

use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Output};

use crate::config::VoiceCommand;

pub(crate) fn normalize_phrase(value: &str) -> String {
    value
        .split_whitespace()
        .map(|word| word.trim_matches(|character: char| character.is_ascii_punctuation()))
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn matching_command(commands: &[VoiceCommand], spoken: &str) -> Option<VoiceCommand> {
    let normalized = normalize_phrase(spoken);
    if normalized.is_empty() {
        return None;
    }
    commands
        .iter()
        .find(|command| normalize_phrase(&command.phrase) == normalized)
        .cloned()
}

pub(crate) fn validate_command_list(commands: &[VoiceCommand]) -> Result<()> {
    if commands.is_empty() {
        bail!("Add at least one command before enabling the feature");
    }
    let mut phrases = HashSet::new();
    for (command_index, command) in commands.iter().enumerate() {
        let normalized = normalize_phrase(&command.phrase);
        if normalized.is_empty() {
            bail!("Command {} needs a spoken word or phrase", command_index + 1);
        }
        if !phrases.insert(normalized) {
            bail!("Each voice-command phrase must be unique");
        }
        if command.scripts.is_empty() {
            bail!("Command {:?} needs at least one script", command.phrase);
        }
        for script in &command.scripts {
            validate_script_path(script)?;
        }
    }
    Ok(())
}

pub(crate) fn execute_command(command: &VoiceCommand) -> Result<()> {
    for script in &command.scripts {
        execute_script(script)
            .with_context(|| format!("run script {} for {:?}", script, command.phrase))?;
    }
    Ok(())
}

fn validate_script_path(script: &str) -> Result<()> {
    let path = Path::new(script.trim());
    if script.trim().is_empty() {
        bail!("Script paths cannot be empty");
    }
    if !path.is_file() {
        bail!("Script does not exist or is not a file: {}", path.display());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "sh" | "bash" | "py") {
        bail!("Linux voice commands support .sh, .bash, and .py files: {}", path.display());
    }
    Ok(())
}

fn execute_script(script: &str) -> Result<()> {
    validate_script_path(script)?;
    let path = Path::new(script.trim());
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let output = match extension.as_str() {
        "sh" | "bash" => Command::new("bash").arg(path).output(),
        "py" => Command::new("python3").arg(path).output(),
        _ => unreachable!("validated extension"),
    }
    .with_context(|| format!("start {}", path.display()))?;
    ensure_success(path, output)
}

fn ensure_success(path: &Path, output: Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        bail!("{} exited with {}", path.display(), output.status);
    }
    bail!("{} failed: {stderr}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_ignores_case_whitespace_and_surrounding_punctuation() {
        let commands = vec![VoiceCommand {
            phrase: "Open Reports".into(),
            scripts: vec!["/tmp/report.sh".into()],
        }];
        assert_eq!(
            matching_command(&commands, "  OPEN   reports. ").unwrap().phrase,
            "Open Reports"
        );
        assert!(matching_command(&commands, "open report").is_none());
    }

    #[test]
    fn empty_command_list_is_rejected() {
        assert!(validate_command_list(&[])
            .unwrap_err()
            .to_string()
            .contains("at least one command"));
    }

    #[test]
    fn duplicate_normalized_phrases_are_rejected_before_path_validation() {
        let commands = vec![
            VoiceCommand {
                phrase: "Build App".into(),
                scripts: vec!["missing.sh".into()],
            },
            VoiceCommand {
                phrase: " build   app ".into(),
                scripts: vec!["other.sh".into()],
            },
        ];
        assert!(validate_command_list(&commands)
            .unwrap_err()
            .to_string()
            .contains("unique"));
    }
}
