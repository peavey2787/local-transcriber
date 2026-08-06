//! Safe phrase matching and sequential local script execution for voice commands.

use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use crate::config::VoiceCommand;

/// Produces a stable phrase key for speech-recognition output and configured
/// aliases. Letter and number runs are preserved, case is folded, and all
/// whitespace, punctuation, control, and invisible formatting characters are
/// treated as word boundaries.
pub(crate) fn normalize_phrase(value: &str) -> String {
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

pub(crate) fn matching_command(commands: &[VoiceCommand], spoken: &str) -> Option<VoiceCommand> {
    let normalized = normalize_phrase(spoken);
    if normalized.is_empty() {
        return None;
    }
    commands
        .iter()
        .find(|command| normalized_aliases(&command.phrase).any(|alias| alias == normalized))
        .cloned()
}

pub(crate) fn validate_command_list(commands: &[VoiceCommand]) -> Result<()> {
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
            validate_script_path(script)?;
        }
    }
    Ok(())
}

pub(crate) fn execute_command(command: &VoiceCommand) -> Result<()> {
    for (index, script) in command.scripts.iter().enumerate() {
        println!(
            "[local-stt] starting voice-command script {}/{}",
            index + 1,
            command.scripts.len()
        );
        execute_script(script).with_context(|| format!("run script {}", script))?;
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
        bail!(
            "Linux voice commands support .sh, .bash, and .py files: {}",
            path.display()
        );
    }
    Ok(())
}

fn canonical_script_path(script: &str) -> Result<PathBuf> {
    let path = Path::new(script.trim());
    path.canonicalize()
        .with_context(|| format!("resolve script path {}", path.display()))
}

fn execute_script(script: &str) -> Result<()> {
    validate_script_path(script)?;
    let path = canonical_script_path(script)?;
    let working_directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let mut command = match extension.as_str() {
        "sh" | "bash" => Command::new("bash"),
        "py" => Command::new("python3"),
        _ => unreachable!("validated extension"),
    };
    let status = command
        .arg(&path)
        .current_dir(working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("start {}", path.display()))?;
    ensure_success(&path, status)
}

fn ensure_success(path: &Path, status: ExitStatus) -> Result<()> {
    if status.success() {
        println!("[local-stt] voice-command script completed: {}", path.display());
        return Ok(());
    }
    bail!("{} exited with {}", path.display(), status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn matching_ignores_case_whitespace_and_surrounding_punctuation() {
        let commands = vec![VoiceCommand {
            phrase: "Open Reports".into(),
            scripts: vec!["/tmp/report.sh".into()],
        }];
        assert_eq!(
            matching_command(&commands, "  OPEN   reports. ")
                .unwrap()
                .phrase,
            "Open Reports"
        );
        assert!(matching_command(&commands, "open report").is_none());
    }

    #[test]
    fn comma_separated_aliases_match_case_and_punctuation_variants() {
        let commands = vec![VoiceCommand {
            phrase: "Hello, hello there, start report".into(),
            scripts: vec!["/tmp/report.sh".into()],
        }];

        assert!(matching_command(&commands, "HELLO!").is_some());
        assert!(matching_command(&commands, "Hello there?").is_some());
        assert!(matching_command(&commands, "start report.").is_some());
        assert!(matching_command(&commands, "hello report").is_none());
    }

    #[test]
    fn unicode_and_invisible_punctuation_are_normalized() {
        let commands = vec![VoiceCommand {
            phrase: "Open reports".into(),
            scripts: vec!["/tmp/report.sh".into()],
        }];

        assert!(matching_command(&commands, "“OPEN\u{200b} reports…”").is_some());
        assert_eq!(normalize_phrase("hel\u{200b}lo"), "hello");
        assert_eq!(normalize_phrase("hello—there"), "hello there");
    }

    #[test]
    fn punctuation_only_alias_duplicates_inside_one_command_are_harmless() {
        let aliases = normalized_aliases("Hello,Hello.,Hello?,Hello!")
            .collect::<HashSet<_>>();
        assert_eq!(aliases, HashSet::from(["hello".to_string()]));
    }

    #[test]
    fn empty_command_list_is_rejected() {
        assert!(validate_command_list(&[])
            .unwrap_err()
            .to_string()
            .contains("at least one command"));
    }

    #[test]
    fn duplicate_normalized_phrases_across_commands_are_rejected_before_paths() {
        let commands = vec![
            VoiceCommand {
                phrase: "Build App, compile".into(),
                scripts: vec!["missing.sh".into()],
            },
            VoiceCommand {
                phrase: " compile. ".into(),
                scripts: vec!["other.sh".into()],
            },
        ];
        assert!(validate_command_list(&commands)
            .unwrap_err()
            .to_string()
            .contains("overlap"));
    }

    #[test]
    fn shell_script_runs_from_its_own_directory() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "local-stt-voice-command-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("payload.txt"), "ran").unwrap();
        let script = directory.join("command.sh");
        fs::write(&script, "#!/usr/bin/env bash\ncat payload.txt > result.txt\n").unwrap();

        execute_script(script.to_str().unwrap()).unwrap();

        assert_eq!(fs::read_to_string(directory.join("result.txt")).unwrap(), "ran");
        fs::remove_dir_all(directory).unwrap();
    }
}
