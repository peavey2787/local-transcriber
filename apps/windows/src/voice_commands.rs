//! Windows script adapter for shared voice-command matching and orchestration.

use anyhow::{bail, Context, Result};
use std::io::ErrorKind;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use transcriber_core::commands::{CommandWorker, ScriptOutput, ScriptRunner};
use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;

struct WindowsScriptRunner;

impl ScriptRunner for WindowsScriptRunner {
    fn validate_script(&self, script: &str) -> Result<()> {
        validate_script_path(script)
    }

    fn run_script(&self, script: &str) -> Result<ScriptOutput> {
        execute_script(script)
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

fn execute_script(script: &str) -> Result<ScriptOutput> {
    validate_script_path(script)?;
    let path = canonical_script_path(script)?;
    let working_directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let (status, output) = match extension(&path).as_str() {
        "ps1" => (
            Command::new("powershell.exe")
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(&path)
                .current_dir(working_directory)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status(),
            ScriptOutput::default(),
        ),
        "bat" | "cmd" => (
            run_batch_in_console(&path, working_directory),
            ScriptOutput {
                stdout: "Batch script completed in Command Prompt.".to_string(),
                stderr: String::new(),
            },
        ),
        "py" => (run_python(&path, working_directory), ScriptOutput::default()),
        _ => unreachable!("validated extension"),
    };

    let status = status.with_context(|| format!("start {}", display_path(&path)))?;
    ensure_success(&path, status)?;
    Ok(output)
}

fn run_batch_in_console(path: &Path, working_directory: &Path) -> std::io::Result<ExitStatus> {
    // Batch files commonly contain interactive commands such as `pause`. The
    // application is a GUI process and has no console stdin, so launching them
    // with redirected/null input makes those commands fail with exit code 1.
    // Give cmd.exe its own visible console and wait for it to finish so script
    // chains still execute in order.
    Command::new("cmd.exe")
        .args(["/D", "/Q", "/C"])
        .arg(path)
        .current_dir(working_directory)
        .creation_flags(CREATE_NEW_CONSOLE)
        .env("LOCAL_STT_VOICE_COMMAND", "1")
        .env("LOCAL_STT_SCRIPT_PATH", path.as_os_str())
        .status()
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

fn display_path(path: &Path) -> String {
    let display = path.to_string_lossy();
    display
        .strip_prefix(r"\\?\")
        .unwrap_or(display.as_ref())
        .to_string()
}

fn ensure_success(path: &Path, status: ExitStatus) -> Result<()> {
    let path = display_path(path);
    if status.success() {
        println!("[local-stt] voice-command script completed: {path}");
        return Ok(());
    }
    bail!("{path} exited with {status}")
}
