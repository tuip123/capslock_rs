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
    ("tray.settings", "Settings"),
    ("tray.exit", "Exit"),
    ("settings.title", "CapsLock RS Settings"),
    ("settings.enabled", "Enable CapsLock layer"),
    ("settings.start_with_windows", "Start with Windows"),
    ("settings.run_as_admin", "Run as administrator"),
    ("settings.show_tray_icon", "Show tray icon"),
    ("settings.tap_capslock", "Tap CapsLock"),
    ("settings.tap_capslock.toggle", "Toggle CapsLock"),
    ("settings.tap_capslock.escape", "Send Escape"),
    ("settings.tap_capslock.none", "Do nothing"),
    ("settings.language", "Language"),
    ("settings.language.system", "Follow system"),
    ("settings.language.zh_cn", "Simplified Chinese"),
    ("settings.language.en_us", "English"),
    ("settings.config_path", "Current config path"),
    ("settings.log_path", "Current log path"),
    ("settings.open", "Open"),
    ("settings.save", "Save"),
    ("settings.close", "Close"),
    ("settings.saved", "Saved and reloaded."),
    ("settings.save_failed", "Save failed."),
    (
        "error.relaunch_as_admin_failed",
        "Failed to relaunch as admin:",
    ),
    ("error.reload_failed", "Reload failed:"),
    ("error.save_config_failed", "Failed to save config:"),
    ("error.save_settings_failed", "Failed to save settings:"),
    ("error.update_startup_failed", "Failed to update startup:"),
    (
        "error.update_tray_icon_failed",
        "Failed to update tray icon:",
    ),
    ("error.open_settings_failed", "Failed to open settings:"),
    ("error.startup_failed", "Startup failed:"),
];

const ZH_CN: &[(&str, &str)] = &[
    ("app.title", "CapsLock RS"),
    ("tray.enabled", "启用"),
    ("tray.start_with_windows", "开机启动"),
    ("tray.reload_config", "重新加载配置"),
    ("tray.open_config", "打开配置"),
    ("tray.open_log", "打开日志"),
    ("tray.settings", "设置"),
    ("tray.exit", "退出"),
    ("settings.title", "CapsLock RS 设置"),
    ("settings.enabled", "启用 CapsLock 功能层"),
    ("settings.start_with_windows", "开机启动"),
    ("settings.run_as_admin", "以管理员身份运行"),
    ("settings.show_tray_icon", "显示托盘图标"),
    ("settings.tap_capslock", "单击 CapsLock"),
    ("settings.tap_capslock.toggle", "切换大小写"),
    ("settings.tap_capslock.escape", "发送 Escape"),
    ("settings.tap_capslock.none", "不执行动作"),
    ("settings.language", "界面语言"),
    ("settings.language.system", "跟随系统"),
    ("settings.language.zh_cn", "简体中文"),
    ("settings.language.en_us", "English"),
    ("settings.config_path", "当前配置路径"),
    ("settings.log_path", "当前日志路径"),
    ("settings.open", "打开"),
    ("settings.save", "保存"),
    ("settings.close", "关闭"),
    ("settings.saved", "已保存并重新加载。"),
    ("settings.save_failed", "保存失败。"),
    (
        "error.relaunch_as_admin_failed",
        "以管理员身份重新启动失败：",
    ),
    ("error.reload_failed", "重新加载失败："),
    ("error.save_config_failed", "保存配置失败："),
    ("error.save_settings_failed", "保存设置失败："),
    ("error.update_startup_failed", "更新开机启动失败："),
    ("error.update_tray_icon_failed", "更新托盘图标失败："),
    ("error.open_settings_failed", "打开设置失败："),
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
    text_from_tables(language, table_for_language(language), EN_US, key)
}

fn text_from_tables(
    language: ResolvedLanguage,
    primary_table: &'static [(&'static str, &'static str)],
    english_table: &'static [(&'static str, &'static str)],
    key: &str,
) -> &'static str {
    if let Some(value) = lookup(primary_table, key) {
        return value;
    }

    if language != ResolvedLanguage::EnUs {
        log_missing(language, key, "falling back to en-US");
        if let Some(value) = lookup(english_table, key) {
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

#[cfg(test)]
fn key_set(table: &'static [(&'static str, &'static str)]) -> BTreeSet<&'static str> {
    table.iter().map(|(key, _)| *key).collect()
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
    fn zh_cn_and_en_us_have_matching_translation_keys() {
        let en_us_keys = key_set(EN_US);
        let zh_cn_keys = key_set(ZH_CN);

        assert_eq!(en_us_keys, zh_cn_keys);
        assert_eq!(en_us_keys.len(), EN_US.len(), "en-US has duplicate keys");
        assert_eq!(zh_cn_keys.len(), ZH_CN.len(), "zh-CN has duplicate keys");
    }

    #[test]
    fn missing_localized_key_falls_back_to_en_us_when_available() {
        static PARTIAL_ZH_CN: &[(&str, &str)] = &[("shared.key", "中文")];
        static COMPLETE_EN_US: &[(&str, &str)] = &[
            ("shared.key", "English"),
            ("fallback.only", "English fallback"),
        ];

        assert_eq!(
            text_from_tables(
                ResolvedLanguage::ZhCn,
                PARTIAL_ZH_CN,
                COMPLETE_EN_US,
                "fallback.only"
            ),
            "English fallback"
        );
    }

    #[test]
    fn missing_key_uses_stable_fallback_and_records_once() {
        let key = "missing.test.logged_key";

        assert_eq!(
            text_for_resolved_language(ResolvedLanguage::ZhCn, key),
            MISSING_TRANSLATION_FALLBACK
        );
        assert_eq!(
            text_for_resolved_language(ResolvedLanguage::ZhCn, key),
            MISSING_TRANSLATION_FALLBACK
        );

        let lines = logging::captured_lines_for_test();
        let zh_cn_logs = lines
            .iter()
            .filter(|line| {
                line.contains("language=zh-CN")
                    && line.contains(&format!("key={key}"))
                    && line.contains("fallback=falling back to en-US")
            })
            .count();
        let en_us_logs = lines
            .iter()
            .filter(|line| {
                line.contains("language=en-US")
                    && line.contains(&format!("key={key}"))
                    && line.contains("fallback=using missing translation fallback")
            })
            .count();

        assert_eq!(zh_cn_logs, 1);
        assert_eq!(en_us_logs, 1);
    }
}
