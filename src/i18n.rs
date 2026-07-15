use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

use crate::config::Language;
use crate::logging;

const MISSING_TRANSLATION_FALLBACK: &str = "Missing translation";
const PRIMARY_LANGUAGE_MASK: u16 = 0x03ff;
const LANG_CHINESE: u16 = 0x04;

const EN_US: &[(&str, &str)] = &[
    ("app.title", "CapsLock RS"),
    ("tray.enabled", "Enabled"),
    ("tray.start_with_windows", "Start with Windows"),
    ("tray.reload_config", "Reload config"),
    ("tray.open_config", "Open config"),
    ("tray.open_log", "Open log"),
    ("tray.settings_future", "Settings page (future)"),
    ("tray.exit", "Exit"),
    (
        "error.relaunch_as_admin_failed",
        "Failed to relaunch as admin:",
    ),
    ("error.reload_failed", "Reload failed:"),
    ("error.save_config_failed", "Failed to save config:"),
    ("error.update_startup_failed", "Failed to update startup:"),
    ("error.startup_failed", "Startup failed:"),
];

const ZH_CN: &[(&str, &str)] = &[
    ("app.title", "CapsLock RS"),
    ("tray.enabled", "启用"),
    ("tray.start_with_windows", "开机启动"),
    ("tray.reload_config", "重新加载配置"),
    ("tray.open_config", "打开配置"),
    ("tray.open_log", "打开日志"),
    ("tray.settings_future", "设置页面（未完成）"),
    ("tray.exit", "退出"),
    (
        "error.relaunch_as_admin_failed",
        "以管理员身份重新启动失败：",
    ),
    ("error.reload_failed", "重新加载失败："),
    ("error.save_config_failed", "保存配置失败："),
    ("error.update_startup_failed", "更新开机启动失败："),
    ("error.startup_failed", "启动失败："),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolvedLanguage {
    ZhCn,
    EnUs,
}

impl ResolvedLanguage {
    pub fn as_config_value(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::EnUs => "en-US",
        }
    }
}

pub fn text(language: Language, key: &str) -> &'static str {
    text_for_resolved_language(resolve_language(language), key)
}

pub fn message_with_detail(language: Language, summary_key: &str, detail: &str) -> String {
    format!("{}\n{}", text(language, summary_key), detail)
}

pub fn resolve_language(language: Language) -> ResolvedLanguage {
    match language {
        Language::System => system_language(),
        Language::ZhCn => ResolvedLanguage::ZhCn,
        Language::EnUs => ResolvedLanguage::EnUs,
    }
}

pub fn resolve_system_langid(lang_id: u16) -> ResolvedLanguage {
    if lang_id & PRIMARY_LANGUAGE_MASK == LANG_CHINESE {
        ResolvedLanguage::ZhCn
    } else {
        ResolvedLanguage::EnUs
    }
}

fn system_language() -> ResolvedLanguage {
    let lang_id = unsafe { GetUserDefaultUILanguage() };
    resolve_system_langid(lang_id)
}

fn text_for_resolved_language(language: ResolvedLanguage, key: &str) -> &'static str {
    if let Some(value) = lookup(table_for_language(language), key) {
        return value;
    }

    if language != ResolvedLanguage::EnUs {
        log_missing(language, key, "falling back to en-US");
        if let Some(value) = lookup(EN_US, key) {
            return value;
        }
    }

    log_missing(
        ResolvedLanguage::EnUs,
        key,
        "using missing translation fallback",
    );
    MISSING_TRANSLATION_FALLBACK
}

fn table_for_language(language: ResolvedLanguage) -> &'static [(&'static str, &'static str)] {
    match language {
        ResolvedLanguage::ZhCn => ZH_CN,
        ResolvedLanguage::EnUs => EN_US,
    }
}

fn lookup(table: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table
        .iter()
        .find_map(|(entry_key, value)| (*entry_key == key).then_some(*value))
}

fn log_missing(language: ResolvedLanguage, key: &str, fallback: &str) {
    static REPORTED: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();

    let reported = REPORTED.get_or_init(|| Mutex::new(BTreeSet::new()));
    let marker = format!("{}:{key}:{fallback}", language.as_config_value());
    let should_log = reported
        .lock()
        .map(|mut reported| reported.insert(marker))
        .unwrap_or(true);

    if should_log {
        logging::log_line(format!(
            "i18n missing key language={} key={} fallback={}",
            language.as_config_value(),
            key,
            fallback
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_chinese_system_language_to_zh_cn() {
        assert_eq!(resolve_system_langid(0x0804), ResolvedLanguage::ZhCn);
        assert_eq!(resolve_system_langid(0x0404), ResolvedLanguage::ZhCn);
    }

    #[test]
    fn resolves_non_chinese_system_language_to_en_us() {
        assert_eq!(resolve_system_langid(0x0409), ResolvedLanguage::EnUs);
        assert_eq!(resolve_system_langid(0x0411), ResolvedLanguage::EnUs);
    }

    #[test]
    fn returns_translated_text_for_selected_language() {
        assert_eq!(text(Language::ZhCn, "tray.exit"), "退出");
        assert_eq!(text(Language::EnUs, "tray.exit"), "Exit");
    }

    #[test]
    fn missing_key_uses_stable_english_fallback() {
        assert_eq!(
            text_for_resolved_language(ResolvedLanguage::ZhCn, "missing.test.key"),
            MISSING_TRANSLATION_FALLBACK
        );
    }
}
