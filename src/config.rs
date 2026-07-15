use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

use crate::keys::{
    parse_capslock_combo_name, parse_combo_suffix, parse_key_code, KeyCode, KeyCombo,
};
use crate::logging;

static SAVE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct ConfigParseResult {
    pub config: Config,
    pub validation: ConfigValidation,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConfigValidation {
    pub issues: Vec<ConfigIssue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigIssue {
    pub severity: ConfigIssueSeverity,
    pub kind: ConfigIssueKind,
    pub line: Option<usize>,
    pub section: Option<String>,
    pub key: Option<String>,
    pub value: Option<String>,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigIssueSeverity {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigIssueKind {
    Syntax,
    InvalidValue,
    InvalidMapping,
    DuplicateMapping,
    UnknownAction,
}
#[derive(Clone, Debug)]
pub struct Config {
    pub general: GeneralConfig,
    pub capslock_layer: Vec<KeyMapping>,
    pub ui: UiConfig,
}

#[derive(Clone, Debug)]
pub struct GeneralConfig {
    pub enabled: bool,
    pub start_with_windows: bool,
    pub run_as_admin: bool,
    pub show_tray_icon: bool,
    pub tap_capslock: TapCapsLock,
}

#[derive(Clone, Debug)]
pub struct UiConfig {
    pub settings_backend: String,
    pub settings_page: String,
    pub language: Language,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    System,
    ZhCn,
    EnUs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltInAction {
    MoveLeft(u32),
    MoveDown(u32),
    MoveUp(u32),
    MoveRight(u32),
    MoveWordLeft(u32),
    MoveWordRight(u32),
    SelectLeft(u32),
    SelectRight(u32),
    SelectUp(u32),
    SelectDown(u32),
    SelectWordLeft(u32),
    SelectWordRight(u32),
    Home(u32),
    End(u32),
    PageUp(u32),
    PageDown(u32),
    Enter(u32),
    Backspace(u32),
    Delete(u32),
    DeleteWord(u32),
    ForwardDeleteWord(u32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LayerAction {
    BuiltIn(BuiltInAction),
    KeyTap(KeyCode),
    KeyCombo(KeyCombo),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TapCapsLock {
    Toggle,
    Escape,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyMapping {
    pub source: KeyCombo,
    pub action: LayerAction,
}

#[derive(Debug)]
pub struct ConfigPaths {
    pub config_path: PathBuf,
    pub log_path: PathBuf,
}

#[derive(Debug)]
struct KeysSectionConfig {
    mappings: Vec<KeyMapping>,
    tap_capslock: Option<TapCapsLock>,
}

#[derive(Debug, Default)]
struct IniDocument {
    sections: BTreeMap<String, IniSection>,
}

#[derive(Debug, Default)]
struct IniSection {
    entries: Vec<IniEntry>,
    values: BTreeMap<String, IniValue>,
}

#[derive(Clone, Debug)]
struct IniEntry {
    key: String,
    value: String,
    line_number: usize,
}

#[derive(Clone, Debug)]
struct IniValue {
    value: String,
    line_number: usize,
}

#[derive(Debug)]
struct IniParseError {
    line_number: Option<usize>,
    message: String,
}

impl IniDocument {
    fn get(&self, section: &str) -> Option<&IniSection> {
        self.sections.get(section)
    }
}

impl ConfigParseResult {
    pub fn into_config_result(self) -> Result<Config, String> {
        if self.validation.has_errors() {
            Err(self.validation.error_summary())
        } else {
            Ok(self.config)
        }
    }
}

impl ConfigValidation {
    pub fn has_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == ConfigIssueSeverity::Error)
    }

    #[allow(dead_code)]
    pub fn has_warnings(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == ConfigIssueSeverity::Warning)
    }

    pub fn errors(&self) -> impl Iterator<Item = &ConfigIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == ConfigIssueSeverity::Error)
    }

    #[allow(dead_code)]
    pub fn warnings(&self) -> impl Iterator<Item = &ConfigIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == ConfigIssueSeverity::Warning)
    }

    fn error(
        &mut self,
        kind: ConfigIssueKind,
        line: Option<usize>,
        section: Option<&str>,
        key: Option<&str>,
        value: Option<&str>,
        message: impl Into<String>,
    ) {
        self.push(ConfigIssue::new(
            ConfigIssueSeverity::Error,
            kind,
            line,
            section,
            key,
            value,
            message,
        ));
    }

    fn warning(
        &mut self,
        kind: ConfigIssueKind,
        line: Option<usize>,
        section: Option<&str>,
        key: Option<&str>,
        value: Option<&str>,
        message: impl Into<String>,
    ) {
        self.push(ConfigIssue::new(
            ConfigIssueSeverity::Warning,
            kind,
            line,
            section,
            key,
            value,
            message,
        ));
    }

    fn push(&mut self, issue: ConfigIssue) {
        self.issues.push(issue);
    }

    fn error_summary(&self) -> String {
        let messages: Vec<String> = self.errors().map(ConfigIssue::summary).collect();
        if messages.is_empty() {
            "config validation failed".to_string()
        } else {
            messages.join("; ")
        }
    }
}

impl ConfigIssue {
    fn new(
        severity: ConfigIssueSeverity,
        kind: ConfigIssueKind,
        line: Option<usize>,
        section: Option<&str>,
        key: Option<&str>,
        value: Option<&str>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            kind,
            line,
            section: section.map(ToOwned::to_owned),
            key: key.map(ToOwned::to_owned),
            value: value.map(ToOwned::to_owned),
            message: message.into(),
        }
    }

    fn summary(&self) -> String {
        match self.line {
            Some(line) => format!("line {line}: {}", self.message),
            None => self.message.clone(),
        }
    }
}

impl ConfigPaths {
    pub fn resolve() -> Result<Self, String> {
        let config_path = if let Ok(path) = env::var("CAPSLOCK_RS_CONFIG") {
            PathBuf::from(path)
        } else {
            let current_dir = env::current_dir()
                .map_err(|error| format!("failed to get current directory: {error}"))?;
            let current_config = current_dir.join("capslock_rs.ini");
            if current_config.exists() {
                current_config
            } else {
                let exe_dir = current_exe_dir()?;
                let exe_config = exe_dir.join("capslock_rs.ini");
                if exe_config.exists() {
                    exe_config
                } else if current_dir.join("Cargo.toml").exists() {
                    current_config
                } else {
                    exe_config
                }
            }
        };

        let log_path = config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("capslock_rs.log");

        Ok(Self {
            config_path,
            log_path,
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                enabled: true,
                start_with_windows: false,
                run_as_admin: false,
                show_tray_icon: true,
                tap_capslock: TapCapsLock::Toggle,
            },
            capslock_layer: vec![
                KeyMapping::new("h", LayerAction::builtin(BuiltInAction::MoveLeft(1))),
                KeyMapping::new("j", LayerAction::builtin(BuiltInAction::MoveDown(1))),
                KeyMapping::new("k", LayerAction::builtin(BuiltInAction::MoveUp(1))),
                KeyMapping::new("l", LayerAction::builtin(BuiltInAction::MoveRight(1))),
                KeyMapping::new("space", LayerAction::builtin(BuiltInAction::Enter(1))),
                KeyMapping::new("q", LayerAction::builtin(BuiltInAction::Backspace(1))),
                KeyMapping::new("e", LayerAction::builtin(BuiltInAction::Delete(1))),
                KeyMapping::new("z", LayerAction::builtin(BuiltInAction::MoveUp(5))),
                KeyMapping::new("x", LayerAction::builtin(BuiltInAction::MoveDown(5))),
                KeyMapping::new(
                    "lalt_a",
                    LayerAction::builtin(BuiltInAction::MoveWordLeft(1)),
                ),
                KeyMapping::new(
                    "lalt_d",
                    LayerAction::builtin(BuiltInAction::MoveWordRight(1)),
                ),
            ],
            ui: UiConfig {
                settings_backend: "ini".to_string(),
                settings_page: "future".to_string(),
                language: Language::System,
            },
        }
    }
}

impl Config {
    pub fn load_or_create(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            let config = Self::default();
            config.save(path)?;
            return Ok(config);
        }

        Self::load(path)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        Self::load_with_validation(path)?.into_config_result()
    }

    pub fn load_with_validation(path: &Path) -> Result<ConfigParseResult, String> {
        let content = read_config_text(path)?;
        Ok(Self::from_ini_with_validation(&content))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let parent = config_parent_dir(path);
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;

        let temp_path = temp_save_path(path);
        if let Err(error) = write_temp_config(&temp_path, &self.to_ini_string()) {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }

        if let Err(error) = replace_file(&temp_path, path) {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub fn from_ini(content: &str) -> Result<Self, String> {
        Self::from_ini_with_validation(content).into_config_result()
    }

    pub fn from_ini_with_validation(content: &str) -> ConfigParseResult {
        let mut validation = ConfigValidation::default();
        let parsed = match parse_ini(content) {
            Ok(parsed) => parsed,
            Err(error) => {
                validation.error(
                    ConfigIssueKind::Syntax,
                    error.line_number,
                    None,
                    None,
                    None,
                    error.message,
                );
                return ConfigParseResult {
                    config: Self::default(),
                    validation,
                };
            }
        };
        let mut config = Self::default();

        if let Some(global) = parsed.get("global") {
            config.general.start_with_windows = read_bool(
                global,
                "global",
                "autostart",
                config.general.start_with_windows,
                &mut validation,
            );
        }

        if let Some(general) = parsed.get("general") {
            config.general.enabled = read_bool(
                general,
                "general",
                "enabled",
                config.general.enabled,
                &mut validation,
            );
            config.general.start_with_windows = read_bool(
                general,
                "general",
                "start_with_windows",
                config.general.start_with_windows,
                &mut validation,
            );
            config.general.run_as_admin = read_bool(
                general,
                "general",
                "run_as_admin",
                config.general.run_as_admin,
                &mut validation,
            );
            config.general.show_tray_icon = read_bool(
                general,
                "general",
                "show_tray_icon",
                config.general.show_tray_icon,
                &mut validation,
            );
            if let Some(value) = general.values.get("tap_capslock") {
                match parse_tap_capslock(&value.value) {
                    Ok(tap_capslock) => config.general.tap_capslock = tap_capslock,
                    Err(error) => validation.error(
                        ConfigIssueKind::InvalidValue,
                        Some(value.line_number),
                        Some("general"),
                        Some("tap_capslock"),
                        Some(&value.value),
                        error,
                    ),
                }
            }
        }

        if let Some(layer) = parsed.get("layer.capslock") {
            config.capslock_layer = parse_layer_section(layer, &mut validation);
        }

        if let Some(keys) = parse_capslock_plus_keys_section(content, &mut validation) {
            if let Some(tap_capslock) = keys.tap_capslock {
                config.general.tap_capslock = tap_capslock;
            }
            config.capslock_layer = keys.mappings;
        }

        if let Some(ui) = parsed.get("ui") {
            if let Some(value) = ui.values.get("settings_backend") {
                config.ui.settings_backend = value.value.clone();
            }
            if let Some(value) = ui.values.get("settings_page") {
                config.ui.settings_page = value.value.clone();
            }
            if let Some(value) = ui.values.get("language") {
                match Language::parse(&value.value) {
                    Ok(language) => config.ui.language = language,
                    Err(error) => validation.error(
                        ConfigIssueKind::InvalidValue,
                        Some(value.line_number),
                        Some("ui"),
                        Some("language"),
                        Some(&value.value),
                        error,
                    ),
                }
            }
        }

        ConfigParseResult { config, validation }
    }

    pub fn to_ini_string(&self) -> String {
        let mut output = String::new();
        output.push_str("; CapsLock RS configuration.\n");
        output.push_str("; [Keys] supports keyFunc_*, keyTarget_* and keyCombo_* actions.\n\n");
        output.push_str("[general]\n");
        output.push_str(&format!("enabled = {}\n", bool_text(self.general.enabled)));
        output.push_str(&format!(
            "start_with_windows = {}\n",
            bool_text(self.general.start_with_windows)
        ));
        output.push_str(&format!(
            "run_as_admin = {}\n",
            bool_text(self.general.run_as_admin)
        ));
        output.push_str(&format!(
            "show_tray_icon = {}\n",
            bool_text(self.general.show_tray_icon)
        ));
        output.push_str(&format!(
            "tap_capslock = {}\n\n",
            self.general.tap_capslock.as_str()
        ));

        output.push_str("[Keys]\n");
        for mapping in &self.capslock_layer {
            output.push_str(&format!(
                "{}={}\n",
                mapping.source.capslock_ini_key(),
                mapping.action.as_ini_value()
            ));
        }

        output.push_str("\n[ui]\n");
        output.push_str(&format!(
            "language = {}\n",
            self.ui.language.as_config_value()
        ));
        output.push_str(&format!(
            "settings_backend = {}\n",
            self.ui.settings_backend
        ));
        output.push_str(&format!("settings_page = {}\n", self.ui.settings_page));
        output
    }
}

impl BuiltInAction {
    pub fn as_key_func(self) -> String {
        match self {
            BuiltInAction::MoveLeft(count) => key_func_with_count("moveLeft", count),
            BuiltInAction::MoveDown(count) => key_func_with_count("moveDown", count),
            BuiltInAction::MoveUp(count) => key_func_with_count("moveUp", count),
            BuiltInAction::MoveRight(count) => key_func_with_count("moveRight", count),
            BuiltInAction::MoveWordLeft(count) => key_func_with_count("moveWordLeft", count),
            BuiltInAction::MoveWordRight(count) => key_func_with_count("moveWordRight", count),
            BuiltInAction::SelectLeft(count) => key_func_with_count("selectLeft", count),
            BuiltInAction::SelectRight(count) => key_func_with_count("selectRight", count),
            BuiltInAction::SelectUp(count) => key_func_with_count("selectUp", count),
            BuiltInAction::SelectDown(count) => key_func_with_count("selectDown", count),
            BuiltInAction::SelectWordLeft(count) => key_func_with_count("selectWordLeft", count),
            BuiltInAction::SelectWordRight(count) => key_func_with_count("selectWordRight", count),
            BuiltInAction::Home(count) => key_func_with_count("home", count),
            BuiltInAction::End(count) => key_func_with_count("end", count),
            BuiltInAction::PageUp(count) => key_func_with_count("pageUp", count),
            BuiltInAction::PageDown(count) => key_func_with_count("pageDown", count),
            BuiltInAction::Enter(count) => key_func_with_count("enter", count),
            BuiltInAction::Backspace(count) => key_func_with_count("backspace", count),
            BuiltInAction::Delete(count) => key_func_with_count("delete", count),
            BuiltInAction::DeleteWord(count) => key_func_with_count("deleteWord", count),
            BuiltInAction::ForwardDeleteWord(count) => {
                key_func_with_count("forwardDeleteWord", count)
            }
        }
    }
}

impl LayerAction {
    pub fn builtin(action: BuiltInAction) -> Self {
        Self::BuiltIn(action)
    }

    pub fn as_ini_value(&self) -> String {
        match self {
            LayerAction::BuiltIn(action) => action.as_key_func(),
            LayerAction::KeyTap(key) => format!("keyTarget_{}", key.name),
            LayerAction::KeyCombo(combo) => format!("keyCombo_{}", combo.ini_suffix()),
        }
    }
}

impl Default for Language {
    fn default() -> Self {
        Self::System
    }
}

impl Language {
    pub fn parse(value: &str) -> Result<Self, String> {
        let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
        match normalized.as_str() {
            "system" => Ok(Self::System),
            "zh-cn" => Ok(Self::ZhCn),
            "en-us" => Ok(Self::EnUs),
            _ => Err(format!("invalid language value: {value}")),
        }
    }

    pub fn as_config_value(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::ZhCn => "zh-CN",
            Self::EnUs => "en-US",
        }
    }
}

impl TapCapsLock {
    pub fn as_str(self) -> &'static str {
        match self {
            TapCapsLock::Toggle => "toggle",
            TapCapsLock::Escape => "escape",
            TapCapsLock::None => "none",
        }
    }
}

