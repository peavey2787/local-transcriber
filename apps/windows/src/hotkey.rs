//! Windows global shortcut backend.
//!
//! Shortcuts are observed with a low-level keyboard hook instead of being
//! reserved with Win32 `RegisterHotKey`. This has two important properties:
//! another application cannot make the shortcut unavailable merely by owning
//! the same global hotkey, and the trigger key can be consumed before it reaches
//! the focused application. Consuming the trigger prevents single-key shortcuts
//! such as ` or - from being typed into an editor just before the transcription
//! is pasted.

use anyhow::{Context, Result};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TryRecvError};
use eframe::egui::Event as EguiEvent;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use std::cell::RefCell;
use std::collections::HashSet;
use std::thread::{self, JoinHandle};
use transcriber_core::hotkey::{HotkeyBackend, HotkeyBindings};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::ui_wake::UiWake;

type NativeCommandResult<T = ()> = std::result::Result<T, String>;

const WM_LOCAL_STT_HOTKEY_COMMAND: u32 = WM_APP + 0x31;
const KBDLL_FLAG_INJECTED: u32 = 0x10;
const MOD_MASK_SHIFT: u8 = 0x01;
const MOD_MASK_CONTROL: u8 = 0x02;
const MOD_MASK_ALT: u8 = 0x04;
const MOD_MASK_SUPER: u8 = 0x08;

#[derive(Clone, Copy)]
struct ActiveHotkey {
    hotkey: HotKey,
    id: u32,
    vk: u32,
    modifiers: u8,
}

struct HookState {
    active: Vec<ActiveHotkey>,
    down_keys: HashSet<u32>,
    suppressed_keys: HashSet<u32>,
    events: Sender<u32>,
    wake: UiWake,
}

