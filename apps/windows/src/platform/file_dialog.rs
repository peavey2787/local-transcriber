//! Native Windows file chooser for voice-command scripts.

use std::ffi::OsStr;
use std::mem::size_of;

use windows_sys::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, OPENFILENAMEW, OFN_EXPLORER,
    OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use super::wide_null;

const FILE_BUFFER_CHARS: usize = 32_768;

pub(crate) fn select_voice_command_script() -> Result<Option<String>, String> {
    let mut file_buffer = vec![0_u16; FILE_BUFFER_CHARS];
    let filter = filter_string();
    let title = wide_null(OsStr::new("Select a voice-command script"));

    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        // SAFETY: The root application window is foreground while the Browse
        // button is processed. A null result is also accepted by the API.
        hwndOwner: unsafe { GetForegroundWindow() },
        lpstrFilter: filter.as_ptr(),
        nFilterIndex: 1,
        lpstrFile: file_buffer.as_mut_ptr(),
        nMaxFile: file_buffer.len() as u32,
        lpstrTitle: title.as_ptr(),
        Flags: OFN_EXPLORER
            | OFN_FILEMUSTEXIST
            | OFN_HIDEREADONLY
            | OFN_NOCHANGEDIR
            | OFN_PATHMUSTEXIST,
        ..Default::default()
    };

    // SAFETY: OPENFILENAMEW points to live, writable UTF-16 buffers for the
    // duration of this synchronous call. The API writes at most nMaxFile code
    // units into file_buffer.
    if unsafe { GetOpenFileNameW(&mut dialog) } != 0 {
        let length = file_buffer
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(file_buffer.len());
        return String::from_utf16(&file_buffer[..length])
            .map(Some)
            .map_err(|_| "Windows returned an invalid UTF-16 script path.".to_string());
    }

    // A zero extended error means the user cancelled the dialog.
    let error = unsafe { CommDlgExtendedError() };
    if error == 0 {
        Ok(None)
    } else {
        Err(format!(
            "Windows could not open the script file dialog (common-dialog error 0x{error:04X})."
        ))
    }
}

fn filter_string() -> Vec<u16> {
    "Supported scripts (*.ps1;*.bat;*.cmd;*.py)\0*.ps1;*.bat;*.cmd;*.py\0All files (*.*)\0*.*\0\0"
        .encode_utf16()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::filter_string;

    #[test]
    fn native_filter_is_double_nul_terminated() {
        let filter = filter_string();
        assert!(filter.ends_with(&[0, 0]));
    }
}