impl KeyMapping {
    fn new(source: &str, action: LayerAction) -> Self {
        Self {
            source: parse_combo_suffix(source).expect("default source combo must be valid"),
            action,
        }
    }
}

fn read_config_text(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(bytes[3..].to_vec())
            .map_err(|error| format!("failed to decode {} as UTF-8: {error}", path.display()));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_bytes(path, &bytes[2..], false);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_bytes(path, &bytes[2..], true);
    }

    String::from_utf8(bytes)
        .map_err(|error| format!("failed to decode {} as UTF-8: {error}", path.display()))
}

fn decode_utf16_bytes(path: &Path, bytes: &[u8], big_endian: bool) -> Result<String, String> {
    if bytes.len() % 2 != 0 {
        return Err(format!("invalid UTF-16 byte length in {}", path.display()));
    }

    let code_units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if big_endian {
                u16::from_be_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_le_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();

    String::from_utf16(&code_units)
        .map_err(|error| format!("failed to decode {} as UTF-16: {error}", path.display()))
}

fn parse_ini(content: &str) -> Result<IniDocument, IniParseError> {
    let mut document = IniDocument::default();
    let mut current_section = String::new();

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') {
            if !line.ends_with(']') {
                return Err(IniParseError {
                    line_number: Some(line_number),
                    message: format!("invalid section header at line {line_number}"),
                });
            }

            current_section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            document
                .sections
                .entry(current_section.clone())
                .or_default();
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(IniParseError {
                line_number: Some(line_number),
                message: format!("invalid key/value at line {line_number}"),
            });
        };

        let key = key.trim().to_ascii_lowercase();
        let value = strip_inline_comment(value.trim()).trim().to_string();
        let section = document
            .sections
            .entry(current_section.clone())
            .or_default();
        section.entries.push(IniEntry {
            key: key.clone(),
            value: value.clone(),
            line_number,
        });
        section.values.insert(key, IniValue { value, line_number });
    }

    Ok(document)
}

