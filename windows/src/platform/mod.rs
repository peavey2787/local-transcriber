//! Windows desktop and filesystem integration.

mod dialog;
mod instance_lock;
mod paste;
mod storage;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

pub(crate) use dialog::show_error;
pub(crate) use instance_lock::acquire as acquire_instance_lock;
pub(crate) use paste::PasteTarget;
pub(crate) use storage::{app_data_dir, replace_file};

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}
