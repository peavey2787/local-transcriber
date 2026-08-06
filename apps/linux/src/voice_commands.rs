//! Linux script adapter for shared voice-command matching and orchestration.

use anyhow::{bail, Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use transcriber_core::commands::{CommandWorker, ScriptOutput, ScriptRunner};

const MAX_CAPTURE_BYTES: u64 = 16 * 1024;
const MAX_DIAGNOSTIC_BYTES: u64 = 1024 * 1024;
static CAPTURE_COUNTER: AtomicU64 = AtomicU64::new(1);
static DIAGNOSTIC_LOCK: Mutex<()> = Mutex::new(());

struct LinuxScriptRunner;

impl ScriptRunner for LinuxScriptRunner {
    fn validate_script(&self, script: &str) -> Result<()> {
        validate_script_path(script)
    }

    fn run_script(&self, script: &str) -> Result<ScriptOutput> {
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

fn execute_script(script: &str) -> Result<ScriptOutput> {
    validate_script_path(script)?;
    let path = canonical_script_path(script)?;
    let working_directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let capture = OutputCapture::create()?;
    let mut command = match extension(&path).as_str() {
        "sh" | "bash" => Command::new("bash"),
        "py" => Command::new("python3"),
        _ => unreachable!("validated extension"),
    };
    let child = command
        .arg(&path)
        .current_dir(working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::from(capture.stdout.try_clone()?))
        .stderr(Stdio::from(capture.stderr.try_clone()?))
        .env("LOCAL_STT_VOICE_COMMAND", "1")
        .env("LOCAL_STT_SCRIPT_PATH", path.as_os_str())
        .spawn()
        .with_context(|| format!("start {}", path.display()));

    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            let _ = capture.read_and_remove();
            append_diagnostic(&format!("FAILED to start {}: {error:#}", path.display()));
            return Err(error);
        }
    };
    let pid = child.id();
    append_diagnostic(&format!(
        "START pid={pid} script={} session={}",
        path.display(),
        desktop_session_summary()
    ));
    println!(
        "[local-stt] voice-command process started: pid={pid} {}",
        path.display()
    );

    let status = child
        .wait()
        .with_context(|| format!("wait for {} (pid {pid})", path.display()));
    let output = capture.read_and_remove();
    let status = status?;
    let mut output = output?;
    append_wayland_xdotool_warning(&path, &mut output);
    if let Err(error) = ensure_success(&path, &status, &output) {
        append_diagnostic(&format!(
            "FAILED pid={pid} script={} status={}",
            path.display(),
            status
        ));
        return Err(error);
    }
    append_diagnostic(&format!(
        "DONE pid={pid} script={} status={} stdout_bytes={} stderr_bytes={}",
        path.display(),
        status,
        output.stdout.len(),
        output.stderr.len()
    ));

    if !output.stdout.trim().is_empty() {
        println!("[local-stt] script stdout:\n{}", output.stdout.trim_end());
    }
    if !output.stderr.trim().is_empty() {
        eprintln!("[local-stt] script stderr:\n{}", output.stderr.trim_end());
    }
    println!(
        "[local-stt] voice-command script completed: {}",
        path.display()
    );
    Ok(output)
}

pub(crate) fn append_diagnostic(message: &str) {
    if cfg!(test) {
        return;
    }
    if let Err(error) = append_diagnostic_inner(message) {
        eprintln!("[local-stt] could not write voice-command diagnostics: {error:#}");
    }
}

pub(crate) fn diagnostic_path() -> PathBuf {
    crate::config::config_dir().join("voice-command.log")
}

fn append_diagnostic_inner(message: &str) -> Result<()> {
    let _guard = DIAGNOSTIC_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let directory = crate::config::config_dir();
    crate::config::prepare_private_dir(&directory)?;
    let path = diagnostic_path();
    let truncate = fs::metadata(&path)
        .map(|metadata| metadata.len() >= MAX_DIAGNOSTIC_BYTES)
        .unwrap_or(false);
    let mut options = OpenOptions::new();
    options.create(true).write(true);
    if truncate {
        options.truncate(true);
    } else {
        options.append(true);
    }
    let mut file = options
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("protect {}", path.display()))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    writeln!(file, "{timestamp} {message}")
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn desktop_session_summary() -> String {
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into());
    let display = std::env::var("DISPLAY").unwrap_or_else(|_| "unset".into());
    let wayland = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "unset".into());
    format!("type={session} DISPLAY={display} WAYLAND_DISPLAY={wayland}")
}