fn parse_layer_section(layer: &IniSection, validation: &mut ConfigValidation) -> Vec<KeyMapping> {
    let mut mappings = Vec::new();
    let mut seen_raw_keys = BTreeSet::new();
    let mut seen_sources = BTreeSet::new();

    for entry in &layer.entries {
        if !seen_raw_keys.insert(entry.key.clone()) {
            warn_duplicate_mapping(
                validation,
                entry.line_number,
                "layer.capslock",
                &entry.key,
                &entry.value,
                format!("duplicate [layer.capslock] mapping for {}", entry.key),
            );
            continue;
        }

        let source = match parse_combo_suffix(&entry.key) {
            Ok(source) => source,
            Err(error) => {
                warn_invalid_mapping(
                    validation,
                    entry.line_number,
                    "layer.capslock",
                    &entry.key,
                    &entry.value,
                    format!("invalid [layer.capslock] mapping {}: {error}", entry.key),
                );
                continue;
            }
        };
        let action = match parse_layer_action(&entry.value) {
            Ok(Some(action)) => action,
            Ok(None) => continue,
            Err(error) => {
                warn_unknown_action(
                    validation,
                    entry.line_number,
                    "layer.capslock",
                    &entry.key,
                    &entry.value,
                    format!(
                        "unknown action for [layer.capslock] mapping {}: {error}",
                        source.ini_suffix()
                    ),
                );
                continue;
            }
        };

        push_mapping(
            &mut mappings,
            &mut seen_sources,
            source,
            action,
            validation,
            entry.line_number,
            "layer.capslock",
            &entry.key,
            &entry.value,
        );
    }

    mappings
}

