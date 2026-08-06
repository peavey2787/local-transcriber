//! Primary-display geometry used to place the persistent root window.
//!
//! The root window is parked off-screen while idle, so egui cannot reliably
//! report a monitor. Windows system metrics provide deterministic placement
//! instead of assuming a fixed 1920x1080 display.

use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
};

const DEFAULT_WINDOWS_DPI: u32 = 96;

pub(crate) fn primary_display_size_points() -> [f32; 2] {
    // SAFETY: GetDpiForSystem reads process-wide display configuration and
    // takes no pointers. A zero result is treated as the Windows default DPI.
    let dpi = unsafe { GetDpiForSystem() };

    // SAFETY: GetSystemMetrics reads process-wide display metrics and takes no
    // pointers. SM_CXSCREEN and SM_CYSCREEN query the primary display size.
    let width_pixels = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    // SAFETY: Same contract as the call above.
    let height_pixels = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    [
        pixels_to_points(width_pixels, dpi),
        pixels_to_points(height_pixels, dpi),
    ]
}

fn pixels_to_points(pixels: i32, dpi: u32) -> f32 {
    let effective_dpi = dpi.max(DEFAULT_WINDOWS_DPI) as f32;
    pixels.max(1) as f32 * DEFAULT_WINDOWS_DPI as f32 / effective_dpi
}

#[cfg(test)]
mod tests {
    use super::pixels_to_points;

    #[test]
    fn display_pixels_are_converted_to_egui_points() {
        assert_eq!(pixels_to_points(1920, 96), 1920.0);
        assert_eq!(pixels_to_points(1920, 144), 1280.0);
    }
}