fn append_wayland_xdotool_warning(path: &Path, output: &mut ScriptOutput) {
    if !is_wayland_session() || !script_mentions_xdotool(path) {
        return;
    }
    let warning = "local-stt warning: this is a Wayland session and the script invokes xdotool. The script was launched, but xdotool cannot reliably control native Wayland windows or the Wayland pointer; use a compositor-supported tool or ydotool with appropriate permissions.";
    if !output.stderr.trim().is_empty() {
        output.stderr.push('\n');
    }
    output.stderr.push_str(warning);
    append_diagnostic(warning);
}

fn is_wayland_session() -> bool {
    std::env::var("XDG_SESSION_TYPE")
        .map(|value| value.eq_ignore_ascii_case("wayland"))
        .unwrap_or_else(|_| std::env::var_os("WAYLAND_DISPLAY").is_some())
}

fn script_mentions_xdotool(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut bytes = Vec::new();
    if Read::by_ref(&mut file)
        .take(256 * 1024)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return false;
    }
    String::from_utf8_lossy(&bytes).contains("xdotool")
}

struct OutputCapture {
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    stdout: File,
    stderr: File,
}

impl OutputCapture {
    fn create() -> Result<Self> {
        let base = std::env::temp_dir();
        for _ in 0..32 {
            let nonce = CAPTURE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let stem = format!("local-stt-command-{}-{nonce}", std::process::id());
            let stdout_path = base.join(format!("{stem}.stdout"));
            let stderr_path = base.join(format!("{stem}.stderr"));

            let stdout = match create_capture_file(&stdout_path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error).context("create voice-command stdout capture"),
            };
            let stderr = match create_capture_file(&stderr_path) {
                Ok(file) => file,
                Err(error) => {
                    let _ = fs::remove_file(&stdout_path);
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        continue;
                    }
                    return Err(error).context("create voice-command stderr capture");
                }
            };
            return Ok(Self {
                stdout_path,
                stderr_path,
                stdout,
                stderr,
            });
        }
        bail!("could not allocate temporary voice-command output files")
    }

    fn read_and_remove(self) -> Result<ScriptOutput> {
        let Self {
            stdout_path,
            stderr_path,
            stdout,
            stderr,
        } = self;
        drop(stdout);
        drop(stderr);
        let stdout = read_capture(&stdout_path);
        let stderr = read_capture(&stderr_path);
        let _ = fs::remove_file(&stdout_path);
        let _ = fs::remove_file(&stderr_path);
        Ok(ScriptOutput {
            stdout: stdout?,
            stderr: stderr?,
        })
    }
}

fn create_capture_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
}

fn read_capture(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("read {}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_CAPTURE_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("read {}", path.display()))?;
    let truncated = bytes.len() as u64 > MAX_CAPTURE_BYTES;
    if truncated {
        bytes.truncate(MAX_CAPTURE_BYTES as usize);
    }
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        text.push_str("\n[output truncated]\n");
    }
    Ok(text)
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn ensure_success(path: &Path, status: &ExitStatus, output: &ScriptOutput) -> Result<()> {
    if status.success() {
        return Ok(());
    }
    let detail = output.display_text();
    if detail.is_empty() {
        bail!("{} exited with {}", path.display(), status);
    }
    bail!("{} exited with {}: {}", path.display(), status, detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn shell_script_output_is_captured_from_its_own_directory() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "local-stt-voice-command-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("payload.txt"), "Hello World").unwrap();
        let script = directory.join("command.sh");
        fs::write(&script, "#!/usr/bin/env bash\ncat payload.txt\n").unwrap();

        let output = execute_script(script.to_str().unwrap()).unwrap();

        assert_eq!(output.stdout.trim(), "Hello World");
        assert!(output.stderr.is_empty());
        fs::remove_dir_all(directory).unwrap();
    }
}
