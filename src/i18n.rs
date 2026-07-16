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
    ("settings.bindings", "Key binding list"),
    ("settings.binding.source", "Input combo"),
    ("settings.binding.action_kind", "Action type"),
    ("settings.binding.action_value", "Action value"),
    ("settings.binding.action_count", "Count"),
    ("settings.binding.add", "Add"),
    ("settings.binding.update", "Update"),
    ("settings.binding.delete", "Delete"),
    ("settings.binding.listen_source", "Listen input"),
    ("settings.binding.ini_preview", "INI preview"),
    ("settings.binding.preview_error", "Preview error: "),
    ("settings.binding.listen_target", "Listen target"),
    ("settings.binding.listen_cancel", "Cancel"),
    ("settings.binding.type.builtin", "Built-in function"),
    ("settings.binding.type.key_tap", "Output key"),
    ("settings.binding.type.key_combo", "Output combo"),
    ("settings.binding.status.normal", "Normal"),
    (
        "settings.binding.status.duplicate_mapping",
        "Duplicate mapping",
    ),
    (
        "settings.binding.status.invalid_input_combo",
        "Invalid input combo",
    ),
    ("settings.binding.status.unknown_action", "Unknown action"),
    ("settings.binding.status.config_error", "Config error"),
    ("settings.binding_added", "Binding added to list."),
    ("settings.binding_updated", "Binding updated in list."),
    ("settings.binding_deleted", "Binding deleted from list."),
    ("settings.binding_failed", "Binding update failed."),
    (
        "settings.capture_source_started",
        "Listening for a CapsLock combo...",
    ),
    (
        "settings.capture_target_started",
        "Listening for a target combo...",
    ),
    ("settings.capture_cancelled", "Key listening cancelled."),
    ("settings.capture_timeout", "Key listening timed out."),
    ("settings.capture_source_saved", "Input combo captured."),
    ("settings.capture_target_saved", "Target combo captured."),
    (
        "settings.capture_missing_caps",
        "Input combo must include CapsLock.",
    ),
    ("settings.capture_unsupported_key", "Unsupported key."),
    ("settings.capture_invalid_combo", "Invalid combo."),
    ("settings.saved", "Saved and reloaded."),
    (
        "settings.saved_with_warnings",
        "Saved and reloaded with warnings.",
    ),
    ("settings.save_failed", "Save failed."),
    (
        "settings.validation_blocked",
        "Validation failed. Save was blocked.",
    ),
    (
        "settings.validation_warnings",
        "Configuration validation warnings:",
    ),
    ("settings.validation.severity.error", "Error"),
    ("settings.validation.severity.warning", "Warning"),
    ("settings.validation.issue.syntax", "INI syntax issue"),
    ("settings.validation.issue.invalid_value", "Invalid value"),
    (
        "settings.validation.issue.invalid_mapping",
        "Invalid input combo",
    ),
    (
        "settings.validation.issue.duplicate_mapping",
        "Duplicate mapping",
    ),
    ("settings.validation.issue.unknown_action", "Unknown action"),
    ("settings.validation.line", "line"),
    (
        "error.relaunch_as_admin_failed",
        "Failed to relaunch as admin:",
    ),
    ("error.reload_failed", "Reload failed:"),
    ("error.save_config_failed", "Failed to save config:"),
    ("error.save_settings_failed", "Failed to save settings:"),
    (
        "error.update_binding_failed",
        "Failed to update key binding:",
    ),
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
    ("settings.bindings", "按键绑定列表"),
    ("settings.binding.source", "输入组合键"),
    ("settings.binding.action_kind", "动作类型"),
    ("settings.binding.action_value", "动作内容"),
    ("settings.binding.action_count", "次数"),
    ("settings.binding.add", "新增"),
    ("settings.binding.update", "更新"),
    ("settings.binding.delete", "删除"),
    ("settings.binding.listen_source", "监听输入"),
    ("settings.binding.ini_preview", "INI 预览"),
    ("settings.binding.preview_error", "预览错误："),
    ("settings.binding.listen_target", "监听目标"),
    ("settings.binding.listen_cancel", "取消"),
    ("settings.binding.type.builtin", "内置函数"),
    ("settings.binding.type.key_tap", "输出单键"),
    ("settings.binding.type.key_combo", "输出组合键"),
    ("settings.binding.status.normal", "正常"),
    ("settings.binding.status.duplicate_mapping", "重复映射"),
    (
        "settings.binding.status.invalid_input_combo",
        "非法输入组合",
    ),
    ("settings.binding.status.unknown_action", "未知动作"),
    ("settings.binding.status.config_error", "配置错误"),
    ("settings.binding_added", "已新增到列表。"),
    ("settings.binding_updated", "已更新列表项。"),
    ("settings.binding_deleted", "已从列表删除。"),
    ("settings.binding_failed", "按键绑定更新失败。"),
    (
        "settings.capture_source_started",
        "请按 CapsLock 开头的输入组合键...",
    ),
    ("settings.capture_target_started", "请按目标输出组合键..."),
    ("settings.capture_cancelled", "已取消按键监听。"),
    ("settings.capture_timeout", "按键监听已超时。"),
    ("settings.capture_source_saved", "已录入输入组合键。"),
    ("settings.capture_target_saved", "已录入目标组合键。"),
    (
        "settings.capture_missing_caps",
        "输入组合键必须包含 CapsLock。",
    ),
    ("settings.capture_unsupported_key", "暂不支持该按键。"),
    ("settings.capture_invalid_combo", "组合键无效。"),
    ("settings.saved", "已保存并重新加载。"),
    (
        "settings.saved_with_warnings",
        "已保存并重新加载，但存在警告。",
    ),
    ("settings.save_failed", "保存失败。"),
    ("settings.validation_blocked", "校验失败，已阻止保存。"),
    ("settings.validation_warnings", "配置校验警告："),
    ("settings.validation.severity.error", "错误"),
    ("settings.validation.severity.warning", "警告"),
    ("settings.validation.issue.syntax", "INI 语法问题"),
    ("settings.validation.issue.invalid_value", "非法配置值"),
    ("settings.validation.issue.invalid_mapping", "非法输入组合"),
    ("settings.validation.issue.duplicate_mapping", "重复映射"),
    ("settings.validation.issue.unknown_action", "未知动作"),
    ("settings.validation.line", "行"),
    (
        "error.relaunch_as_admin_failed",
        "以管理员身份重新启动失败：",
    ),
    ("error.reload_failed", "重新加载失败："),
    ("error.save_config_failed", "保存配置失败："),
    ("error.save_settings_failed", "保存设置失败："),
    ("error.update_binding_failed", "更新按键绑定失败："),
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
    fn binding_status_translation_keys_are_localized() {
        assert_eq!(
            text(Language::ZhCn, "settings.binding.status.normal"),
            "正常"
        );
        assert_eq!(
            text(Language::EnUs, "settings.binding.status.config_error"),
            "Config error"
        );
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
