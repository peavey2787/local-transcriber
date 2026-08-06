//! Windows script adapter for shared voice-command matching and orchestration.

use anyhow::{bail, Context, Result};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use transcriber_core::commands::{CommandWorker, ScriptOutput, ScriptRunner};

struct WindowsScriptRunner;

impl ScriptRunner for WindowsScriptRunner {
    fn validate_script(&self, script: &str) -> Result<()> {
        validate_script_path(script)
    }

    fn run_script(&self, script: &str) -> Result<ScriptOutput> {
        execute_script(script)?;
        Ok(ScriptOutput::default())
    }
}

pub(crate) fn create_worker(wake: crate::ui_wake::UiWake) -> CommandWorker {
    let repaint = Arc::new(move || wake.request_repaint());
    CommandWorker::new(Arc::new(WindowsScriptRunner), repaint)
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
    if !matches!(extension.as_str(), "ps1" | "bat" | "cmd" | "py") {
        bail!(
            "Windows voice commands support .ps1, .bat, .cmd, and .py files: {}",
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

    let status = match extension(&path).as_str() {
        "ps1" => Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&path)
            .current_dir(working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status(),
        "bat" | "cmd" => Command::new("cmd.exe")
            .arg("/C")
            .arg(&path)
            .current_dir(working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status(),
        "py" => run_python(&path, working_directory),
        _ => unreachable!("validated extension"),
    }
    .with_context(|| format!("start {}", path.display()))?;
    ensure_success(&path, status)
}

fn run_python(path: &Path, working_directory: &Path) -> std::io::Result<ExitStatus> {
    let configure = |command: &mut Command| {
        command
            .arg(path)
            .current_dir(working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    };

    let mut launcher = Command::new("py.exe");
    launcher.arg("-3");
    configure(&mut launcher);
    match launcher.status() {
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let mut fallback = Command::new("python.exe");
            configure(&mut fallback);
            fallback.status()
        }
        result => result,
    }
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