fn parse_capslock_plus_keys_section(
    content: &str,
    validation: &mut ConfigValidation,
) -> Option<KeysSectionConfig> {
    let mut current_section = String::new();
    let mut mappings = Vec::new();
    let mut tap_capslock = None;
    let mut seen_raw_keys = BTreeSet::new();
    let mut seen_sources = BTreeSet::new();
    let mut found_keys_section = false;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') {
            if !line.ends_with(']') {
                validation.error(
                    ConfigIssueKind::Syntax,
                    Some(line_number),
                    None,
                    None,
                    None,
                    format!("invalid section header at line {line_number}"),
                );
                return None;
            }
            current_section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            continue;
        }

        if current_section != "keys" {
            continue;
        }

        found_keys_section = true;
        let Some((key, value)) = line.split_once('=') else {
            validation.error(
                ConfigIssueKind::Syntax,
                Some(line_number),
                Some("keys"),
                None,
                None,
                format!("invalid key/value at line {line_number}"),
            );
            return None;
        };

        let key = key.trim().to_ascii_lowercase();
        let value = strip_inline_comment(value.trim()).trim().to_string();

        // Copied CapsLock+ configs often keep a later doNothing entry for the same key.
        if !seen_raw_keys.insert(key.clone()) {
            warn_duplicate_mapping(
                validation,
                line_number,
                "keys",
                &key,
                &value,
                format!("duplicate [Keys] mapping for {key}"),
            );
            continue;
        }

        if key == "press_caps" {
            match parse_key_func_tap_capslock(&value) {
                Ok(mode) => tap_capslock = Some(mode),
                Err(error) => warn_unknown_action(
                    validation,
                    line_number,
                    "keys",
                    &key,
                    &value,
                    format!("unsupported press_caps action: {error}"),
                ),
            }
            continue;
        }

        if is_do_nothing_action(&value) {
            continue;
        }

        let source = match parse_capslock_combo_name(&key) {
            Ok(source) => source,
            Err(error) => {
                warn_invalid_mapping(
                    validation,
                    line_number,
                    "keys",
                    &key,
                    &value,
                    format!("invalid [Keys] mapping {key}: {error}"),
                );
                continue;
            }
        };
        let action = match parse_layer_action(&value) {
            Ok(Some(action)) => action,
            Ok(None) => continue,
            Err(error) => {
                warn_unknown_action(
                    validation,
                    line_number,
                    "keys",
                    &key,
                    &value,
                    format!("unknown action for [Keys] mapping {key}: {error}"),
                );
                continue;
            }
        };

        push_mapping(
            &mut mappings,
            &mut seen_sources,
            source,
            action,
            validation,
            line_number,
            "keys",
            &key,
            &value,
        );
    }

    found_keys_section.then_some(KeysSectionConfig {
        mappings,
        tap_capslock,
    })
}

