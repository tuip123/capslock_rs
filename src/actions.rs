use std::mem::size_of;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC_EX,
};

use crate::config::{BuiltInAction, LayerAction, TapCapsLock};
use crate::keys::{
    KeyCombo, KeyModifier, VK_APPS, VK_BACK, VK_CAPITAL, VK_DELETE, VK_DIVIDE, VK_DOWN, VK_END,
    VK_ESCAPE, VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN, VK_NEXT, VK_PRIOR, VK_RCONTROL, VK_RETURN,
    VK_RIGHT, VK_RMENU, VK_RWIN, VK_UP,
};
use crate::logging;

pub const SYNTHETIC_EXTRA_INFO: usize = 0x4350_5253;
const NO_MODIFIERS: &[KeyModifier] = &[];
const SHIFT_MODIFIER: &[KeyModifier] = &[KeyModifier::Shift];

#[derive(Clone, Debug)]
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
        LayerAction::BuiltIn(action) => send_builtin_action(action),
        LayerAction::KeyTap(key) => send_key_tap(key.vk),
        LayerAction::KeyCombo(combo) => send_key_combo(&combo),
    }
}

fn send_builtin_action(action: BuiltInAction) {
    match action {
        BuiltInAction::MoveLeft(count) => send_repeated_key(VK_LEFT, count, &[]),
        BuiltInAction::MoveDown(count) => send_repeated_key(VK_DOWN, count, &[]),
        BuiltInAction::MoveUp(count) => send_repeated_key(VK_UP, count, &[]),
        BuiltInAction::MoveRight(count) => send_repeated_key(VK_RIGHT, count, &[]),
        BuiltInAction::MoveWordLeft(count) => {
            send_repeated_key(VK_LEFT, count, &[KeyModifier::Ctrl])
        }
        BuiltInAction::MoveWordRight(count) => {
            send_repeated_key(VK_RIGHT, count, &[KeyModifier::Ctrl])
        }
        BuiltInAction::SelectLeft(count) => {
            send_repeated_key(VK_LEFT, count, &[KeyModifier::Shift])
        }
        BuiltInAction::SelectRight(count) => {
            send_repeated_key(VK_RIGHT, count, &[KeyModifier::Shift])
        }
        BuiltInAction::SelectUp(count) => send_repeated_key(VK_UP, count, &[KeyModifier::Shift]),
        BuiltInAction::SelectDown(count) => {
            send_repeated_key(VK_DOWN, count, &[KeyModifier::Shift])
        }
        BuiltInAction::SelectWordLeft(count) => {
            send_repeated_key(VK_LEFT, count, &[KeyModifier::Ctrl, KeyModifier::Shift])
        }
        BuiltInAction::SelectWordRight(count) => {
            send_repeated_key(VK_RIGHT, count, &[KeyModifier::Ctrl, KeyModifier::Shift])
        }
        BuiltInAction::Home(count) => send_repeated_key(VK_HOME, count, &[]),
        BuiltInAction::End(count) => send_repeated_key(VK_END, count, &[]),
        BuiltInAction::PageUp(count) => send_repeated_key(VK_PRIOR, count, &[]),
        BuiltInAction::PageDown(count) => send_repeated_key(VK_NEXT, count, &[]),
        BuiltInAction::Enter(count) => send_repeated_key(VK_RETURN, count, &[]),
        BuiltInAction::Backspace(count) => send_repeated_key(VK_BACK, count, &[]),
        BuiltInAction::Delete(count) => send_repeated_key(VK_DELETE, count, &[]),
        BuiltInAction::DeleteWord(count) => send_repeated_key(VK_BACK, count, &[KeyModifier::Ctrl]),
        BuiltInAction::ForwardDeleteWord(count) => {
            send_repeated_key(VK_DELETE, count, &[KeyModifier::Ctrl])
        }
        BuiltInAction::DeleteLine(count) => send_delete_line(count),
    }
}

fn send_capslock_tap(mode: TapCapsLock) {
    match mode {
        TapCapsLock::Toggle => send_key_tap(VK_CAPITAL),
        TapCapsLock::Escape => send_key_tap(VK_ESCAPE),
        TapCapsLock::None => {}
    }
}

