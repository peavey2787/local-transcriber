//! Canonical local-stt microphone icon used by Windows and the system tray.

pub const APP_ICON_SIZE: u32 = 64;
const ICON_COLOR: [u8; 4] = [0x1B, 0xB9, 0xCE, 0xFF];

pub fn mic_icon_rgba(size: u32) -> Vec<u8> {
    assert!(size >= 16);
    let mut pixels = vec![0_u8; (size * size * 4) as usize];
    let scale = size as f32;

    fill_ellipse(
        &mut pixels,
        size,
        (scale * 0.50) as i32,
        (scale * 0.30) as i32,
        (scale * 0.18) as i32,
        (scale * 0.26) as i32,
    );
    fill_rect(
        &mut pixels,
        size,
        (scale * 0.46) as i32,
        (scale * 0.54) as i32,
        (scale * 0.08) as i32,
        (scale * 0.22) as i32,
    );
    fill_rect(
        &mut pixels,
        size,
        (scale * 0.30) as i32,
        (scale * 0.84) as i32,
        (scale * 0.40) as i32,
        (scale * 0.08) as i32,
    );

    let center_x = (scale * 0.50) as i32;
    let center_y = (scale * 0.52) as i32;
    let radius = (scale * 0.28) as i32;
    let thickness = (scale * 0.03125).round().max(1.0) as i32;
    for degrees in 10..170 {
        let radians = (degrees as f32).to_radians();
        let x = center_x + (radius as f32 * radians.cos()) as i32;
        let y = center_y + (radius as f32 * radians.sin()) as i32;
        put_thick(&mut pixels, size, x, y, thickness);
    }
    pixels
}

fn fill_rect(pixels: &mut [u8], size: u32, x: i32, y: i32, width: i32, height: i32) {
    for pixel_y in y..(y + height) {
        for pixel_x in x..(x + width) {
            put_pixel(pixels, size, pixel_x, pixel_y);
        }
    }
}

fn fill_ellipse(
    pixels: &mut [u8],
    size: u32,
    center_x: i32,
    center_y: i32,
    radius_x: i32,
    radius_y: i32,
) {
    for y in (center_y - radius_y)..=(center_y + radius_y) {
        for x in (center_x - radius_x)..=(center_x + radius_x) {
            let delta_x = (x - center_x) as f32 / radius_x as f32;
            let delta_y = (y - center_y) as f32 / radius_y as f32;
            if delta_x * delta_x + delta_y * delta_y <= 1.0 {
                put_pixel(pixels, size, x, y);
            }
        }
    }
}

fn put_thick(pixels: &mut [u8], size: u32, x: i32, y: i32, radius: i32) {
    for delta_y in -radius..=radius {
        for delta_x in -radius..=radius {
            put_pixel(pixels, size, x + delta_x, y + delta_y);
        }
    }
}

fn put_pixel(pixels: &mut [u8], size: u32, x: i32, y: i32) {
    if x < 0 || y < 0 || x >= size as i32 || y >= size as i32 {
        return;
    }
    let offset = ((y as u32 * size + x as u32) * 4) as usize;
    pixels[offset..offset + 4].copy_from_slice(&ICON_COLOR);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_the_requested_dimensions_and_transparency() {
        let pixels = mic_icon_rgba(APP_ICON_SIZE);
        assert_eq!(pixels.len(), (APP_ICON_SIZE * APP_ICON_SIZE * 4) as usize);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(pixels.chunks_exact(4).any(|pixel| pixel == ICON_COLOR));
    }
}
