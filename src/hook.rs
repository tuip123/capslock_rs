use std::collections::BTreeSet;
use std::ptr::null_mut;
use std::sync::mpsc::Sender;

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::actions::{Action, SYNTHETIC_EXTRA_INFO};
use crate::app::APP_CONTEXT;
use crate::config::{Config, LayerAction, TapCapsLock};
use crate::keys::{modifier_from_vk, KeyCombo, KeyModifier, ModifierFamily, VK_CAPITAL};
use crate::logging;

pub struct KeyboardHook {
    handle: HHOOK,
}

pub struct HookState {
    enabled: bool,
    tap_capslock: TapCapsLock,
    mappings: Vec<HookMapping>,
    caps_down: bool,
    caps_used: bool,
    active_modifiers: BTreeSet<KeyModifier>,
    suppressed_modifiers: BTreeSet<KeyModifier>,
    suppressed_keys: BTreeSet<u32>,
    action_sender: Sender<Action>,
}

#[derive(Clone, Debug)]
struct HookMapping {
    source: KeyCombo,
    action: LayerAction,
}

#[derive(Clone, Copy)]
struct KeyEvent {
    vk: u32,
    is_down: bool,
    is_up: bool,
}

impl KeyboardHook {
    pub fn install() -> Result<Self, String> {
        let handle =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), null_mut(), 0) };
        if handle.is_null() {
            return Err("failed to install low level keyboard hook".to_string());
        }

        logging::log_line("keyboard hook installed");
        Ok(Self { handle })
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        unsafe {
            UnhookWindowsHookEx(self.handle);
        }
        logging::log_line("keyboard hook removed");
    }
}

impl HookState {
    pub fn from_config(config: &Config, action_sender: Sender<Action>) -> Self {
        Self {
            enabled: config.general.enabled,
            tap_capslock: config.general.tap_capslock,
            mappings: build_mappings(config),
            caps_down: false,
            caps_used: false,
            active_modifiers: BTreeSet::new(),
            suppressed_modifiers: BTreeSet::new(),
            suppressed_keys: BTreeSet::new(),
            action_sender,
        }
    }

    pub fn apply_config(&mut self, config: &Config) {
        self.enabled = config.general.enabled;
        self.tap_capslock = config.general.tap_capslock;
        self.mappings = build_mappings(config);
        self.reset_transient_state();
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.reset_transient_state();
    }

    fn reset_transient_state(&mut self) {
        self.caps_down = false;
        self.caps_used = false;
        self.active_modifiers.clear();
        self.suppressed_modifiers.clear();
        self.suppressed_keys.clear();
    }

    fn handle_event(&mut self, event: KeyEvent) -> bool {
        if !self.enabled {
            return false;
        }

        if event.vk == VK_CAPITAL as u32 {
            return self.handle_capslock(event);
        }

        if let Some(modifier) = modifier_from_vk(event.vk as u16) {
            return self.handle_modifier(event, modifier);
        }

        if event.is_up && self.suppressed_keys.remove(&event.vk) {
            return true;
        }

        if self.caps_down && event.is_down {
            if let Some(action) = self.mapped_action(event.vk) {
                self.caps_used = true;
                self.suppressed_keys.insert(event.vk);
                if let Err(error) = self.action_sender.send(Action::KeyTap(action)) {
                    logging::log_line(format!("failed to queue key action: {error}"));
                }
                return true;
            }

            self.caps_used = true;
        }

        false
    }

    fn handle_modifier(&mut self, event: KeyEvent, modifier: KeyModifier) -> bool {
        if event.is_down {
            self.active_modifiers.insert(modifier);
            if self.caps_down {
                self.caps_used = true;
                self.suppressed_modifiers.insert(modifier);
                return true;
            }
            return false;
        }

        if event.is_up {
            remove_modifier(&mut self.active_modifiers, modifier);
            let was_suppressed = remove_modifier(&mut self.suppressed_modifiers, modifier);
            if was_suppressed || self.caps_down {
                return true;
            }
        }

        false
    }

    fn handle_capslock(&mut self, event: KeyEvent) -> bool {
        if event.is_down {
            if !self.caps_down {
                self.caps_down = true;
                self.caps_used = false;
            }
            return true;
        }

        if event.is_up {
            if self.caps_down && !self.caps_used {
                if let Err(error) = self
                    .action_sender
                    .send(Action::TapCapsLock(self.tap_capslock))
                {
                    logging::log_line(format!("failed to queue capslock tap: {error}"));
                }
            }

            self.caps_down = false;
            self.caps_used = false;
            return true;
        }

        false
    }

    fn mapped_action(&self, vk: u32) -> Option<LayerAction> {
        self.mappings.iter().find_map(|mapping| {
            (mapping.source.key.vk as u32 == vk
                && modifiers_match(&mapping.source, &self.active_modifiers))
            .then(|| mapping.action.clone())
        })
    }
}

unsafe extern "system" fn keyboard_proc(n_code: i32, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    if n_code < 0 {
        return CallNextHookEx(null_mut(), n_code, w_param, l_param);
    }

    let keyboard = &*(l_param as *const KBDLLHOOKSTRUCT);
    if keyboard.dwExtraInfo == SYNTHETIC_EXTRA_INFO
        || (keyboard.flags & LLKHF_INJECTED) == LLKHF_INJECTED
    {
        return CallNextHookEx(null_mut(), n_code, w_param, l_param);
    }

    let message = w_param as u32;
    let event = KeyEvent {
        vk: keyboard.vkCode,
        is_down: message == WM_KEYDOWN || message == WM_SYSKEYDOWN,
        is_up: message == WM_KEYUP || message == WM_SYSKEYUP,
    };

    if !event.is_down && !event.is_up {
        return CallNextHookEx(null_mut(), n_code, w_param, l_param);
    }

    let Some(context) = APP_CONTEXT.get() else {
        return CallNextHookEx(null_mut(), n_code, w_param, l_param);
    };

    let swallow = match context.hook_state.lock() {
        Ok(mut state) => state.handle_event(event),
        Err(_) => {
            logging::log_line("hook state lock is poisoned");
            false
        }
    };

    if swallow {
        1
    } else {
        CallNextHookEx(null_mut(), n_code, w_param, l_param)
    }
}