thread_local! {
    static HOOK_STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

enum NativeCommand {
    Register(HotKey, Sender<NativeCommandResult>),
    Unregister(HotKey, Sender<NativeCommandResult>),
    Stop,
}

struct WindowsHotkeyManager {
    commands: Sender<NativeCommand>,
    worker_thread_id: u32,
    worker: Option<JoinHandle<()>>,
}

impl WindowsHotkeyManager {
    fn new(wake: UiWake, events: Sender<u32>) -> Result<Self> {
        let (commands, command_rx) = unbounded();
        let (ready_tx, ready_rx) = bounded(1);
        let worker = thread::Builder::new()
            .name("local-stt-win-hotkeys".to_string())
            .spawn(move || hotkey_worker(command_rx, events, wake, ready_tx))
            .context("start the Windows shortcut watcher thread")?;

        let worker_thread_id = ready_rx
            .recv()
            .context("wait for the Windows shortcut watcher to initialize")?
            .map_err(anyhow::Error::msg)?;

        Ok(Self {
            commands,
            worker_thread_id,
            worker: Some(worker),
        })
    }

    fn register(&self, hotkey: HotKey) -> Result<()> {
        self.request(|reply| NativeCommand::Register(hotkey, reply))
            .with_context(|| format!("activate {} as the Windows global shortcut", hotkey))
    }

    fn unregister(&self, hotkey: HotKey) -> Result<()> {
        self.request(|reply| NativeCommand::Unregister(hotkey, reply))
            .with_context(|| format!("deactivate {} as the Windows global shortcut", hotkey))
    }

    fn request(
        &self,
        make_command: impl FnOnce(Sender<NativeCommandResult>) -> NativeCommand,
    ) -> Result<()> {
        let (reply_tx, reply_rx) = bounded(1);
        self.commands
            .send(make_command(reply_tx))
            .map_err(|_| anyhow::anyhow!("the Windows hotkey thread stopped unexpectedly"))?;
        self.wake_worker()?;
        reply_rx
            .recv()
            .context("receive a reply from the Windows hotkey thread")?
            .map_err(anyhow::Error::msg)
    }

    fn wake_worker(&self) -> Result<()> {
        let posted = unsafe {
            PostThreadMessageW(
                self.worker_thread_id,
                WM_LOCAL_STT_HOTKEY_COMMAND,
                0,
                0,
            )
        };
        if posted == 0 {
            anyhow::bail!(
                "wake the Windows hotkey thread: {}",
                std::io::Error::last_os_error()
            );
        }
        Ok(())
    }
}

impl Drop for WindowsHotkeyManager {
    fn drop(&mut self) {
        let _ = self.commands.send(NativeCommand::Stop);
        let _ = unsafe {
            PostThreadMessageW(
                self.worker_thread_id,
                WM_LOCAL_STT_HOTKEY_COMMAND,
                0,
                0,
            )
        };
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn hotkey_worker(
    commands: Receiver<NativeCommand>,
    events: Sender<u32>,
    wake: UiWake,
    ready: Sender<NativeCommandResult<u32>>,
) {
    // Force creation of this worker's Win32 message queue before publishing the
    // thread id. PostThreadMessageW can only target a thread after its queue
    // exists.
    let worker_thread_id = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    let mut bootstrap_message: MSG = unsafe { std::mem::zeroed() };
    unsafe {
        PeekMessageW(
            &mut bootstrap_message,
            std::ptr::null_mut(),
            WM_USER,
            WM_USER,
            PM_NOREMOVE,
        );
    }

    HOOK_STATE.with(|slot| {
        *slot.borrow_mut() = Some(HookState {
            active: Vec::new(),
            down_keys: HashSet::new(),
            suppressed_keys: HashSet::new(),
            events,
            wake,
        });
    });

    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    let hook = unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), module, 0)
    };
    if hook.is_null() {
        HOOK_STATE.with(|slot| {
            slot.borrow_mut().take();
        });
        let _ = ready.send(Err(format!(
            "install the Windows low-level keyboard hook: {}",
            std::io::Error::last_os_error()
        )));
        return;
    }

    if ready.send(Ok(worker_thread_id)).is_err() {
        unsafe {
            UnhookWindowsHookEx(hook);
        }
        HOOK_STATE.with(|slot| {
            slot.borrow_mut().take();
        });
        return;
    }

    let mut running = true;
    while running {
        let mut message: MSG = unsafe { std::mem::zeroed() };
        let status = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
        if status <= 0 {
            break;
        }

        if message.message == WM_LOCAL_STT_HOTKEY_COMMAND {
            running = process_commands(&commands);
            continue;
        }

        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    unsafe {
        UnhookWindowsHookEx(hook);
    }
    HOOK_STATE.with(|slot| {
        slot.borrow_mut().take();
    });
}

fn process_commands(commands: &Receiver<NativeCommand>) -> bool {
    loop {
        match commands.try_recv() {
            Ok(NativeCommand::Register(hotkey, reply)) => {
                let result = register_hook_hotkey(hotkey);
                let _ = reply.send(result);
            }
            Ok(NativeCommand::Unregister(hotkey, reply)) => {
                unregister_hook_hotkey(hotkey);
                let _ = reply.send(Ok(()));
            }
            Ok(NativeCommand::Stop) => return false,
            Err(TryRecvError::Empty) => return true,
            Err(TryRecvError::Disconnected) => return false,
        }
    }
}

fn register_hook_hotkey(hotkey: HotKey) -> NativeCommandResult {
    let (_, vk) = native_hotkey_parts(hotkey)?;
    let id = native_hotkey_id(hotkey)?;
    let modifiers = hotkey_modifier_mask(hotkey);

    HOOK_STATE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let state = slot
            .as_mut()
            .ok_or_else(|| "the Windows keyboard hook is not initialized".to_string())?;
        if !state.active.iter().any(|entry| entry.hotkey == hotkey) {
            state.active.push(ActiveHotkey {
                hotkey,
                id,
                vk: vk as u32,
                modifiers,
            });
        }
        Ok(())
    })
}

fn unregister_hook_hotkey(hotkey: HotKey) {
    HOOK_STATE.with(|slot| {
        if let Some(state) = slot.borrow_mut().as_mut() {
            state.active.retain(|entry| entry.hotkey != hotkey);
        }
    });
}

unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if code < 0 {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
    }

    let message = wparam as u32;
    let is_down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    let is_up = message == WM_KEYUP || message == WM_SYSKEYUP;
    if !is_down && !is_up {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
    }

    let event = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };

    // Do not react to SendInput/keybd_event traffic. In particular, the app's
    // own Ctrl+V auto-paste must never retrigger or be swallowed by the hotkey
    // hook when the chosen shortcut happens to involve V or a modifier.
    if event.flags & KBDLL_FLAG_INJECTED != 0 {
        return unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) };
    }

    let vk = event.vkCode;
    let suppress = HOOK_STATE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(state) = slot.as_mut() else {
            return false;
        };

        if is_down {
            let was_down = !state.down_keys.insert(vk);

            // Auto-repeat for a trigger key must remain consumed, but it must
            // not generate a second recording event while the physical key is
            // still held.
            if state.suppressed_keys.contains(&vk) {
                return true;
            }

            if !was_down {
                let modifiers = current_modifier_mask(&state.down_keys);
                if let Some(id) = state
                    .active
                    .iter()
                    .find(|entry| entry.vk == vk && entry.modifiers == modifiers)
                    .map(|entry| entry.id)
                {
                    let _ = state.events.send(id);
                    state.wake.request_repaint();
                    state.suppressed_keys.insert(vk);
                    return true;
                }
            }
            false
        } else {
            state.down_keys.remove(&vk);
            state.suppressed_keys.remove(&vk)
        }
    });

    if suppress {
        // A non-zero return from WH_KEYBOARD_LL prevents this keystroke from
        // reaching the focused application. Swallow both key-down and key-up
        // for the trigger so editors never receive a stray `, -, etc.
        1
    } else {
        unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
    }
}

fn hotkey_modifier_mask(hotkey: HotKey) -> u8 {
    let mut mask = 0u8;
    if hotkey.mods.contains(Modifiers::SHIFT) {
        mask |= MOD_MASK_SHIFT;
    }
    if hotkey.mods.contains(Modifiers::CONTROL) {
        mask |= MOD_MASK_CONTROL;
    }
    if hotkey.mods.contains(Modifiers::ALT) {
        mask |= MOD_MASK_ALT;
    }
    if hotkey.mods.intersects(Modifiers::SUPER | Modifiers::META) {
        mask |= MOD_MASK_SUPER;
    }
    mask
}

fn current_modifier_mask(down_keys: &HashSet<u32>) -> u8 {
    let mut mask = 0u8;
    if any_key_down(down_keys, &[VK_SHIFT, VK_LSHIFT, VK_RSHIFT]) {
        mask |= MOD_MASK_SHIFT;
    }
    if any_key_down(down_keys, &[VK_CONTROL, VK_LCONTROL, VK_RCONTROL]) {
        mask |= MOD_MASK_CONTROL;
    }
    if any_key_down(down_keys, &[VK_MENU, VK_LMENU, VK_RMENU]) {
        mask |= MOD_MASK_ALT;
    }
    if any_key_down(down_keys, &[VK_LWIN, VK_RWIN]) {
        mask |= MOD_MASK_SUPER;
    }
    mask
}

fn any_key_down(down_keys: &HashSet<u32>, keys: &[VIRTUAL_KEY]) -> bool {
    keys.iter().any(|key| down_keys.contains(&(*key as u32)))
}

fn validate_native_hotkey(hotkey: HotKey) -> NativeCommandResult {
    native_hotkey_parts(hotkey).map(|_| ())
}

fn native_hotkey_id(hotkey: HotKey) -> NativeCommandResult<u32> {
    let (_, vk) = native_hotkey_parts(hotkey)?;
    let mut modifier_id = 0u32;
    if hotkey.mods.contains(Modifiers::SHIFT) {
        modifier_id |= 0x1;
    }
    if hotkey.mods.contains(Modifiers::CONTROL) {
        modifier_id |= 0x2;
    }
    if hotkey.mods.contains(Modifiers::ALT) {
        modifier_id |= 0x4;
    }
    if hotkey.mods.intersects(Modifiers::SUPER | Modifiers::META) {
        modifier_id |= 0x8;
    }

    // Keep a stable collision-free identifier for the supported modifier/key pairs.
    // All Windows virtual-key values fit in one byte.
    Ok((modifier_id << 8) | (vk as u32 & 0xFF))
}

fn native_hotkey_parts(hotkey: HotKey) -> NativeCommandResult<(u32, VIRTUAL_KEY)> {
    let mut modifiers = 0u32;
    if hotkey.mods.contains(Modifiers::SHIFT) {
        modifiers |= MOD_SHIFT;
    }
    if hotkey.mods.contains(Modifiers::CONTROL) {
        modifiers |= MOD_CONTROL;
    }
    if hotkey.mods.contains(Modifiers::ALT) {
        modifiers |= MOD_ALT;
    }
    if hotkey.mods.intersects(Modifiers::SUPER | Modifiers::META) {
        modifiers |= MOD_WIN;
    }

    let vk = key_to_vk(hotkey.key)
        .ok_or_else(|| format!("Windows does not expose a virtual-key code for {}", hotkey.key))?;
    Ok((modifiers, vk))
}