fn push_mapping(
    mappings: &mut Vec<KeyMapping>,
    seen_sources: &mut BTreeSet<String>,
    source: KeyCombo,
    action: LayerAction,
    validation: &mut ConfigValidation,
    line_number: usize,
    section: &str,
    key: &str,
    value: &str,
) {
    let normalized_source = source.ini_suffix();
    if !seen_sources.insert(normalized_source.clone()) {
        warn_duplicate_mapping(
            validation,
            line_number,
            section,
            key,
            value,
            format!("duplicate mapping for caps_{normalized_source}"),
        );
        return;
    }

    mappings.push(KeyMapping { source, action });
}

fn warn_invalid_mapping(
    validation: &mut ConfigValidation,
    line_number: usize,
    section: &str,
    key: &str,
    value: &str,
    message: String,
) {
    logging::log_line(format!("skipping mapping at line {line_number}: {message}"));
    validation.warning(
        ConfigIssueKind::InvalidMapping,
        Some(line_number),
        Some(section),
        Some(key),
        Some(value),
        message,
    );
}

fn warn_duplicate_mapping(
    validation: &mut ConfigValidation,
    line_number: usize,
    section: &str,
    key: &str,
    value: &str,
    message: String,
) {
    logging::log_line(format!("skipping mapping at line {line_number}: {message}"));
    validation.warning(
        ConfigIssueKind::DuplicateMapping,
        Some(line_number),
        Some(section),
        Some(key),
        Some(value),
        message,
    );
}

fn warn_unknown_action(
    validation: &mut ConfigValidation,
    line_number: usize,
    section: &str,
    key: &str,
    value: &str,
    message: String,
) {
    logging::log_line(format!("skipping mapping at line {line_number}: {message}"));
    validation.warning(
        ConfigIssueKind::UnknownAction,
        Some(line_number),
        Some(section),
        Some(key),
        Some(value),
        message,
    );
}
fn strip_inline_comment(value: &str) -> &str {
    for marker in [" ;", " #"] {
        if let Some(index) = value.find(marker) {
            return &value[..index];
        }
    }

    value
}

fn read_bool(
    section: &IniSection,
    section_name: &str,
    key: &str,
    default_value: bool,
    validation: &mut ConfigValidation,
) -> bool {
    let Some(value) = section.values.get(key) else {
        return default_value;
    };

    match parse_bool(&value.value) {
        Some(parsed) => parsed,
        None => {
            validation.error(
                ConfigIssueKind::InvalidValue,
                Some(value.line_number),
                Some(section_name),
                Some(key),
                Some(&value.value),
                format!("invalid bool for {key}: {}", value.value),
            );
            default_value
        }
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn bool_text(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn parse_tap_capslock(value: &str) -> Result<TapCapsLock, String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "toggle" => Ok(TapCapsLock::Toggle),
        "escape" => Ok(TapCapsLock::Escape),
        "none" | "off" | "disabled" => Ok(TapCapsLock::None),
        _ => Err(format!("invalid tap_capslock value: {value}")),
    }
}

fn parse_key_func_tap_capslock(value: &str) -> Result<TapCapsLock, String> {
    let (name, _) = parse_key_func_call(value)?;
    match compact_name(&name).as_str() {
        "togglecapslock" => Ok(TapCapsLock::Toggle),
        "esc" | "escape" => Ok(TapCapsLock::Escape),
        "donothing" => Ok(TapCapsLock::None),
        _ => Err(format!("unsupported press_caps action: {value}")),
    }
}

fn parse_layer_action(value: &str) -> Result<Option<LayerAction>, String> {
    let value = value.trim();
    if is_do_nothing_action(value) {
        return Ok(None);
    }

    if let Some(key_name) = strip_ascii_case_prefix(value, "keytarget_") {
        let key = parse_key_code(key_name)
            .ok_or_else(|| format!("unsupported target key action: {value}"))?;
        return Ok(Some(LayerAction::KeyTap(key)));
    }

    if let Some(combo_name) = strip_ascii_case_prefix(value, "keycombo_") {
        let combo = parse_combo_suffix(combo_name)
            .map_err(|error| format!("unsupported target combo action {value}: {error}"))?;
        return Ok(Some(LayerAction::KeyCombo(combo)));
    }

    parse_builtin_action(value).map(|action| Some(LayerAction::BuiltIn(action)))
}

fn parse_builtin_action(value: &str) -> Result<BuiltInAction, String> {
    let (name, count) = parse_key_func_call(value)?;
    let count = count.unwrap_or(1).max(1);
    let action = match compact_name(&name).as_str() {
        "left" | "arrowleft" | "moveleft" => BuiltInAction::MoveLeft(count),
        "down" | "arrowdown" | "movedown" => BuiltInAction::MoveDown(count),
        "up" | "arrowup" | "moveup" => BuiltInAction::MoveUp(count),
        "right" | "arrowright" | "moveright" => BuiltInAction::MoveRight(count),
        "movewordleft" => BuiltInAction::MoveWordLeft(count),
        "movewordright" => BuiltInAction::MoveWordRight(count),
        "selectleft" => BuiltInAction::SelectLeft(count),
        "selectright" => BuiltInAction::SelectRight(count),
        "selectup" => BuiltInAction::SelectUp(count),
        "selectdown" => BuiltInAction::SelectDown(count),
        "selectwordleft" => BuiltInAction::SelectWordLeft(count),
        "selectwordright" => BuiltInAction::SelectWordRight(count),
        "home" => BuiltInAction::Home(count),
        "end" => BuiltInAction::End(count),
        "pageup" => BuiltInAction::PageUp(count),
        "pagedown" => BuiltInAction::PageDown(count),
        "enter" | "return" => BuiltInAction::Enter(count),
        "backspace" | "back" | "bs" => BuiltInAction::Backspace(count),
        "delete" | "del" => BuiltInAction::Delete(count),
        "deleteword" => BuiltInAction::DeleteWord(count),
        "forwarddeleteword" => BuiltInAction::ForwardDeleteWord(count),
        _ => return Err(format!("unsupported key action: {value}")),
    };

    Ok(action)
}

fn parse_key_func_call(value: &str) -> Result<(String, Option<u32>), String> {
    let value = value.trim();
    let value = strip_ascii_case_prefix(value, "keyfunc_").unwrap_or(value);

    let Some(open_index) = value.find('(') else {
        return Ok((value.to_string(), None));
    };

    if !value.ends_with(')') {
        return Err(format!("invalid key function call: {value}"));
    }

    let name = value[..open_index].trim().to_string();
    let raw_count = value[open_index + 1..value.len() - 1].trim();
    if raw_count.is_empty() {
        return Ok((name, Some(1)));
    }

    let count = raw_count
        .parse::<u32>()
        .map_err(|error| format!("invalid key function count in {value}: {error}"))?;
    Ok((name, Some(count)))
}

fn compact_name(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-', ' '], "")
}

fn is_do_nothing_action(value: &str) -> bool {
    let value = value.trim();
    let value = strip_ascii_case_prefix(value, "keyfunc_").unwrap_or(value);
    matches!(
        compact_name(value).as_str(),
        "donothing" | "none" | "off" | "disabled"
    )
}

fn strip_ascii_case_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    if value.len() < prefix.len() {
        return None;
    }

    let (candidate, rest) = value.split_at(prefix.len());
    candidate.eq_ignore_ascii_case(prefix).then_some(rest)
}
fn key_func_with_count(name: &str, count: u32) -> String {
    if count <= 1 {
        format!("keyFunc_{name}")
    } else {
        format!("keyFunc_{name}({count})")
    }
}

