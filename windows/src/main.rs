#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! local-stt-rs — Windows tray speech-to-text powered by Parakeet TDT.

#[cfg(not(target_os = "windows"))]
compile_error!("The windows project supports only Windows.");

mod app;
mod asr;
mod audio;
mod config;
mod hotkey;
mod icon;
mod model;
mod overlay;
mod platform;
mod sha256;
mod tray;
mod ui_wake;
mod util;

use anyhow::Result;
use eframe::egui;

use crate::app::{LocalSttApp, CONTROL_VIEWPORT_POSITION, CONTROL_VIEWPORT_SIZE};

fn main() {
    if let Err(error) = run() {
        let message = format!("{error:#}");
        eprintln!("[local-stt] {message}");
        platform::show_error("local-stt could not start", &message);
    }
}

fn run() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let _instance_lock = platform::acquire_instance_lock()?;

    let mut cfg = config::load();
    if !cfg.hotkey.trim().is_empty() {
        if let Err(error) = hotkey::validate(&cfg.hotkey) {
            eprintln!(
                "[local-stt] configured hotkey is invalid ({error}); disabling it until Settings is updated"
            );
            cfg.hotkey.clear();
        }
    }
    config::save(&cfg)?;

    println!(
        "[local-stt] running on {} — hotkey={} — auto_paste={}",
        std::env::consts::OS,
        hotkey::friendly_name(&cfg.hotkey),
        cfg.auto_paste
    );

    // Keep one native root window alive for the entire process lifetime.
    // The same window presents idle control, notifications, and Settings, so
    // opening or closing Settings never creates or destroys an event loop.
    let viewport = egui::ViewportBuilder::default()
        .with_title("local-stt")
        .with_icon(egui::IconData {
            rgba: icon::mic_icon_rgba(icon::APP_ICON_SIZE),
            width: icon::APP_ICON_SIZE,
            height: icon::APP_ICON_SIZE,
        })
        .with_inner_size(CONTROL_VIEWPORT_SIZE)
        .with_position(CONTROL_VIEWPORT_POSITION)
        .with_decorations(false)
        .with_transparent(true)
        .with_mouse_passthrough(true)
        .with_always_on_top()
        .with_taskbar(false)
        .with_resizable(false)
        .with_visible(true)
        .with_active(false);

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "local-stt",
        native_options,
        Box::new(move |cc| match LocalSttApp::new(cc, cfg.clone()) {
            Ok(app) => Ok(Box::new(app) as Box<dyn eframe::App>),
            Err(error) => {
                eprintln!("[local-stt] failed to start: {error:#}");
                Err(error.into())
            }
        }),
    )
    .map_err(|error| anyhow::anyhow!("eframe error: {error}"))?;

    Ok(())
}
