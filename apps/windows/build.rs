#[path = "../../crates/transcriber-ui/src/tray_icon.rs"]
mod tray_icon;

mod tray {
    pub const APP_ICON_SIZE: u32 = 64;

    #[derive(Clone, Copy)]
    pub enum TrayStatus {
        Idle,
    }

    impl TrayStatus {
        pub fn rgba(self) -> [u8; 4] {
            [0x1B, 0xB9, 0xCE, 0xFF]
        }
    }
}

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ICON_SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, tray::APP_ICON_SIZE, 128, 256];

fn main() {
    println!("cargo:rerun-if-changed=../../crates/transcriber-ui/src/tray_icon.rs");
    println!("cargo:rerun-if-env-changed=RC");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let output_directory = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR"));
    let icon_path = output_directory.join("local-stt.ico");
    let resource_script_path = output_directory.join("local-stt.rc");
    let resource_output_path = output_directory.join("local-stt.res");

    write_icon_file(&icon_path).expect("generate the local-stt Windows icon");
    write_resource_script(&resource_script_path, &icon_path)
        .expect("generate the local-stt Windows resource script");

    let resource_compiler = find_resource_compiler().unwrap_or_else(|| {
        panic!(
            "Windows SDK resource compiler rc.exe was not found. Install the Windows 10/11 SDK or set the RC environment variable to rc.exe."
        )
    });

    let status = Command::new(&resource_compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource_output_path)
        .arg(&resource_script_path)
        .status()
        .unwrap_or_else(|error| {
            panic!(
                "failed to launch Windows resource compiler {}: {error}",
                resource_compiler.display()
            )
        });

    if !status.success() {
        panic!(
            "Windows resource compiler {} exited with {status}",
            resource_compiler.display()
        );
    }

    println!(
        "cargo:rustc-link-arg-bin=local-stt-rs={}",
        resource_output_path.display()
    );
}

fn find_resource_compiler() -> Option<PathBuf> {
    if let Some(explicit) = env::var_os("RC") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Some(path);
        }
    }

    if let Some(path) = find_on_path("rc.exe") {
        return Some(path);
    }

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "x86_64".to_owned());
    let sdk_arch = match target_arch.as_str() {
        "x86_64" => "x64",
        "x86" => "x86",
        "aarch64" => "arm64",
        _ => "x64",
    };

    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        let Some(program_files) = env::var_os(variable) else {
            continue;
        };
        let bin_root = PathBuf::from(program_files)
            .join("Windows Kits")
            .join("10")
            .join("bin");
        let Ok(entries) = fs::read_dir(&bin_root) else {
            continue;
        };

        let mut versions = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        versions.sort_by(|left, right| right.file_name().cmp(&left.file_name()));

        for version_directory in versions {
            let candidate = version_directory.join(sdk_arch).join("rc.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn find_on_path(executable: &str) -> Option<PathBuf> {
    let output = Command::new("where.exe").arg(executable).output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

fn write_resource_script(path: &Path, icon_path: &Path) -> std::io::Result<()> {
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".to_owned());
    let numeric_version = numeric_version(&version);
    let icon = resource_string(icon_path.as_os_str());

    let script = format!(
        r#"1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {numeric_version}
PRODUCTVERSION {numeric_version}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "FileDescription", "Local speech-to-text powered by NVIDIA Parakeet TDT\0"
            VALUE "FileVersion", "{version}\0"
            VALUE "OriginalFilename", "local-stt.exe\0"
            VALUE "ProductName", "local-stt\0"
            VALUE "ProductVersion", "{version}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
    );

    fs::write(path, script)
}

fn numeric_version(version: &str) -> String {
    let mut components = version
        .split('.')
        .map(|component| {
            component
                .chars()
                .take_while(|character| character.is_ascii_digit())
                .collect::<String>()
                .parse::<u16>()
                .unwrap_or(0)
        })
        .take(4)
        .collect::<Vec<_>>();
    components.resize(4, 0);
    components
        .into_iter()
        .map(|component| component.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn resource_string(value: &OsStr) -> String {
    value
        .to_string_lossy()
        .replace('\\', "/")
        .replace('"', "\"\"")
}

fn write_icon_file(path: &Path) -> std::io::Result<()> {
    let images = ICON_SIZES
        .into_iter()
        .map(|size| encode_bitmap_icon(&tray_icon::mic_icon_rgba(size), size))
        .collect::<Vec<_>>();

    let mut file = Vec::new();
    push_u16(&mut file, 0);
    push_u16(&mut file, 1);
    push_u16(&mut file, images.len() as u16);

    let mut offset = 6 + images.len() * 16;
    for (&size, image) in ICON_SIZES.iter().zip(&images) {
        file.push(if size == 256 { 0 } else { size as u8 });
        file.push(if size == 256 { 0 } else { size as u8 });
        file.push(0);
        file.push(0);
        push_u16(&mut file, 1);
        push_u16(&mut file, 32);
        push_u32(&mut file, image.len() as u32);
        push_u32(&mut file, offset as u32);
        offset += image.len();
    }

    for image in images {
        file.extend_from_slice(&image);
    }
    fs::write(path, file)
}

fn encode_bitmap_icon(rgba: &[u8], size: u32) -> Vec<u8> {
    assert_eq!(rgba.len(), (size * size * 4) as usize);

    let mask_stride = size.div_ceil(32) * 4;
    let mut image = Vec::with_capacity((40 + size * size * 4 + mask_stride * size) as usize);

    push_u32(&mut image, 40);
    push_i32(&mut image, size as i32);
    push_i32(&mut image, (size * 2) as i32);
    push_u16(&mut image, 1);
    push_u16(&mut image, 32);
    push_u32(&mut image, 0);
    push_u32(&mut image, 0);
    push_i32(&mut image, 0);
    push_i32(&mut image, 0);
    push_u32(&mut image, 0);
    push_u32(&mut image, 0);

    for y in (0..size).rev() {
        for x in 0..size {
            let pixel = ((y * size + x) * 4) as usize;
            image.extend_from_slice(&[
                rgba[pixel + 2],
                rgba[pixel + 1],
                rgba[pixel],
                rgba[pixel + 3],
            ]);
        }
    }

    for y in (0..size).rev() {
        let mut mask_row = vec![0_u8; mask_stride as usize];
        for x in 0..size {
            let alpha = rgba[((y * size + x) * 4 + 3) as usize];
            if alpha == 0 {
                mask_row[(x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        image.extend_from_slice(&mask_row);
    }
    image
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}