fn config_parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn temp_save_path(path: &Path) -> PathBuf {
    let parent = config_parent_dir(path);
    let file_name = path
        .file_name()
        .unwrap_or_else(|| OsStr::new("capslock_rs.ini"));
    let counter = SAVE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);

    let mut temp_name = OsString::from(".");
    temp_name.push(file_name);
    temp_name.push(format!(".tmp.{}.{}", std::process::id(), counter));
    parent.join(temp_name)
}

fn write_temp_config(path: &Path, content: &str) -> Result<(), String> {
    let mut file = File::create(path)
        .map_err(|error| format!("failed to create temp config {}: {error}", path.display()))?;
    file.write_all(content.as_bytes())
        .map_err(|error| format!("failed to write temp config {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("failed to flush temp config {}: {error}", path.display()))?;
    Ok(())
}

fn replace_file(source: &Path, target: &Path) -> Result<(), String> {
    let source_wide = path_to_wide_null(source);
    let target_wide = path_to_wide_null(target);
    let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
    let replaced = unsafe { MoveFileExW(source_wide.as_ptr(), target_wide.as_ptr(), flags) };

    if replaced == 0 {
        return Err(format!(
            "failed to replace {} with {}: {}",
            target.display(),
            source.display(),
            std::io::Error::last_os_error()
        ));
    }

    Ok(())
}

fn path_to_wide_null(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn current_exe_dir() -> Result<PathBuf, String> {
    let exe = env::current_exe().map_err(|error| format!("failed to get exe path: {error}"))?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("failed to get parent directory for {}", exe.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::keys::{KeyModifier, VK_F1, VK_HOME, VK_NEXT};

    #[test]
    fn parses_capslock_layer_actions() {
        let config = Config::from_ini(
            r#"
            [general]
            enabled = true
            start_with_windows = false
            run_as_admin = true

            [layer.capslock]
            h = left
            j = down
            k = up
            l = right
            space = enter
            q = backspace
            e = delete
            "#,
        )
        .unwrap();

        assert!(config.general.enabled);
        assert!(config.general.run_as_admin);
        assert_eq!(config.capslock_layer.len(), 7);
        assert_eq!(
            find_action(&config, "h"),
            Some(LayerAction::BuiltIn(BuiltInAction::MoveLeft(1)))
        );
        assert_eq!(
            find_action(&config, "space"),
            Some(LayerAction::BuiltIn(BuiltInAction::Enter(1)))
        );
        assert_eq!(
            find_action(&config, "q"),
            Some(LayerAction::BuiltIn(BuiltInAction::Backspace(1)))
        );
        assert_eq!(
            find_action(&config, "e"),
            Some(LayerAction::BuiltIn(BuiltInAction::Delete(1)))
        );
    }

    #[test]
    fn parses_capslock_plus_key_func_actions() {
        let config = Config::from_ini(
            r#"
            [Global]
            autostart=1

            [Keys]
            caps_q=keyFunc_backspace
            caps_e=keyFunc_delete
            caps_w=keyFunc_moveUp(1)
            caps_a=keyFunc_moveLeft(1)
            caps_s=keyFunc_moveDown(1)
            caps_d=keyFunc_moveRight(1)
            caps_z=keyFunc_moveUp(5)
            caps_x=keyFunc_moveDown(5)
            caps_lalt_a=keyFunc_moveWordLeft
            caps_lalt_d=keyFunc_moveWordRight(2)
            caps_w=keyFunc_doNothing
            "#,
        )
        .unwrap();

        assert!(config.general.start_with_windows);
        assert_eq!(
            find_action(&config, "q"),
            Some(LayerAction::BuiltIn(BuiltInAction::Backspace(1)))
        );
        assert_eq!(
            find_action(&config, "z"),
            Some(LayerAction::BuiltIn(BuiltInAction::MoveUp(5)))
        );
        assert_eq!(
            find_action(&config, "x"),
            Some(LayerAction::BuiltIn(BuiltInAction::MoveDown(5)))
        );
        assert_eq!(
            find_action(&config, "w"),
            Some(LayerAction::BuiltIn(BuiltInAction::MoveUp(1)))
        );
        assert_eq!(
            find_action(&config, "lalt_a"),
            Some(LayerAction::BuiltIn(BuiltInAction::MoveWordLeft(1)))
        );
        assert_eq!(
            find_action(&config, "lalt_d"),
            Some(LayerAction::BuiltIn(BuiltInAction::MoveWordRight(2)))
        );
    }

    #[test]
    fn parses_key_target_and_combo_actions() {
        let config = Config::from_ini(
            r#"
            [Keys]
            caps_r=keyTarget_f5
            caps_c=keyCombo_ctrl_c
            caps_lalt_shift_j=keyCombo_ctrl_shift_left
            caps_u=keyFunc_selectUp(5)
            caps_p=keyFunc_pageDown
            "#,
        )
        .unwrap();

        assert_eq!(
            find_action(&config, "r"),
            Some(LayerAction::KeyTap(parse_key_code("f5").unwrap()))
        );
        assert_eq!(
            find_action(&config, "c"),
            Some(LayerAction::KeyCombo(parse_combo_suffix("ctrl_c").unwrap()))
        );
        assert_eq!(
            find_action(&config, "lalt_shift_j"),
            Some(LayerAction::KeyCombo(
                parse_combo_suffix("ctrl_shift_left").unwrap()
            ))
        );
        assert_eq!(
            find_action(&config, "u"),
            Some(LayerAction::BuiltIn(BuiltInAction::SelectUp(5)))
        );
        assert_eq!(
            find_action(&config, "p"),
            Some(LayerAction::BuiltIn(BuiltInAction::PageDown(1)))
        );
    }

    #[test]
    fn parses_ui_language_values() {
        let zh_cn = Config::from_ini(
            r#"
            [ui]
            language = zh-CN
            "#,
        )
        .unwrap();
        let en_us = Config::from_ini(
            r#"
            [ui]
            language = en_us
            "#,
        )
        .unwrap();
        let system = Config::from_ini(
            r#"
            [ui]
            language = system
            "#,
        )
        .unwrap();

        assert_eq!(zh_cn.ui.language, Language::ZhCn);
        assert_eq!(en_us.ui.language, Language::EnUs);
        assert_eq!(system.ui.language, Language::System);
        assert!(Config::from_ini("[ui]\nlanguage = fr-FR\n").is_err());
    }

    #[test]
    fn serializes_ui_language_values() {
        let default_ini = Config::default().to_ini_string();
        assert!(default_ini.contains("language = system"));

        let mut config = Config::default();
        config.ui.language = Language::ZhCn;
        assert!(config.to_ini_string().contains("language = zh-CN"));

        config.ui.language = Language::EnUs;
        assert!(config.to_ini_string().contains("language = en-US"));
    }
    #[test]
    fn serializes_normalized_modifiers_and_action_types() {
        let config = Config {
            capslock_layer: vec![
                KeyMapping {
                    source: parse_combo_suffix("shift_ctrl_h").unwrap(),
                    action: LayerAction::KeyTap(parse_key_code("home").unwrap()),
                },
                KeyMapping {
                    source: parse_combo_suffix("lalt_j").unwrap(),
                    action: LayerAction::KeyCombo(
                        parse_combo_suffix("shift_ctrl_pageDown").unwrap(),
                    ),
                },
            ],
            ..Config::default()
        };

        let ini = config.to_ini_string();

        assert!(ini.contains("caps_ctrl_shift_h=keyTarget_home"));
        assert!(ini.contains("caps_lalt_j=keyCombo_ctrl_shift_page_down"));
    }

    #[test]
    fn skips_unknown_mapping_without_failing_config() {
        let config = Config::from_ini(
            r#"
            [Keys]
            caps_h=keyFunc_moveLeft
            caps_badkey=keyFunc_moveRight
            caps_j=keyFunc_noSuchAction
            "#,
        )
        .unwrap();

        assert_eq!(config.capslock_layer.len(), 1);
        assert_eq!(
            find_action(&config, "h"),
            Some(LayerAction::BuiltIn(BuiltInAction::MoveLeft(1)))
        );
    }

    #[test]
    fn loads_utf16_capslock_plus_ini() {
        let path =
            std::env::temp_dir().join(format!("capslock_rs_utf16_test_{}.ini", std::process::id()));
        let content = "[Keys]\ncaps_z=keyFunc_moveUp(5)\n";
        let mut bytes = vec![0xFF, 0xFE];
        for code_unit in content.encode_utf16() {
            bytes.extend_from_slice(&code_unit.to_le_bytes());
        }

        fs::write(&path, bytes).unwrap();
        let config = Config::load(&path).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(
            find_action(&config, "z"),
            Some(LayerAction::BuiltIn(BuiltInAction::MoveUp(5)))
        );
    }

    #[test]
    fn loads_utf8_bom_capslock_plus_ini() {
        let path = std::env::temp_dir().join(format!(
            "capslock_rs_utf8_bom_test_{}.ini",
            std::process::id()
        ));
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"[Keys]\ncaps_r=keyTarget_f5\n");

        fs::write(&path, bytes).unwrap();
        let config = Config::load(&path).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(
            find_action(&config, "r"),
            Some(LayerAction::KeyTap(parse_key_code("f5").unwrap()))
        );
    }

    #[test]
    fn parses_editing_actions_and_keys() {
        let home = parse_layer_action("keyFunc_home").unwrap().unwrap();
        let page_down = parse_layer_action("keyTarget_page_down").unwrap().unwrap();
        let combo = parse_layer_action("keyCombo_rctrl_shift_pageup")
            .unwrap()
            .unwrap();

        assert_eq!(home, LayerAction::BuiltIn(BuiltInAction::Home(1)));
        assert_eq!(
            page_down,
            LayerAction::KeyTap(parse_key_code("page_down").unwrap())
        );
        assert_eq!(
            combo,
            LayerAction::KeyCombo(KeyCombo {
                modifiers: vec![KeyModifier::RCtrl, KeyModifier::Shift],
                key: KeyCode {
                    name: "page_up",
                    vk: VK_NEXT - 1,
                    kind: crate::keys::KeyKind::VirtualKey,
                },
            })
        );
        assert_eq!(parse_key_code("f5").unwrap().vk, VK_F1 + 4);
        assert_eq!(parse_key_code("home").unwrap().vk, VK_HOME);
    }

    #[test]
    fn example_ini_stays_parseable() {
        let config = Config::from_ini(include_str!("../examples/capslock_rs.example.ini")).unwrap();

        assert!(config.capslock_layer.len() >= 30);
        assert_eq!(
            find_action(&config, "m"),
            Some(LayerAction::KeyTap(
                parse_key_code("media_play_pause").unwrap()
            ))
        );
        assert_eq!(
            find_action(&config, "ctrl_shift_h"),
            Some(LayerAction::KeyCombo(
                parse_combo_suffix("ctrl_shift_left").unwrap()
            ))
        );
        assert_eq!(
            find_action(&config, "lalt_shift_j"),
            Some(LayerAction::BuiltIn(BuiltInAction::SelectDown(5)))
        );
    }
    #[test]
    fn reports_mapping_validation_warnings() {
        let result = Config::from_ini_with_validation(
            r#"
            [Keys]
            caps_h=keyFunc_moveLeft
            caps_h=keyFunc_moveRight
            caps_badkey=keyFunc_moveRight
            caps_j=keyFunc_noSuchAction
            caps_shift_lalt_k=keyFunc_moveUp
            caps_lalt_shift_k=keyFunc_moveDown
            "#,
        );

        assert!(!result.validation.has_errors());
        assert!(has_issue(
            &result.validation,
            ConfigIssueKind::DuplicateMapping
        ));
        assert!(has_issue(
            &result.validation,
            ConfigIssueKind::InvalidMapping
        ));
        assert!(has_issue(
            &result.validation,
            ConfigIssueKind::UnknownAction
        ));
        assert_eq!(
            find_action(&result.config, "h"),
            Some(LayerAction::BuiltIn(BuiltInAction::MoveLeft(1)))
        );
        assert_eq!(
            find_action(&result.config, "lalt_shift_k"),
            Some(LayerAction::BuiltIn(BuiltInAction::MoveUp(1)))
        );
    }

    #[test]
    fn preserves_case_for_unparsed_string_values() {
        let config = Config::from_ini(
            r#"
            [general]
            enabled = ON
            tap_capslock = EsCaPe

            [Keys]
            caps_r=KEYTARGET_F5
            caps_lalt_d=KeyFunc_MoveWordRight(2)

            [ui]
            language = ZH_cn
            settings_backend = IniBackendWithCASE
            settings_page = C:\Users\TuIp\ConfigHTTP?Token=AbC123
            "#,
        )
        .unwrap();

        assert!(config.general.enabled);
        assert_eq!(config.general.tap_capslock, TapCapsLock::Escape);
        assert_eq!(config.ui.language, Language::ZhCn);
        assert_eq!(config.ui.settings_backend, "IniBackendWithCASE");
        assert_eq!(
            config.ui.settings_page,
            "C:\\Users\\TuIp\\ConfigHTTP?Token=AbC123"
        );
        assert_eq!(
            find_action(&config, "r"),
            Some(LayerAction::KeyTap(parse_key_code("f5").unwrap()))
        );
        assert_eq!(
            find_action(&config, "lalt_d"),
            Some(LayerAction::BuiltIn(BuiltInAction::MoveWordRight(2)))
        );
    }

    #[test]
    fn reports_invalid_scalar_values_as_errors() {
        let result = Config::from_ini_with_validation(
            r#"
            [general]
            enabled = maybe

            [ui]
            language = fr-FR
            "#,
        );

        assert!(result.validation.has_errors());
        assert_eq!(
            result
                .validation
                .errors()
                .filter(|issue| issue.kind == ConfigIssueKind::InvalidValue)
                .count(),
            2
        );
        assert!(Config::from_ini("[general]\nenabled = maybe\n").is_err());
    }

    #[test]
    fn save_keeps_existing_config_when_atomic_replace_fails() {
        let dir = unique_temp_dir("atomic_save_failure");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("capslock_rs.ini");
        let original = "[ui]\nsettings_page = KeepCASE\n";
        fs::write(&path, original).unwrap();

        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();

        let save_result = Config::default().save(&path);
        let current = fs::read_to_string(&path).unwrap();

        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions).unwrap();
        let _ = fs::remove_dir_all(&dir);

        assert!(save_result.is_err());
        assert_eq!(current, original);
    }

    fn has_issue(validation: &ConfigValidation, kind: ConfigIssueKind) -> bool {
        validation.issues.iter().any(|issue| issue.kind == kind)
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "capslock_rs_{name}_{}_{}",
            std::process::id(),
            nanos
        ))
    }
    fn find_action(config: &Config, source: &str) -> Option<LayerAction> {
        let source = parse_combo_suffix(source).unwrap();
        config
            .capslock_layer
            .iter()
            .find_map(|mapping| (mapping.source == source).then_some(mapping.action.clone()))
    }
}
