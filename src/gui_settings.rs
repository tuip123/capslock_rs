#![allow(dead_code)]

use crate::config::{Config, KeyMapping, TapCapsLock};

#[derive(Clone, Debug)]
pub struct SettingsModel {
    pub enabled: bool,
    pub start_with_windows: bool,
    pub run_as_admin: bool,
    pub show_tray_icon: bool,
    pub tap_capslock: TapCapsLock,
    pub capslock_layer: Vec<KeyMapping>,
}

impl SettingsModel {
    pub fn from_config(config: &Config) -> Self {
        Self {
            enabled: config.general.enabled,
            start_with_windows: config.general.start_with_windows,
            run_as_admin: config.general.run_as_admin,
            show_tray_icon: config.general.show_tray_icon,
            tap_capslock: config.general.tap_capslock,
            capslock_layer: config.capslock_layer.clone(),
        }
    }

    pub fn apply_to_config(&self, config: &mut Config) {
        config.general.enabled = self.enabled;
        config.general.start_with_windows = self.start_with_windows;
        config.general.run_as_admin = self.run_as_admin;
        config.general.show_tray_icon = self.show_tray_icon;
        config.general.tap_capslock = self.tap_capslock;
        config.capslock_layer = self.capslock_layer.clone();
    }
}
