//! Pure RGBA microphone icon renderer shared by tray and Windows resources.

pub fn mic_icon_rgba(size: u32) -> Vec<u8> {
    mic_icon_rgba_with_color(size, crate::tray::TrayStatus::Idle.rgba())
}

pub fn mic_icon_rgba_with_color(size: u32, color: [u8; 4]) -> Vec<u8> {
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
        color,
    );
    fill_rect(
        &mut pixels,
        size,
        (scale * 0.46) as i32,
        (scale * 0.54) as i32,
        (scale * 0.08) as i32,
        (scale * 0.22) as i32,
        color,
    );
    fill_rect(
        &mut pixels,
        size,
        (scale * 0.30) as i32,
        (scale * 0.84) as i32,
        (scale * 0.40) as i32,
        (scale * 0.08) as i32,
        color,
    );

    let center_x = (scale * 0.50) as i32;
    let center_y = (scale * 0.52) as i32;
    let radius = (scale * 0.28) as i32;
    let thickness = (scale * 0.03125).round().max(1.0) as i32;
    for degrees in 10..170 {
        let radians = (degrees as f32).to_radians();
        let x = center_x + (radius as f32 * radians.cos()) as i32;
        let y = center_y + (radius as f32 * radians.sin()) as i32;
        put_thick(&mut pixels, size, x, y, thickness, color);
    }
    pixels
}

fn fill_rect(
    pixels: &mut [u8],
    size: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    color: [u8; 4],
) {
    for pixel_y in y..(y + height) {
        for pixel_x in x..(x + width) {
            put_pixel(pixels, size, pixel_x, pixel_y, color);
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
    color: [u8; 4],
) {
    for y in (center_y - radius_y)..=(center_y + radius_y) {
        for x in (center_x - radius_x)..=(center_x + radius_x) {
            let delta_x = (x - center_x) as f32 / radius_x as f32;
            let delta_y = (y - center_y) as f32 / radius_y as f32;
            if delta_x * delta_x + delta_y * delta_y <= 1.0 {
                put_pixel(pixels, size, x, y, color);
            }
        }
    }
}

fn put_thick(
    pixels: &mut [u8],
    size: u32,
    x: i32,
    y: i32,
    radius: i32,
    color: [u8; 4],
) {
    for delta_y in -radius..=radius {
        for delta_x in -radius..=radius {
            put_pixel(pixels, size, x + delta_x, y + delta_y, color);
        }
    }
}

fn put_pixel(pixels: &mut [u8], size: u32, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 || x >= size as i32 || y >= size as i32 {
        return;
    }
    let offset = ((y as u32 * size + x as u32) * 4) as usize;
    pixels[offset..offset + 4].copy_from_slice(&color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_requested_dimensions_and_transparency() {
        let pixels = mic_icon_rgba(crate::tray::APP_ICON_SIZE);
        assert_eq!(pixels.len(), (crate::tray::APP_ICON_SIZE * crate::tray::APP_ICON_SIZE * 4) as usize);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(pixels
            .chunks_exact(4)
            .any(|pixel| pixel == crate::tray::TrayStatus::Idle.rgba()));
    }
}