fn key_to_vk(key: Code) -> Option<VIRTUAL_KEY> {
    Some(match key {
        Code::KeyA => VK_A,
        Code::KeyB => VK_B,
        Code::KeyC => VK_C,
        Code::KeyD => VK_D,
        Code::KeyE => VK_E,
        Code::KeyF => VK_F,
        Code::KeyG => VK_G,
        Code::KeyH => VK_H,
        Code::KeyI => VK_I,
        Code::KeyJ => VK_J,
        Code::KeyK => VK_K,
        Code::KeyL => VK_L,
        Code::KeyM => VK_M,
        Code::KeyN => VK_N,
        Code::KeyO => VK_O,
        Code::KeyP => VK_P,
        Code::KeyQ => VK_Q,
        Code::KeyR => VK_R,
        Code::KeyS => VK_S,
        Code::KeyT => VK_T,
        Code::KeyU => VK_U,
        Code::KeyV => VK_V,
        Code::KeyW => VK_W,
        Code::KeyX => VK_X,
        Code::KeyY => VK_Y,
        Code::KeyZ => VK_Z,
        Code::Digit0 => VK_0,
        Code::Digit1 => VK_1,
        Code::Digit2 => VK_2,
        Code::Digit3 => VK_3,
        Code::Digit4 => VK_4,
        Code::Digit5 => VK_5,
        Code::Digit6 => VK_6,
        Code::Digit7 => VK_7,
        Code::Digit8 => VK_8,
        Code::Digit9 => VK_9,
        Code::Equal => VK_OEM_PLUS,
        Code::Comma => VK_OEM_COMMA,
        Code::Minus => VK_OEM_MINUS,
        Code::Period => VK_OEM_PERIOD,
        Code::Semicolon => VK_OEM_1,
        Code::Slash => VK_OEM_2,
        Code::Backquote => VK_OEM_3,
        Code::BracketLeft => VK_OEM_4,
        Code::Backslash => VK_OEM_5,
        Code::BracketRight => VK_OEM_6,
        Code::Quote => VK_OEM_7,
        Code::Backspace => VK_BACK,
        Code::Tab => VK_TAB,
        Code::Space => VK_SPACE,
        Code::Enter | Code::NumpadEnter => VK_RETURN,
        Code::CapsLock => VK_CAPITAL,
        Code::Escape => VK_ESCAPE,
        Code::PageUp => VK_PRIOR,
        Code::PageDown => VK_NEXT,
        Code::End => VK_END,
        Code::Home => VK_HOME,
        Code::ArrowLeft => VK_LEFT,
        Code::ArrowUp => VK_UP,
        Code::ArrowRight => VK_RIGHT,
        Code::ArrowDown => VK_DOWN,
        Code::PrintScreen => VK_SNAPSHOT,
        Code::Insert => VK_INSERT,
        Code::Delete => VK_DELETE,
        Code::F1 => VK_F1,
        Code::F2 => VK_F2,
        Code::F3 => VK_F3,
        Code::F4 => VK_F4,
        Code::F5 => VK_F5,
        Code::F6 => VK_F6,
        Code::F7 => VK_F7,
        Code::F8 => VK_F8,
        Code::F9 => VK_F9,
        Code::F10 => VK_F10,
        Code::F11 => VK_F11,
        Code::F12 => VK_F12,
        Code::F13 => VK_F13,
        Code::F14 => VK_F14,
        Code::F15 => VK_F15,
        Code::F16 => VK_F16,
        Code::F17 => VK_F17,
        Code::F18 => VK_F18,
        Code::F19 => VK_F19,
        Code::F20 => VK_F20,
        Code::F21 => VK_F21,
        Code::F22 => VK_F22,
        Code::F23 => VK_F23,
        Code::F24 => VK_F24,
        Code::NumLock => VK_NUMLOCK,
        Code::Numpad0 => VK_NUMPAD0,
        Code::Numpad1 => VK_NUMPAD1,
        Code::Numpad2 => VK_NUMPAD2,
        Code::Numpad3 => VK_NUMPAD3,
        Code::Numpad4 => VK_NUMPAD4,
        Code::Numpad5 => VK_NUMPAD5,
        Code::Numpad6 => VK_NUMPAD6,
        Code::Numpad7 => VK_NUMPAD7,
        Code::Numpad8 => VK_NUMPAD8,
        Code::Numpad9 => VK_NUMPAD9,
        Code::NumpadAdd => VK_ADD,
        Code::NumpadDecimal => VK_DECIMAL,
        Code::NumpadDivide => VK_DIVIDE,
        Code::NumpadEqual => VK_OEM_PLUS,
        Code::NumpadMultiply => VK_MULTIPLY,
        Code::NumpadSubtract => VK_SUBTRACT,
        Code::ScrollLock => VK_SCROLL,
        Code::AudioVolumeDown => VK_VOLUME_DOWN,
        Code::AudioVolumeUp => VK_VOLUME_UP,
        Code::AudioVolumeMute => VK_VOLUME_MUTE,
        Code::MediaPlay => VK_PLAY,
        Code::MediaPause | Code::Pause => VK_PAUSE,
        Code::MediaPlayPause => VK_MEDIA_PLAY_PAUSE,
        Code::MediaStop => VK_MEDIA_STOP,
        Code::MediaTrackNext => VK_MEDIA_NEXT_TRACK,
        Code::MediaTrackPrevious => VK_MEDIA_PREV_TRACK,
        _ => return None,
    })
}

