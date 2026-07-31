//! Background Windows recording-device discovery for the Settings window.
//!
//! CPAL ultimately calls Windows audio APIs that can block while devices are
//! being added, removed, or reconfigured. Device enumeration therefore never
//! runs on eframe's sole GUI/event-loop thread.

use anyhow::Context;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TrySendError};
use std::thread;

use crate::audio::{input_device_options, InputDeviceOption};
use crate::ui_wake::UiWake;

pub(super) struct DeviceDiscoveryResult {
    pub(super) announce_success: bool,
    pub(super) options: Result<Vec<InputDeviceOption>, String>,
}

struct DeviceDiscoveryRequest {
    announce_success: bool,
}

pub(in crate::app) struct SettingsDeviceDiscovery {
    requests: Sender<DeviceDiscoveryRequest>,
    results: Receiver<DeviceDiscoveryResult>,
    _thread: thread::JoinHandle<()>,
}

impl SettingsDeviceDiscovery {
    pub(in crate::app) fn spawn(ui_wake: UiWake) -> anyhow::Result<Self> {
        // Only one scan may be pending. SettingsState also prevents duplicate
        // requests, while this bound protects the worker boundary itself.
        let (requests, request_rx) = bounded::<DeviceDiscoveryRequest>(1);
        let (result_tx, results) = unbounded();
        let worker = thread::Builder::new()
            .name("local-stt-device-discovery".into())
            .spawn(move || run_worker(request_rx, result_tx, ui_wake))
            .context("start recording-device discovery worker")?;

        Ok(Self {
            requests,
            results,
            _thread: worker,
        })
    }

    pub(super) fn request(&self, announce_success: bool) -> Result<(), String> {
        self.requests
            .try_send(DeviceDiscoveryRequest { announce_success })
            .map_err(|error| match error {
                TrySendError::Full(_) => "A recording-device scan is already queued.".to_string(),
                TrySendError::Disconnected(_) => {
                    "The recording-device discovery worker stopped unexpectedly.".to_string()
                }
            })
    }

    pub(super) fn try_recv(&self) -> Option<DeviceDiscoveryResult> {
        self.results.try_recv().ok()
    }
}

fn run_worker(
    requests: Receiver<DeviceDiscoveryRequest>,
    results: Sender<DeviceDiscoveryResult>,
    ui_wake: UiWake,
) {
    while let Ok(request) = requests.recv() {
        let options = input_device_options().map_err(|error| format!("{error:#}"));
        if results
            .send(DeviceDiscoveryResult {
                announce_success: request.announce_success,
                options,
            })
            .is_err()
        {
            return;
        }
        ui_wake.request_root_repaint();
    }
}