fn send_key_combo(combo: &KeyCombo) {
    send_with_modifiers(&combo.modifiers, || send_key_tap(combo.key.vk));
}

fn send_repeated_key(vk: u16, count: u32, modifiers: &[KeyModifier]) {
    send_with_modifiers(modifiers, || {
        for _ in 0..count.max(1) {
            send_key_tap(vk);
        }
    });
}

fn send_delete_line(count: u32) {
    for _ in 0..count.max(1) {
        for (vk, modifiers) in delete_line_steps() {
            send_key_tap_with_modifiers(vk, modifiers);
        }
    }
}

fn delete_line_steps() -> [(u16, &'static [KeyModifier]); 3] {
    [
        (VK_HOME, NO_MODIFIERS),
        // Select to the next line start so the first character of that line is left untouched.
        (VK_DOWN, SHIFT_MODIFIER),
        (VK_BACK, NO_MODIFIERS),
    ]
}

fn send_key_tap_with_modifiers(vk: u16, modifiers: &[KeyModifier]) {
    if modifiers.is_empty() {
        send_key_tap(vk);
        return;
    }

    send_with_modifiers(modifiers, || send_key_tap(vk));
}

fn send_with_modifiers(modifiers: &[KeyModifier], action: impl FnOnce()) {
    for modifier in modifiers {
        send_key_down(modifier.output_vk());
    }

    action();

    for modifier in modifiers.iter().rev() {
        send_key_up(modifier.output_vk());
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
    let use_scan_code = scan.code != 0;
    let mut flags = if use_scan_code { KEYEVENTF_SCANCODE } else { 0 };
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
                // Prefer scancodes for physical keys; fall back to VKs for media keys.
                wVk: if use_scan_code { 0 } else { vk },
                wScan: if use_scan_code { scan.code } else { 0 },
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
    // Navigation, right-side modifiers and shell/media keys need the E0 scancode prefix.
    matches!(
        vk,
        VK_LEFT
            | VK_RIGHT
            | VK_UP
            | VK_DOWN
            | VK_INSERT
            | VK_DELETE
            | VK_HOME
            | VK_END
            | VK_PRIOR
            | VK_NEXT
            | VK_RCONTROL
            | VK_RMENU
            | VK_LWIN
            | VK_RWIN
            | VK_APPS
            | VK_DIVIDE
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BuiltInAction;
    use crate::keys::{parse_combo_suffix, VK_F1};

    #[test]
    fn target_combo_keeps_normalized_modifier_order() {
        let combo = parse_combo_suffix("shift_ctrl_f5").unwrap();

        assert_eq!(combo.modifiers, vec![KeyModifier::Ctrl, KeyModifier::Shift]);
        assert_eq!(combo.key.vk, VK_F1 + 4);
    }

    #[test]
    fn builtin_actions_are_distinct_for_editing_operations() {
        assert_ne!(
            LayerAction::BuiltIn(BuiltInAction::DeleteWord(1)),
            LayerAction::BuiltIn(BuiltInAction::ForwardDeleteWord(1))
        );
    }

    #[test]
    fn delete_line_selects_to_next_line_start() {
        let steps = delete_line_steps();

        assert_eq!(
            steps,
            [
                (VK_HOME, NO_MODIFIERS),
                (VK_DOWN, SHIFT_MODIFIER),
                (VK_BACK, NO_MODIFIERS)
            ]
        );
        assert!(!steps.iter().any(|(vk, _)| *vk == VK_RIGHT));
    }

    #[test]
    fn media_keys_fall_back_to_vk_input_when_scan_code_is_missing() {
        let input = keyboard_input(crate::keys::VK_MEDIA_PLAY_PAUSE, false);
        let keyboard = unsafe { input.Anonymous.ki };

        if keyboard.wScan == 0 {
            assert_eq!(keyboard.wVk, crate::keys::VK_MEDIA_PLAY_PAUSE);
            assert_eq!(keyboard.dwFlags & KEYEVENTF_SCANCODE, 0);
        }
    }
}
