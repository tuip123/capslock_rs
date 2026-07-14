use std::collections::BTreeSet;
use std::ptr::null_mut;
use std::sync::mpsc::Sender;

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    VK_CAPITAL, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::actions::{Action, SYNTHETIC_EXTRA_INFO};
use crate::app::APP_CONTEXT;
use crate::config::{Config, LayerAction, TapCapsLock};
use crate::logging;

pub struct KeyboardHook {
    handle: HHOOK,
}

pub struct HookState {
    enabled: bool,
    tap_capslock: TapCapsLock,
    mappings: Vec<(u32, bool, LayerAction)>,
    caps_down: bool,
    caps_used: bool,
    lalt_down: bool,
    lalt_suppressed: bool,
    suppressed_keys: BTreeSet<u32>,
    action_sender: Sender<Action>,
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
            lalt_down: false,
            lalt_suppressed: false,
            suppressed_keys: BTreeSet::new(),
            action_sender,
        }
    }

    pub fn apply_config(&mut self, config: &Config) {
        self.enabled = config.general.enabled;
        self.tap_capslock = config.general.tap_capslock;
        self.mappings = build_mappings(config);
        self.caps_down = false;
        self.caps_used = false;
        self.lalt_down = false;
        self.lalt_suppressed = false;
        self.suppressed_keys.clear();
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.caps_down = false;
        self.caps_used = false;
        self.lalt_down = false;
        self.lalt_suppressed = false;
        self.suppressed_keys.clear();
    }

    fn handle_event(&mut self, event: KeyEvent) -> bool {
        if !self.enabled {
            return false;
        }

        if is_lalt_key(event.vk) {
            return self.handle_lalt(event);
        }

        if event.vk == VK_CAPITAL as u32 {
            return self.handle_capslock(event);
        }

        if event.is_up && self.suppressed_keys.remove(&event.vk) {
            return true;
        }

        if self.caps_down && event.is_down {
            if !is_modifier_key(event.vk) {
                self.caps_used = true;
            }

            if let Some(action) = self.mapped_action(event.vk) {
                self.suppressed_keys.insert(event.vk);
                if let Err(error) = self.action_sender.send(Action::KeyTap(action)) {
                    logging::log_line(format!("failed to queue key action: {error}"));
                }
                return true;
            }
        }

        false
    }

    fn handle_lalt(&mut self, event: KeyEvent) -> bool {
        if event.is_down {
            self.lalt_down = true;
            if self.caps_down {
                self.caps_used = true;
                self.lalt_suppressed = true;
                return true;
            }
        }

        if event.is_up {
            self.lalt_down = false;
            if self.lalt_suppressed {
                self.lalt_suppressed = false;
                return true;
            }
            if self.caps_down {
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
        self.mappings
            .iter()
            .find_map(|(source_vk, require_lalt, action)| {
                (*source_vk == vk && *require_lalt == self.lalt_down).then_some(*action)
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

fn build_mappings(config: &Config) -> Vec<(u32, bool, LayerAction)> {
    config
        .capslock_layer
        .iter()
        .map(|mapping| (mapping.source_vk, mapping.require_lalt, mapping.action))
        .collect()
}

fn is_lalt_key(vk: u32) -> bool {
    vk == VK_LMENU as u32 || vk == VK_MENU as u32
}

fn is_modifier_key(vk: u32) -> bool {
    matches!(
        vk,
        value if value == VK_SHIFT as u32
            || value == VK_LSHIFT as u32
            || value == VK_RSHIFT as u32
            || value == VK_CONTROL as u32
            || value == VK_LCONTROL as u32
            || value == VK_RCONTROL as u32
            || value == VK_MENU as u32
            || value == VK_LMENU as u32
            || value == VK_RMENU as u32
            || value == VK_LWIN as u32
            || value == VK_RWIN as u32
    )
}
