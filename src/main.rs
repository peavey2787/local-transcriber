//! local-stt-rs — Linux-first tray speech-to-text powered by Parakeet TDT.

mod app;
mod asr;
mod audio;
mod config;
mod hotkey;
mod instance_lock;
mod model;
mod overlay;
mod paste;
mod sha256;
mod tray;
mod util;

use anyhow::{Context, Result};
use eframe::egui;

use crate::app::LocalSttApp;
use crate::overlay::{CARD_H, CARD_W};

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let _lock = match instance_lock::acquire() {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("[local-stt] {error}");
            std::process::exit(1);
        }
    };

    #[cfg(target_os = "linux")]
    let _legacy_backend_warning_filter = tray::install_legacy_backend_warning_filter();

    #[cfg(target_os = "linux")]
    {
        gtk::init().context("initialize GTK for the Linux tray icon")?;
        if std::env::var_os("GLOBAL_HOTKEY_APP_ID").is_none() {
            std::env::set_var("GLOBAL_HOTKEY_APP_ID", "io.local-stt.parakeet");
        }
    }

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

    // The single native window becomes either a non-activating status overlay
    // or the settings panel. When idle it is parked off-screen while the tray
    // and global shortcut remain active.
    let viewport = egui::ViewportBuilder::default()
        .with_title("local-stt")
        .with_inner_size([CARD_W, CARD_H])
        .with_position([-32000.0, -32000.0])
        .with_decorations(false)
        .with_transparent(true)
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
