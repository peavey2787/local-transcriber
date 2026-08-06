//! Linux script adapter for shared voice-command matching and orchestration.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use transcriber_core::commands::{CommandWorker, ScriptRunner};

struct LinuxScriptRunner;

impl ScriptRunner for LinuxScriptRunner {
    fn validate_script(&self, script: &str) -> Result<()> {
        validate_script_path(script)
    }

    fn run_script(&self, script: &str) -> Result<()> {
        execute_script(script)
    }
}

pub(crate) fn create_worker(wake: crate::hotkey::UiWake) -> CommandWorker {
    let repaint = Arc::new(move || {
        if let Some(context) = wake.lock().as_ref() {
            context.request_repaint();
        }
    });
    CommandWorker::new(Arc::new(LinuxScriptRunner), repaint)
}

fn validate_script_path(script: &str) -> Result<()> {
    let path = Path::new(script.trim());
    if script.trim().is_empty() {
        bail!("Script paths cannot be empty");
    }
    if !path.is_file() {
        bail!("Script does not exist or is not a file: {}", path.display());
    }
    let extension = extension(path);
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

    let mut command = match extension(&path).as_str() {
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

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn ensure_success(path: &Path, status: ExitStatus) -> Result<()> {
    if status.success() {
        println!("[local-stt] voice-command script completed: {}", path.display());
        return Ok(());
    }
    bail!("{} exited with {}", path.display(), status)
}
