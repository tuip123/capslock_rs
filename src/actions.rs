use std::mem::size_of;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC_EX, VK_BACK, VK_CAPITAL, VK_CONTROL,
    VK_DELETE, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_UP,
};

use crate::config::{LayerAction, TapCapsLock};
use crate::logging;

pub const SYNTHETIC_EXTRA_INFO: usize = 0x4350_5253;

#[derive(Clone, Copy, Debug)]
pub enum Action {
    KeyTap(LayerAction),
    TapCapsLock(TapCapsLock),
}

pub fn run_action_worker(receiver: Receiver<Action>) {
    while let Ok(action) = receiver.recv() {
        match action {
            Action::KeyTap(action) => send_layer_action(action),
            Action::TapCapsLock(mode) => send_capslock_tap(mode),
        }
    }
}

fn send_layer_action(action: LayerAction) {
    match action {
        LayerAction::MoveLeft(count) => send_repeated_key(VK_LEFT, count, false),
        LayerAction::MoveDown(count) => send_repeated_key(VK_DOWN, count, false),
        LayerAction::MoveUp(count) => send_repeated_key(VK_UP, count, false),
        LayerAction::MoveRight(count) => send_repeated_key(VK_RIGHT, count, false),
        LayerAction::MoveWordLeft(count) => send_repeated_key(VK_LEFT, count, true),
        LayerAction::MoveWordRight(count) => send_repeated_key(VK_RIGHT, count, true),
        LayerAction::Enter(count) => send_repeated_key(VK_RETURN, count, false),
        LayerAction::Backspace(count) => send_repeated_key(VK_BACK, count, false),
        LayerAction::Delete(count) => send_repeated_key(VK_DELETE, count, false),
    }
}

fn send_capslock_tap(mode: TapCapsLock) {
    match mode {
        TapCapsLock::Toggle => send_key_tap(VK_CAPITAL),
        TapCapsLock::Escape => send_key_tap(VK_ESCAPE),
        TapCapsLock::None => {}
    }
}

fn send_repeated_key(vk: u16, count: u32, with_ctrl: bool) {
    if with_ctrl {
        send_key_down(VK_CONTROL);
    }

    for _ in 0..count.max(1) {
        send_key_tap(vk);
    }

    if with_ctrl {
        send_key_up(VK_CONTROL);
    }
}

fn send_key_tap(vk: u16) {
    send_key_down(vk);
    thread::sleep(Duration::from_millis(1));
    send_key_up(vk);
    logging::log_line(format!("sent key tap vk={vk}"));
}

fn send_key_down(vk: u16) {
    send_key_event(vk, false);
}

fn send_key_up(vk: u16) {
    send_key_event(vk, true);
}

fn send_key_event(vk: u16, key_up: bool) {
    let input = keyboard_input(vk, key_up);
    let sent = unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) };
    if sent != 1 {
        logging::log_line(format!("failed to send input vk={vk} key_up={key_up}"));
    }
}

fn keyboard_input(vk: u16, key_up: bool) -> INPUT {
    let scan = scan_code(vk);
    let mut flags = KEYEVENTF_SCANCODE;
    if scan.is_extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if key_up {
        flags |= KEYEVENTF_KEYUP;
    }

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                // Scancode input is closer to a physical key than a VK-only keybd_event.
                wVk: 0,
                wScan: scan.code,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: SYNTHETIC_EXTRA_INFO,
            },
        },
    }
}

struct ScanCode {
    code: u16,
    is_extended: bool,
}

fn scan_code(vk: u16) -> ScanCode {
    let mapped = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC_EX) };
    let prefix = (mapped >> 8) & 0xff;

    ScanCode {
        code: (mapped & 0xff) as u16,
        is_extended: is_extended_key(vk) || prefix == 0xe0 || prefix == 0xe1,
    }
}

fn is_extended_key(vk: u16) -> bool {
    // Navigation keys share scancodes with numpad keys; E0 keeps them as real arrows/delete.
    matches!(vk, VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_DELETE)
}