fn build_mappings(config: &Config) -> Vec<HookMapping> {
    config
        .capslock_layer
        .iter()
        .map(|mapping| HookMapping {
            source: mapping.source.clone(),
            action: mapping.action.clone(),
        })
        .collect()
}

fn modifiers_match(source: &KeyCombo, active: &BTreeSet<KeyModifier>) -> bool {
    if modifier_families(&source.modifiers) != modifier_families(active) {
        return false;
    }

    source.modifiers.iter().all(|required| {
        active
            .iter()
            .any(|active| modifier_satisfies(*active, *required))
    })
}

fn modifier_families<'a>(
    modifiers: impl IntoIterator<Item = &'a KeyModifier>,
) -> BTreeSet<ModifierFamily> {
    modifiers
        .into_iter()
        .map(|modifier| modifier.family())
        .collect()
}

fn modifier_satisfies(active: KeyModifier, required: KeyModifier) -> bool {
    active == required
        || (is_generic_modifier(required) && active.family() == required.family())
        || (is_generic_modifier(active) && active.family() == required.family())
}

fn remove_modifier(modifiers: &mut BTreeSet<KeyModifier>, modifier: KeyModifier) -> bool {
    let before = modifiers.len();
    modifiers.retain(|active| !same_modifier_event(*active, modifier));
    modifiers.len() != before
}

fn same_modifier_event(active: KeyModifier, event: KeyModifier) -> bool {
    active == event
        || (is_generic_modifier(event) && active.family() == event.family())
        || (is_generic_modifier(active) && active.family() == event.family())
}

fn is_generic_modifier(modifier: KeyModifier) -> bool {
    matches!(
        modifier,
        KeyModifier::Ctrl | KeyModifier::Alt | KeyModifier::Shift | KeyModifier::Win
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    use crate::config::{BuiltInAction, LayerAction};
    use crate::keys::{parse_combo_suffix, VK_LCONTROL, VK_LMENU, VK_RMENU};

    #[test]
    fn matches_normalized_multi_modifier_mapping() {
        let config = Config::from_ini(
            r#"
            [Keys]
            caps_shift_lalt_h=keyFunc_selectLeft(2)
            "#,
        )
        .unwrap();
        let (sender, receiver) = mpsc::channel();
        let mut state = HookState::from_config(&config, sender);

        assert!(state.handle_event(down(VK_CAPITAL as u32)));
        assert!(state.handle_event(down(VK_LMENU as u32)));
        assert!(state.handle_event(down(crate::keys::VK_SHIFT as u32)));
        assert!(state.handle_event(down(b'H' as u32)));

        let action = receiver.try_recv().unwrap();
        assert!(matches!(
            action,
            Action::KeyTap(LayerAction::BuiltIn(BuiltInAction::SelectLeft(2)))
        ));
        assert!(state.handle_event(up(b'H' as u32)));
        assert!(state.handle_event(up(crate::keys::VK_SHIFT as u32)));
        assert!(state.handle_event(up(VK_LMENU as u32)));
        assert!(state.handle_event(up(VK_CAPITAL as u32)));
    }

    #[test]
    fn side_specific_modifier_does_not_match_other_side() {
        let source = parse_combo_suffix("lalt_h").unwrap();
        let mut active = BTreeSet::new();
        active.insert(KeyModifier::RAlt);

        assert!(!modifiers_match(&source, &active));

        let source = parse_combo_suffix("alt_h").unwrap();
        assert!(modifiers_match(&source, &active));
    }

    #[test]
    fn modifier_release_is_suppressed_after_caps_release() {
        let config = Config::from_ini("[Keys]\ncaps_lctrl_h=keyFunc_moveLeft\n").unwrap();
        let (sender, _receiver) = mpsc::channel();
        let mut state = HookState::from_config(&config, sender);

        assert!(state.handle_event(down(VK_CAPITAL as u32)));
        assert!(state.handle_event(down(VK_LCONTROL as u32)));
        assert!(state.handle_event(up(VK_CAPITAL as u32)));
        assert!(state.handle_event(up(VK_LCONTROL as u32)));
    }

    #[test]
    fn exact_modifier_set_is_required() {
        let source = parse_combo_suffix("ctrl_h").unwrap();
        let mut active = BTreeSet::new();
        active.insert(KeyModifier::Ctrl);
        active.insert(KeyModifier::Alt);

        assert!(!modifiers_match(&source, &active));
    }

    #[test]
    fn remove_generic_modifier_cleans_side_specific_state() {
        let mut active = BTreeSet::new();
        active.insert(KeyModifier::RAlt);

        assert!(remove_modifier(&mut active, KeyModifier::Alt));
        assert!(active.is_empty());
        assert!(!remove_modifier(&mut active, KeyModifier::RAlt));
        assert_eq!(modifier_from_vk(VK_RMENU), Some(KeyModifier::RAlt));
    }

    fn down(vk: u32) -> KeyEvent {
        KeyEvent {
            vk,
            is_down: true,
            is_up: false,
        }
    }

    fn up(vk: u32) -> KeyEvent {
        KeyEvent {
            vk,
            is_down: false,
            is_up: true,
        }
    }
}
