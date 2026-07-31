#[path = "src/icon.rs"]
mod icon;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const ICON_SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, icon::APP_ICON_SIZE, 128, 256];

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let output_directory = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR"));
    let icon_path = output_directory.join("local-stt.ico");
    write_icon_file(&icon_path).expect("generate the local-stt Windows icon");

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(
        icon_path
            .to_str()
            .expect("the Cargo output path is valid UTF-8"),
    );
    resource
        .compile()
        .expect("compile the local-stt Windows resources");
}

fn write_icon_file(path: &Path) -> std::io::Result<()> {
    let images = ICON_SIZES
        .into_iter()
        .map(|size| encode_bitmap_icon(&icon::mic_icon_rgba(size), size))
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
