#![allow(dead_code)]

use crate::config::{Config, KeyMapping, Language, TapCapsLock};

#[derive(Clone, Debug)]
pub struct SettingsModel {
    pub enabled: bool,
    pub start_with_windows: bool,
    pub run_as_admin: bool,
    pub show_tray_icon: bool,
    pub tap_capslock: TapCapsLock,
    pub language: Language,
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
            language: config.ui.language,
            capslock_layer: config.capslock_layer.clone(),
        }
    }

    pub fn apply_to_config(&self, config: &mut Config) {
        config.general.enabled = self.enabled;
        config.general.start_with_windows = self.start_with_windows;
        config.general.run_as_admin = self.run_as_admin;
        config.general.show_tray_icon = self.show_tray_icon;
        config.general.tap_capslock = self.tap_capslock;
        config.ui.language = self.language;
        config.capslock_layer = self.capslock_layer.clone();
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_model_round_trips_language() {
        let mut config = Config::default();
        config.ui.language = Language::ZhCn;

        let mut model = SettingsModel::from_config(&config);
        assert_eq!(model.language, Language::ZhCn);

        model.language = Language::EnUs;
        model.apply_to_config(&mut config);

        assert_eq!(config.ui.language, Language::EnUs);
    }
}
