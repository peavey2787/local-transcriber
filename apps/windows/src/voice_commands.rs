//! Windows script adapter for shared voice-command matching and orchestration.

use anyhow::{bail, Context, Result};
use std::fs;
use std::ffi::{OsStr, OsString};
use std::io::ErrorKind;
use std::mem::{size_of, zeroed};
use std::os::windows::{ffi::OsStrExt, process::ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use transcriber_core::commands::{CommandWorker, ScriptOutput, ScriptRunner};
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    CreateProcessW, GetExitCodeProcess, WaitForSingleObject, CREATE_NEW_CONSOLE, INFINITE,
    PROCESS_INFORMATION, STARTUPINFOW,
};

static BATCH_WRAPPER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    let canonical = path
        .canonicalize()
        .with_context(|| format!("resolve script path {}", path.display()))?;
    Ok(shell_compatible_path(&canonical))
}

/// `std::fs::canonicalize` returns verbatim (`\\?\`) paths on Windows. Those
/// paths are valid for Win32 file APIs, but `cmd.exe` treats them as UNC paths
/// and may reject both the script and its working directory. Convert them back
/// to the normal DOS/UNC spelling before handing them to a command shell.
fn shell_compatible_path(path: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = value.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
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

struct BatchWrapper {
    path: PathBuf,
}

impl BatchWrapper {
    fn create(script: &Path) -> std::io::Result<Self> {
        let sequence = BATCH_WRAPPER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "local-stt-voice-command-{}-{sequence}.cmd",
            std::process::id()
        ));
        let script = escape_batch_quoted_value(&display_path(script));
        let contents = format!(
            "@echo off\r\n\
             setlocal\r\n\
             set \"LOCAL_STT_VOICE_COMMAND=1\"\r\n\
             set \"LOCAL_STT_SCRIPT_PATH={script}\"\r\n\
             call \"{script}\"\r\n\
             set \"_LOCAL_STT_EXIT=%ERRORLEVEL%\"\r\n\
             if not \"%_LOCAL_STT_EXIT%\"==\"0\" (\r\n\
               echo.\r\n\
               echo Voice command script failed with exit code %_LOCAL_STT_EXIT%.\r\n\
               echo Script: \"{script}\"\r\n\
               echo.\r\n\
               echo Press any key to close this window...\r\n\
               pause >nul\r\n\
             )\r\n\
             exit /b %_LOCAL_STT_EXIT%\r\n"
        );
        fs::write(&path, contents)?;
        Ok(Self { path })
    }
}

impl Drop for BatchWrapper {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn escape_batch_quoted_value(value: &str) -> String {
    value.replace('%', "%%")
}

fn run_batch_in_console(path: &Path, working_directory: &Path) -> std::io::Result<ExitStatus> {
    // Use a small wrapper so a failed batch script cannot flash closed before
    // the user can read the actual error. Launch it through CreateProcessW
    // without STARTF_USESTDHANDLES: the release executable is a GUI process,
    // so inheriting its absent standard handles leaves `pause` without a real
    // console input buffer even when CREATE_NEW_CONSOLE is requested.
    let wrapper = BatchWrapper::create(path)?;
    let result = run_command_prompt(&wrapper.path, working_directory);
    drop(wrapper);
    result
}

fn run_command_prompt(wrapper: &Path, working_directory: &Path) -> std::io::Result<ExitStatus> {
    let command_processor = std::env::var_os("ComSpec")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| OsString::from("cmd.exe"));
    let command_line = format!(
        "\"{}\" /D /Q /C call \"{}\"",
        command_processor.to_string_lossy(),
        wrapper.to_string_lossy()
    );
    let application = wide_null(&command_processor);
    let mut command_line = wide_null(OsStr::new(&command_line));
    let current_directory = wide_null(working_directory.as_os_str());

    // SAFETY: All pointers refer to live, NUL-terminated buffers for the
    // duration of CreateProcessW. STARTUPINFOW intentionally leaves dwFlags
    // clear so Windows assigns stdin/stdout/stderr to the new console. Process
    // and thread handles returned on success are closed on every path.
    unsafe {
        let mut startup: STARTUPINFOW = zeroed();
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        let mut process: PROCESS_INFORMATION = zeroed();
        let created = CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_NEW_CONSOLE,
            std::ptr::null(),
            current_directory.as_ptr(),
            &startup,
            &mut process,
        );
        if created == 0 {
            return Err(std::io::Error::last_os_error());
        }

        CloseHandle(process.hThread);
        let wait_result = WaitForSingleObject(process.hProcess, INFINITE);
        if wait_result != 0 {
            CloseHandle(process.hProcess);
            return Err(std::io::Error::last_os_error());
        }

        let mut exit_code = 0;
        if GetExitCodeProcess(process.hProcess, &mut exit_code) == 0 {
            let error = std::io::Error::last_os_error();
            CloseHandle(process.hProcess);
            return Err(error);
        }
        CloseHandle(process.hProcess);
        Ok(ExitStatus::from_raw(exit_code))
    }
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
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
    shell_compatible_path(path).to_string_lossy().into_owned()
}

fn ensure_success(path: &Path, status: ExitStatus) -> Result<()> {
    let path = display_path(path);
    if status.success() {
        println!("[local-stt] voice-command script completed: {path}");
        return Ok(());
    }
    bail!("{path} exited with {status}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_verbatim_drive_prefix_for_cmd() {
        let converted = shell_compatible_path(Path::new(r"\\?\C:\Users\name\hello.cmd"));
        assert_eq!(converted, PathBuf::from(r"C:\Users\name\hello.cmd"));
    }

    #[test]
    fn converts_verbatim_unc_prefix_for_cmd() {
        let converted = shell_compatible_path(Path::new(r"\\?\UNC\server\share\hello.cmd"));
        assert_eq!(converted, PathBuf::from(r"\\server\share\hello.cmd"));
    }

    #[test]
    fn escapes_percent_expansion_in_wrapper_values() {
        assert_eq!(escape_batch_quoted_value(r"C:\100%\hello.cmd"), r"C:\100%%\hello.cmd");
    }
}