struct WindowsHotkeyBackend {
    manager: WindowsHotkeyManager,
}

impl HotkeyBackend for WindowsHotkeyBackend {
    type Hotkey = HotKey;

    fn parse(&self, shortcut: &str) -> Result<HotKey> {
        parse(shortcut)
    }

    fn register(&mut self, hotkey: HotKey) -> Result<()> {
        self.manager.register(hotkey)
    }

    fn unregister(&mut self, hotkey: HotKey) -> Result<()> {
        self.manager.unregister(hotkey)
    }

    fn id(hotkey: HotKey) -> u32 {
        native_hotkey_id(hotkey).unwrap_or(u32::MAX)
    }
}

pub struct Hotkeys {
    bindings: HotkeyBindings<WindowsHotkeyBackend>,
}

impl Hotkeys {
    pub fn register(wake: UiWake, shortcut: &str) -> Result<(Self, Option<String>)> {
        let (tx, rx) = unbounded();
        let manager = WindowsHotkeyManager::new(wake, tx)?;
        let (bindings, warning) =
            HotkeyBindings::new(WindowsHotkeyBackend { manager }, rx, shortcut)?;
        Ok((Self { bindings }, warning))
    }

    pub fn rebind(&mut self, shortcut: &str) -> Result<()> {
        self.bindings.rebind(shortcut)
    }

    pub fn configure_voice_commands(&mut self, enabled: bool, shortcut: &str) -> Result<()> {
        self.bindings.configure_voice_commands(enabled, shortcut)
    }

    pub fn is_bound(&self) -> bool {
        self.bindings.is_bound()
    }

    pub fn is_voice_commands_bound(&self) -> bool {
        self.bindings.is_voice_commands_bound()
    }

    pub fn poll(&self) -> HotkeyPresses {
        self.bindings.poll()
    }
}

pub fn validate(shortcut: &str) -> Result<()> {
    let hotkey = parse(shortcut)?;
    validate_native_hotkey(hotkey).map_err(anyhow::Error::msg)?;
    Ok(())
}

fn parse(shortcut: &str) -> Result<HotKey> {
    let trimmed = shortcut.trim();
    if trimmed.is_empty() {
        anyhow::bail!("hotkey cannot be empty");
    }
    trimmed
        .parse::<HotKey>()
        .map_err(|error| anyhow::anyhow!("invalid hotkey {trimmed:?}: {error}"))
}

pub use transcriber_core::hotkey::{friendly_name, same_shortcut, HotkeyPresses};
pub use transcriber_ui::shortcut::CaptureOutcome;

pub fn capture_shortcut(event: &EguiEvent) -> Option<CaptureOutcome> {
    transcriber_ui::shortcut::capture_shortcut(event, validate)
}
