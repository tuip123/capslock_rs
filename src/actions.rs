use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYEVENTF_KEYUP, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN, VK_ESCAPE,
    VK_LEFT, VK_RETURN, VK_RIGHT, VK_UP,
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
        LayerAction::MoveLeft(count) => send_repeated_key(VK_LEFT as u8, count, false),
        LayerAction::MoveDown(count) => send_repeated_key(VK_DOWN as u8, count, false),
        LayerAction::MoveUp(count) => send_repeated_key(VK_UP as u8, count, false),
        LayerAction::MoveRight(count) => send_repeated_key(VK_RIGHT as u8, count, false),
        LayerAction::MoveWordLeft(count) => send_repeated_key(VK_LEFT as u8, count, true),
        LayerAction::MoveWordRight(count) => send_repeated_key(VK_RIGHT as u8, count, true),
        LayerAction::Enter(count) => send_repeated_key(VK_RETURN as u8, count, false),
        LayerAction::Backspace(count) => send_repeated_key(VK_BACK as u8, count, false),
        LayerAction::Delete(count) => send_repeated_key(VK_DELETE as u8, count, false),
    }
}

fn send_capslock_tap(mode: TapCapsLock) {
    match mode {
        TapCapsLock::Toggle => send_key_tap(VK_CAPITAL as u8),
        TapCapsLock::Escape => send_key_tap(VK_ESCAPE as u8),
        TapCapsLock::None => {}
    }
}

fn send_repeated_key(vk: u8, count: u32, with_ctrl: bool) {
    if with_ctrl {
        send_key_down(VK_CONTROL as u8);
    }

    for _ in 0..count.max(1) {
        send_key_tap(vk);
    }

    if with_ctrl {
        send_key_up(VK_CONTROL as u8);
    }
}

fn send_key_tap(vk: u8) {
    send_key_down(vk);
    thread::sleep(Duration::from_millis(1));
    send_key_up(vk);
    logging::log_line(format!("sent key tap vk={vk}"));
}

fn send_key_down(vk: u8) {
    unsafe {
        keybd_event(vk, 0, 0, SYNTHETIC_EXTRA_INFO);
    }
}

fn send_key_up(vk: u8) {
    unsafe {
        keybd_event(vk, 0, KEYEVENTF_KEYUP, SYNTHETIC_EXTRA_INFO);
    }
}
