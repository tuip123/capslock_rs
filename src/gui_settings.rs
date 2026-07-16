use std::mem::zeroed;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT, HBRUSH};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetDlgItem, GetWindowTextLengthW,
    GetWindowTextW, IsWindow, KillTimer, LoadCursorW, MoveWindow, RegisterClassW, SendMessageW,
    SetForegroundWindow, SetTimer, SetWindowTextW, ShowWindow, BS_AUTOCHECKBOX, BS_PUSHBUTTON,
    CBS_DROPDOWNLIST, CB_ADDSTRING, CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL, CW_USEDEFAULT,
    ES_AUTOHSCROLL, ES_READONLY, HMENU, IDC_ARROW, SW_HIDE, SW_SHOW, WM_APP, WM_CLOSE, WM_COMMAND,
    WM_DESTROY, WM_SETFONT, WM_TIMER, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_MINIMIZEBOX,
    WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use crate::config::{
    Config, ConfigIssue, ConfigIssueKind, ConfigIssueSeverity, ConfigValidation, KeyMapping,
    Language, LayerAction, TapCapsLock,
};
use crate::hook::{KeyCaptureMode, KeyCaptureOutcome, KeyCaptureRejectReason};
use crate::keys::{parse_capslock_combo_name, parse_combo_suffix, KeyCombo};
use crate::{app, i18n, logging, win};

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

#[derive(Clone, Debug)]
pub struct SettingsValidationReport {
    pub validation: ConfigValidation,
    pub expected_mapping_count: usize,
    pub parsed_mapping_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyBindingActionKind {
    BuiltIn,
    KeyTap,
    KeyCombo,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyBindingRow {
    pub source: String,
    pub action_kind: KeyBindingActionKind,
    pub action_value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ComboEditorParts {
    modifiers: Vec<String>,
    key: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BuiltInEditorValue {
    name: String,
    count: u32,
}

struct SettingsWindowState {
    hwnd: isize,
    model: SettingsModel,
    config_path: PathBuf,
    log_path: PathBuf,
    selected_binding_index: Option<usize>,
    active_capture: Option<KeyCaptureMode>,
}

const CLASS_NAME: &str = "CapsLockRSSettingsWindow";
const WINDOW_WIDTH: i32 = 880;
const WINDOW_HEIGHT: i32 = 760;

const ID_ENABLED: i32 = 3001;
const ID_START_WITH_WINDOWS: i32 = 3002;
const ID_RUN_AS_ADMIN: i32 = 3003;
const ID_SHOW_TRAY_ICON: i32 = 3004;
const ID_TAP_CAPSLOCK_LABEL: i32 = 3005;
const ID_TAP_CAPSLOCK: i32 = 3006;
const ID_LANGUAGE_LABEL: i32 = 3007;
const ID_LANGUAGE: i32 = 3008;
const ID_CONFIG_PATH_LABEL: i32 = 3009;
const ID_CONFIG_PATH: i32 = 3010;
const ID_OPEN_CONFIG: i32 = 3011;
const ID_LOG_PATH_LABEL: i32 = 3012;
const ID_LOG_PATH: i32 = 3013;
const ID_OPEN_LOG: i32 = 3014;
const ID_STATUS: i32 = 3015;
const ID_SAVE: i32 = 3016;
const ID_CLOSE: i32 = 3017;
const ID_BINDINGS_LABEL: i32 = 3018;
const ID_BINDING_LIST: i32 = 3019;
const ID_BINDING_SOURCE_LABEL: i32 = 3020;
const ID_BINDING_SOURCE: i32 = 3021;
const ID_BINDING_ACTION_KIND_LABEL: i32 = 3022;
const ID_BINDING_ACTION_KIND: i32 = 3023;
const ID_BINDING_ACTION_VALUE_LABEL: i32 = 3024;
const ID_BINDING_ACTION_VALUE: i32 = 3025;
const ID_BINDING_ADD: i32 = 3026;
const ID_BINDING_UPDATE: i32 = 3027;
const ID_BINDING_DELETE: i32 = 3028;
const ID_BINDING_SOURCE_PREFIX: i32 = 3029;
const ID_BINDING_SOURCE_MODIFIER_1: i32 = 3030;
const ID_BINDING_SOURCE_MODIFIER_2: i32 = 3031;
const ID_BINDING_SOURCE_MODIFIER_3: i32 = 3032;
const ID_BINDING_SOURCE_MODIFIER_4: i32 = 3033;
const ID_BINDING_ACTION_MODIFIER_1: i32 = 3034;
const ID_BINDING_ACTION_MODIFIER_2: i32 = 3035;
const ID_BINDING_ACTION_MODIFIER_3: i32 = 3036;
const ID_BINDING_ACTION_MODIFIER_4: i32 = 3037;
const ID_BINDING_SOURCE_MODIFIER_ADD: i32 = 3038;
const ID_BINDING_ACTION_MODIFIER_ADD: i32 = 3039;
const ID_BINDING_ACTION_COUNT_LABEL: i32 = 3040;
const ID_BINDING_ACTION_COUNT: i32 = 3041;
const ID_BINDING_SOURCE_CAPTURE: i32 = 3042;
const ID_BINDING_ACTION_CAPTURE: i32 = 3043;

const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;
const BST_CHECKED: u32 = 1;
const BST_UNCHECKED: u32 = 0;
const COLOR_WINDOW: i32 = 5;
const SS_LEFT: u32 = 0;
const LBS_NOTIFY: u32 = 0x0001;
const LBS_NOINTEGRALHEIGHT: u32 = 0x0100;
const LB_ADDSTRING: u32 = 0x0180;
const LB_RESETCONTENT: u32 = 0x0184;
const LB_GETCURSEL: u32 = 0x0188;
const LB_SETCURSEL: u32 = 0x0186;
const LB_ERR: isize = -1;
const LBN_SELCHANGE: u16 = 1;
const CBN_SELCHANGE: u16 = 1;
const WS_VSCROLL: u32 = 0x00200000;
const KEY_CAPTURE_TIMER_ID: usize = 1;
const KEY_CAPTURE_TIMEOUT_MS: u32 = 10_000;
const WM_KEY_CAPTURE_DONE: u32 = WM_APP + 2;
const NO_MODIFIER_CHOICE: &str = "-";
const SOURCE_MODIFIER_IDS: [i32; 4] = [
    ID_BINDING_SOURCE_MODIFIER_1,
    ID_BINDING_SOURCE_MODIFIER_2,
    ID_BINDING_SOURCE_MODIFIER_3,
    ID_BINDING_SOURCE_MODIFIER_4,
];
const ACTION_MODIFIER_IDS: [i32; 4] = [
    ID_BINDING_ACTION_MODIFIER_1,
    ID_BINDING_ACTION_MODIFIER_2,
    ID_BINDING_ACTION_MODIFIER_3,
    ID_BINDING_ACTION_MODIFIER_4,
];
const CONTROL_HEIGHT: i32 = 26;
const COMBO_DROPDOWN_HEIGHT: i32 = 180;
const SOURCE_MODIFIER_POSITIONS: [(i32, i32); 4] = [(590, 336), (700, 336), (500, 366), (610, 366)];
const ACTION_MODIFIER_POSITIONS: [(i32, i32); 4] = [(500, 528), (610, 528), (500, 558), (610, 558)];
const ACTION_VALUE_DEFAULT_Y: i32 = 528;
const ACTION_VALUE_COMBO_Y: i32 = 590;
const MODIFIER_CHOICES: &[&str] = &[
    NO_MODIFIER_CHOICE,
    "ctrl",
    "alt",
    "shift",
    "win",
    "lctrl",
    "rctrl",
    "lalt",
    "ralt",
    "lshift",
    "rshift",
    "lwin",
    "rwin",
];
const KEY_CHOICES: &[&str] = &[
    "space",
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "enter",
    "escape",
    "tab",
    "backspace",
    "delete",
    "insert",
    "home",
    "end",
    "page_up",
    "page_down",
    "left",
    "down",
    "up",
    "right",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
    "minus",
    "equals",
    "left_square_bracket",
    "right_square_bracket",
    "backslash",
    "semicolon",
    "single_quote",
    "grave",
    "comma",
    "dot",
    "slash",
    "volume_mute",
    "volume_down",
    "volume_up",
    "media_next",
    "media_prev",
    "media_stop",
    "media_play_pause",
];
const BUILTIN_ACTION_CHOICES: &[&str] = &[
    "moveLeft",
    "moveDown",
    "moveUp",
    "moveRight",
    "moveWordLeft",
    "moveWordRight",
    "selectLeft",
    "selectRight",
    "selectUp",
    "selectDown",
    "selectWordLeft",
    "selectWordRight",
    "home",
    "end",
    "pageUp",
    "pageDown",
    "enter",
    "backspace",
    "delete",
    "deleteWord",
    "forwardDeleteWord",
];

static CLASS_REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
static WINDOW_STATE: OnceLock<Mutex<Option<SettingsWindowState>>> = OnceLock::new();

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

    pub fn validate_for_save(&self) -> SettingsValidationReport {
        let mut config = Config::default();
        self.apply_to_config(&mut config);
        validate_config_for_save(&config, self.capslock_layer.len())
    }
}

impl SettingsValidationReport {
    pub fn has_errors(&self) -> bool {
        self.validation
            .issues
            .iter()
            .any(settings_issue_blocks_save)
    }

    pub fn has_warnings(&self) -> bool {
        self.validation.issues.iter().any(|issue| {
            issue.severity == ConfigIssueSeverity::Warning && !settings_issue_blocks_save(issue)
        })
    }

    pub fn format_for_language(&self, language: Language) -> String {
        format_settings_validation_report(language, self)
    }
}

pub(crate) fn validate_config_for_save(
    config: &Config,
    expected_mapping_count: usize,
) -> SettingsValidationReport {
    settings_validation_report_from_ini(&config.to_ini_string(), expected_mapping_count)
}

impl SettingsModel {
    pub fn binding_rows(&self) -> Vec<KeyBindingRow> {
        self.capslock_layer
            .iter()
            .map(KeyBindingRow::from_mapping)
            .collect()
    }

    pub fn replace_binding_rows(&mut self, rows: Vec<KeyBindingRow>) -> Result<(), String> {
        self.capslock_layer = parse_binding_rows(&rows)?;
        Ok(())
    }

    pub fn add_binding_row(&mut self, row: KeyBindingRow) -> Result<(), String> {
        let mut rows = self.binding_rows();
        rows.push(row);
        self.replace_binding_rows(rows)
    }

    pub fn update_binding_row(&mut self, index: usize, row: KeyBindingRow) -> Result<(), String> {
        let mut rows = self.binding_rows();
        let Some(slot) = rows.get_mut(index) else {
            return Err(format!("binding row {index} was not found"));
        };
        *slot = row;
        self.replace_binding_rows(rows)
    }

    pub fn delete_binding_row(&mut self, index: usize) -> Result<(), String> {
        let mut rows = self.binding_rows();
        if index >= rows.len() {
            return Err(format!("binding row {index} was not found"));
        }
        rows.remove(index);
        self.replace_binding_rows(rows)
    }
}

impl KeyBindingRow {
    pub fn new(
        source: impl Into<String>,
        action_kind: KeyBindingActionKind,
        action_value: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            action_kind,
            action_value: action_value.into(),
        }
    }

    fn from_mapping(mapping: &KeyMapping) -> Self {
        let (action_kind, action_value) = match &mapping.action {
            LayerAction::BuiltIn(action) => {
                let value = strip_ascii_case_prefix(&action.as_key_func(), "keyFunc_").to_string();
                (KeyBindingActionKind::BuiltIn, value)
            }
            LayerAction::KeyTap(key) => (KeyBindingActionKind::KeyTap, key.name.to_string()),
            LayerAction::KeyCombo(combo) => (KeyBindingActionKind::KeyCombo, combo.ini_suffix()),
        };

        Self {
            source: mapping.source.capslock_ini_key(),
            action_kind,
            action_value,
        }
    }

    fn source_ini_key(&self) -> Result<String, String> {
        let source = self.source.trim();
        if source.is_empty() {
            return Err("binding source cannot be empty".to_string());
        }
        if has_ascii_case_prefix(source, "caps_") {
            Ok(source.to_string())
        } else {
            Ok(format!("caps_{source}"))
        }
    }

    fn action_ini_value(&self) -> Result<String, String> {
        let value = self.action_value.trim();
        if value.is_empty() {
            return Err("binding action value cannot be empty".to_string());
        }

        let prefix = match self.action_kind {
            KeyBindingActionKind::BuiltIn => "keyFunc_",
            KeyBindingActionKind::KeyTap => "keyTarget_",
            KeyBindingActionKind::KeyCombo => "keyCombo_",
        };
        Ok(ensure_ascii_case_prefix(value, prefix))
    }
}

impl KeyBindingActionKind {
    fn combo_index(self) -> usize {
        match self {
            KeyBindingActionKind::BuiltIn => 0,
            KeyBindingActionKind::KeyTap => 1,
            KeyBindingActionKind::KeyCombo => 2,
        }
    }

    fn from_combo_index(index: isize) -> Result<Self, String> {
        match index {
            0 => Ok(KeyBindingActionKind::BuiltIn),
            1 => Ok(KeyBindingActionKind::KeyTap),
            2 => Ok(KeyBindingActionKind::KeyCombo),
            _ => Err("binding action type is not selected".to_string()),
        }
    }
}

fn parse_binding_rows(rows: &[KeyBindingRow]) -> Result<Vec<KeyMapping>, String> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let content = binding_rows_ini(rows)?;

    // Use the real config parser so GUI validation cannot drift from INI semantics.
    let parsed = Config::from_ini_with_validation(&content);
    let report = settings_validation_report(
        parsed.validation.clone(),
        rows.len(),
        parsed.config.capslock_layer.len(),
    );
    if !report.validation.issues.is_empty() {
        return Err(report.format_for_language(Language::EnUs));
    }
    if parsed.config.capslock_layer.len() != rows.len() {
        return Err("some binding rows were ignored by the config parser".to_string());
    }

    Ok(parsed.config.capslock_layer)
}

#[cfg(test)]
fn validate_binding_rows_for_save(
    rows: &[KeyBindingRow],
) -> Result<SettingsValidationReport, String> {
    let content = binding_rows_ini(rows)?;
    Ok(settings_validation_report_from_ini(&content, rows.len()))
}

fn binding_rows_ini(rows: &[KeyBindingRow]) -> Result<String, String> {
    let mut content = String::from("[Keys]\n");
    for row in rows {
        content.push_str(&row.source_ini_key()?);
        content.push('=');
        content.push_str(&row.action_ini_value()?);
        content.push('\n');
    }
    Ok(content)
}

fn settings_validation_report_from_ini(
    content: &str,
    expected_mapping_count: usize,
) -> SettingsValidationReport {
    let parsed = Config::from_ini_with_validation(content);
    settings_validation_report(
        parsed.validation,
        expected_mapping_count,
        parsed.config.capslock_layer.len(),
    )
}

fn settings_validation_report(
    mut validation: ConfigValidation,
    expected_mapping_count: usize,
    parsed_mapping_count: usize,
) -> SettingsValidationReport {
    if expected_mapping_count != parsed_mapping_count
        && !validation.issues.iter().any(settings_issue_blocks_save)
    {
        validation.issues.push(ConfigIssue {
            severity: ConfigIssueSeverity::Error,
            kind: ConfigIssueKind::InvalidMapping,
            line: None,
            section: Some("keys".to_string()),
            key: None,
            value: None,
            message: format!(
                "expected {expected_mapping_count} mapping rows but parser returned {parsed_mapping_count}"
            ),
        });
    }

    SettingsValidationReport {
        validation,
        expected_mapping_count,
        parsed_mapping_count,
    }
}

fn settings_issue_blocks_save(issue: &ConfigIssue) -> bool {
    issue.severity == ConfigIssueSeverity::Error
        || matches!(
            issue.kind,
            ConfigIssueKind::DuplicateMapping
                | ConfigIssueKind::InvalidMapping
                | ConfigIssueKind::UnknownAction
        )
}

fn format_settings_validation_report(
    language: Language,
    report: &SettingsValidationReport,
) -> String {
    let mut lines = Vec::new();
    let header_key = if report.has_errors() {
        "settings.validation_blocked"
    } else {
        "settings.validation_warnings"
    };
    lines.push(i18n::text(language, header_key).to_string());

    for issue in &report.validation.issues {
        lines.push(format_settings_validation_issue(language, issue));
    }

    lines.join("\n")
}

fn format_settings_validation_issue(language: Language, issue: &ConfigIssue) -> String {
    let severity_key = if settings_issue_blocks_save(issue) {
        "settings.validation.severity.error"
    } else {
        "settings.validation.severity.warning"
    };
    let kind_key = match issue.kind {
        ConfigIssueKind::Syntax => "settings.validation.issue.syntax",
        ConfigIssueKind::InvalidValue => "settings.validation.issue.invalid_value",
        ConfigIssueKind::InvalidMapping => "settings.validation.issue.invalid_mapping",
        ConfigIssueKind::DuplicateMapping => "settings.validation.issue.duplicate_mapping",
        ConfigIssueKind::UnknownAction => "settings.validation.issue.unknown_action",
    };

    let mut context = Vec::new();
    if let Some(line) = issue.line {
        context.push(format!(
            "{} {line}",
            i18n::text(language, "settings.validation.line")
        ));
    }
    if let Some(section) = &issue.section {
        context.push(format!("[{section}]"));
    }
    match (&issue.key, &issue.value) {
        (Some(key), Some(value)) => context.push(format!("{key}={value}")),
        (Some(key), None) => context.push(key.clone()),
        _ => {}
    }

    let context = if context.is_empty() {
        String::new()
    } else {
        format!(" ({})", context.join(", "))
    };

    format!(
        "- {}: {}{} - {}",
        i18n::text(language, severity_key),
        i18n::text(language, kind_key),
        context,
        issue.message
    )
}

fn ensure_ascii_case_prefix(value: &str, prefix: &str) -> String {
    if has_ascii_case_prefix(value, prefix) {
        value.to_string()
    } else {
        format!("{prefix}{value}")
    }
}

fn strip_ascii_case_prefix<'a>(value: &'a str, prefix: &str) -> &'a str {
    if has_ascii_case_prefix(value, prefix) {
        &value[prefix.len()..]
    } else {
        value
    }
}

fn has_ascii_case_prefix(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .map(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .unwrap_or(false)
}
impl ComboEditorParts {
    fn default_key() -> Self {
        Self {
            modifiers: Vec::new(),
            key: KEY_CHOICES[0].to_string(),
        }
    }

    fn from_key_combo(combo: &KeyCombo) -> Self {
        Self {
            modifiers: combo
                .modifiers
                .iter()
                .map(|modifier| modifier.canonical_name().to_string())
                .collect(),
            key: combo.key.name.to_string(),
        }
    }
}

fn combo_parts_from_source(source: &str) -> ComboEditorParts {
    parse_capslock_combo_name(source)
        .map(|combo| ComboEditorParts::from_key_combo(&combo))
        .unwrap_or_else(|_| ComboEditorParts::default_key())
}

fn combo_parts_from_suffix(suffix: &str) -> ComboEditorParts {
    parse_combo_suffix(suffix)
        .map(|combo| ComboEditorParts::from_key_combo(&combo))
        .unwrap_or_else(|_| ComboEditorParts::default_key())
}

fn caps_source_from_parts(modifiers: &[String], key: &str) -> Result<String, String> {
    normalized_combo_suffix_from_parts(modifiers, key).map(|suffix| format!("caps_{suffix}"))
}

fn normalized_combo_suffix_from_parts(modifiers: &[String], key: &str) -> Result<String, String> {
    let key = key.trim();
    if key.is_empty() || key == NO_MODIFIER_CHOICE {
        return Err("combo key cannot be empty".to_string());
    }

    let mut parts: Vec<String> = modifiers
        .iter()
        .map(|modifier| modifier_choice_value(modifier))
        .filter(|modifier| !modifier.is_empty())
        .collect();
    parts.push(key.to_string());

    // Let the existing key parser normalize order and reject duplicate modifier families.
    parse_combo_suffix(&parts.join("_"))
        .map(|combo| combo.ini_suffix())
        .map_err(|error| format!("invalid combo selection: {error}"))
}

fn modifier_choice_value(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() || value == NO_MODIFIER_CHOICE {
        String::new()
    } else {
        value.to_ascii_lowercase()
    }
}
fn builtin_editor_value(value: &str) -> BuiltInEditorValue {
    let value = strip_ascii_case_prefix(value.trim(), "keyFunc_");
    let Some(open_index) = value.find('(') else {
        return BuiltInEditorValue {
            name: value.to_string(),
            count: 1,
        };
    };

    let name = value[..open_index].trim().to_string();
    let count = value
        .strip_suffix(')')
        .and_then(|text| text.get(open_index + 1..))
        .and_then(|text| text.trim().parse::<u32>().ok())
        .unwrap_or(1)
        .max(1);
    BuiltInEditorValue { name, count }
}

fn builtin_action_value(name: &str, count: &str) -> Result<String, String> {
    let name = strip_ascii_case_prefix(name.trim(), "keyFunc_").trim();
    if name.is_empty() {
        return Err("built-in action cannot be empty".to_string());
    }

    let count = count
        .trim()
        .parse::<u32>()
        .map_err(|error| format!("invalid built-in action count: {error}"))?
        .max(1);
    if count <= 1 {
        Ok(name.to_string())
    } else {
        Ok(format!("{name}({count})"))
    }
}

pub fn open() -> Result<(), String> {
    if let Some(hwnd) = active_window() {
        unsafe {
            SetForegroundWindow(hwnd);
        }
        return Ok(());
    }

    register_class()?;
    let snapshot = app::settings_snapshot()?;
    let title = win::to_wide_null(i18n::text(snapshot.model.language, "settings.title"));
    let class_name = win::to_wide_null(CLASS_NAME);

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            null_mut(),
            null_mut(),
            win::module_handle(),
            null(),
        )
    };

    if hwnd.is_null() {
        return Err(last_os_error("failed to create settings window"));
    }

    {
        let mut state = state_holder()
            .lock()
            .map_err(|_| "settings window lock is poisoned".to_string())?;
        *state = Some(SettingsWindowState {
            hwnd: hwnd as isize,
            model: snapshot.model,
            config_path: snapshot.config_path,
            log_path: snapshot.log_path,
            selected_binding_index: None,
            active_capture: None,
        });
    }

    create_controls(hwnd)?;
    refresh_window(hwnd, None)?;

    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
    }

    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    match message {
        WM_COMMAND => {
            handle_command(
                hwnd,
                (w_param & 0xffff) as i32,
                ((w_param >> 16) & 0xffff) as u16,
            );
            0
        }
        WM_KEY_CAPTURE_DONE => {
            finish_key_capture(hwnd);
            0
        }
        WM_TIMER if w_param == KEY_CAPTURE_TIMER_ID => {
            timeout_key_capture(hwnd);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            let _ = app::cancel_key_capture();
            KillTimer(hwnd, KEY_CAPTURE_TIMER_ID);
            clear_window_state(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, message, w_param, l_param),
    }
}

fn handle_command(hwnd: HWND, command: i32, notification: u16) {
    match command {
        ID_SAVE => save_from_window(hwnd),
        ID_CLOSE => unsafe {
            DestroyWindow(hwnd);
        },
        ID_OPEN_CONFIG => app::open_config(),
        ID_OPEN_LOG => app::open_log(),
        ID_BINDING_LIST if notification == LBN_SELCHANGE => change_selected_binding(hwnd),
        ID_BINDING_ACTION_KIND if notification == CBN_SELCHANGE => change_binding_action_kind(hwnd),
        ID_BINDING_SOURCE_MODIFIER_ADD => add_modifier_to_editor(hwnd, &SOURCE_MODIFIER_IDS),
        ID_BINDING_ACTION_MODIFIER_ADD => add_modifier_to_editor(hwnd, &ACTION_MODIFIER_IDS),
        ID_BINDING_SOURCE_CAPTURE => toggle_key_capture(hwnd, KeyCaptureMode::Source),
        ID_BINDING_ACTION_CAPTURE => toggle_key_capture(hwnd, KeyCaptureMode::Target),
        ID_BINDING_ADD => add_default_binding_from_list(hwnd),
        id if is_modifier_combo(id) && notification == CBN_SELCHANGE => {
            normalize_modifier_editor(hwnd, id)
        }
        ID_BINDING_UPDATE => update_selected_binding_from_window(hwnd),
        ID_BINDING_DELETE => delete_selected_binding(hwnd),
        _ => {}
    }
}

fn save_from_window(hwnd: HWND) {
    let model = match collect_model(hwnd) {
        Ok(model) => model,
        Err(error) => {
            logging::log_line(format!("failed to collect settings form: {error}"));
            show_settings_error(
                app::current_language(),
                "error.save_settings_failed",
                &error,
            );
            return;
        }
    };
    let language = model.language;
    let validation = model.validate_for_save();
    if validation.has_errors() {
        log_settings_validation_report(&validation);
        let _ = set_status(hwnd, language, "settings.validation_blocked");
        show_settings_validation(language, &validation);
        return;
    }

    let has_warnings = validation.has_warnings();
    if has_warnings {
        log_settings_validation_report(&validation);
    }

    match app::save_settings_model(&model) {
        Ok(()) => match app::settings_snapshot() {
            Ok(snapshot) => {
                if let Ok(mut state) = state_holder().lock() {
                    let selected_binding_index = state
                        .as_ref()
                        .and_then(|state| state.selected_binding_index);
                    *state = Some(SettingsWindowState {
                        hwnd: hwnd as isize,
                        model: snapshot.model,
                        config_path: snapshot.config_path,
                        log_path: snapshot.log_path,
                        selected_binding_index,
                        active_capture: None,
                    });
                }
                let status_key = if has_warnings {
                    "settings.saved_with_warnings"
                } else {
                    "settings.saved"
                };
                let _ = refresh_window(hwnd, Some(status_key));
                if has_warnings {
                    show_settings_validation(language, &validation);
                }
            }
            Err(error) => {
                logging::log_line(format!("failed to refresh settings after save: {error}"));
                show_settings_error(language, "error.reload_failed", &error);
            }
        },
        Err(error) => {
            logging::log_line(format!("failed to save settings: {error}"));
            show_settings_error(language, "error.save_settings_failed", &error);
            let _ = set_status(hwnd, language, "settings.save_failed");
        }
    }
}

fn add_default_binding_from_list(hwnd: HWND) {
    match add_default_binding_from_list_result() {
        Ok(()) => {
            let _ = refresh_bindings_area(hwnd);
            let _ = set_status(hwnd, state_language(), "settings.binding_added");
        }
        Err(error) => handle_binding_error(hwnd, error),
    }
}

fn add_default_binding_from_list_result() -> Result<(), String> {
    let mut state = state_holder()
        .lock()
        .map_err(|_| "settings window lock is poisoned".to_string())?;
    let state = state
        .as_mut()
        .ok_or_else(|| "settings window is not initialized".to_string())?;
    let source = next_default_source(&state.model);
    state.model.add_binding_row(KeyBindingRow::new(
        source,
        KeyBindingActionKind::BuiltIn,
        default_action_value(KeyBindingActionKind::BuiltIn),
    ))?;
    state.selected_binding_index = state.model.capslock_layer.len().checked_sub(1);
    Ok(())
}

fn update_selected_binding_from_window(hwnd: HWND) {
    match commit_current_binding_editor(hwnd) {
        Ok(()) => {
            let _ = refresh_bindings_area(hwnd);
            let _ = set_status(hwnd, state_language(), "settings.binding_updated");
        }
        Err(error) => handle_binding_error(hwnd, error),
    }
}

fn delete_selected_binding(hwnd: HWND) {
    match delete_selected_binding_result() {
        Ok(()) => {
            let _ = refresh_bindings_area(hwnd);
            let _ = set_status(hwnd, state_language(), "settings.binding_deleted");
        }
        Err(error) => handle_binding_error(hwnd, error),
    }
}

fn delete_selected_binding_result() -> Result<(), String> {
    let mut state = state_holder()
        .lock()
        .map_err(|_| "settings window lock is poisoned".to_string())?;
    let state = state
        .as_mut()
        .ok_or_else(|| "settings window is not initialized".to_string())?;
    let Some(index) = state.selected_binding_index else {
        return Err("no binding row is selected".to_string());
    };

    state.model.delete_binding_row(index)?;
    let len = state.model.capslock_layer.len();
    state.selected_binding_index = if len == 0 {
        None
    } else {
        Some(index.min(len - 1))
    };
    Ok(())
}

fn change_selected_binding(hwnd: HWND) {
    let result = (|| {
        let Some(new_index) = selected_binding_index(hwnd)? else {
            set_selected_binding_index(None)?;
            return populate_binding_editor(hwnd);
        };
        let row_count = with_state(|state| state.model.capslock_layer.len())?;
        if new_index >= row_count {
            add_default_binding_from_list(hwnd);
            return Ok(());
        }
        set_selected_binding_index(Some(new_index))?;
        populate_binding_editor(hwnd)
    })();

    if let Err(error) = result {
        let previous_index = with_state(|state| state.selected_binding_index)
            .ok()
            .flatten();
        let _ = set_list_selection(hwnd, previous_index);
        handle_binding_error(hwnd, error);
    }
}

fn commit_current_binding_editor(hwnd: HWND) -> Result<(), String> {
    let selected = with_state(|state| state.selected_binding_index)?;
    commit_binding_editor_for_index(hwnd, selected)
}

fn commit_binding_editor_for_index(hwnd: HWND, index: Option<usize>) -> Result<(), String> {
    let Some(index) = index else {
        return Ok(());
    };
    let row = collect_binding_editor_row(hwnd)?;
    let mut state = state_holder()
        .lock()
        .map_err(|_| "settings window lock is poisoned".to_string())?;
    let state = state
        .as_mut()
        .ok_or_else(|| "settings window is not initialized".to_string())?;
    state.model.update_binding_row(index, row)
}

fn set_selected_binding_index(index: Option<usize>) -> Result<(), String> {
    let mut state = state_holder()
        .lock()
        .map_err(|_| "settings window lock is poisoned".to_string())?;
    let state = state
        .as_mut()
        .ok_or_else(|| "settings window is not initialized".to_string())?;
    state.selected_binding_index = index;
    Ok(())
}

fn change_binding_action_kind(hwnd: HWND) {
    let language = state_language();
    let kind = combo_index(hwnd, ID_BINDING_ACTION_KIND)
        .and_then(KeyBindingActionKind::from_combo_index)
        .unwrap_or(KeyBindingActionKind::BuiltIn);
    let row = KeyBindingRow::new("caps_space", kind, default_action_value(kind));
    if let Err(error) = populate_binding_action_editor(hwnd, language, &row)
        .and_then(|()| update_capture_controls(hwnd))
    {
        handle_binding_error(hwnd, error);
    }
}

fn toggle_key_capture(hwnd: HWND, mode: KeyCaptureMode) {
    let result = (|| {
        if active_capture_mode()? == Some(mode) {
            cancel_active_key_capture(hwnd, Some("settings.capture_cancelled"))?;
            return Ok(());
        }

        if active_capture_mode()?.is_some() {
            cancel_active_key_capture(hwnd, None)?;
        }

        app::begin_key_capture(hwnd, mode, WM_KEY_CAPTURE_DONE)?;
        set_active_capture(Some(mode))?;
        unsafe {
            SetTimer(hwnd, KEY_CAPTURE_TIMER_ID, KEY_CAPTURE_TIMEOUT_MS, None);
        }
        update_capture_controls(hwnd)?;
        set_status(hwnd, state_language(), capture_started_status_key(mode))
    })();

    if let Err(error) = result {
        handle_binding_error(hwnd, error);
    }
}

fn finish_key_capture(hwnd: HWND) {
    let result = (|| {
        unsafe {
            KillTimer(hwnd, KEY_CAPTURE_TIMER_ID);
        }
        let outcome = app::take_key_capture_result()
            .ok_or_else(|| "key capture finished without a result".to_string())?;
        set_active_capture(None)?;
        update_capture_controls(hwnd)?;

        match outcome {
            KeyCaptureOutcome::Captured { mode, combo } => match mode {
                KeyCaptureMode::Source => apply_captured_source(hwnd, &combo),
                KeyCaptureMode::Target => apply_captured_target(hwnd, &combo),
            },
            KeyCaptureOutcome::Rejected { mode, reason } => {
                handle_capture_rejection(hwnd, mode, reason)
            }
        }
    })();

    if let Err(error) = result {
        handle_binding_error(hwnd, error);
    }
}

fn timeout_key_capture(hwnd: HWND) {
    if active_capture_mode().ok().flatten().is_some() {
        if let Err(error) = cancel_active_key_capture(hwnd, Some("settings.capture_timeout")) {
            handle_binding_error(hwnd, error);
        }
    }
}

fn cancel_active_key_capture(hwnd: HWND, status_key: Option<&str>) -> Result<(), String> {
    app::cancel_key_capture()?;
    unsafe {
        KillTimer(hwnd, KEY_CAPTURE_TIMER_ID);
    }
    set_active_capture(None)?;
    update_capture_controls(hwnd)?;
    if let Some(status_key) = status_key {
        set_status(hwnd, state_language(), status_key)?;
    }
    Ok(())
}

fn apply_captured_source(hwnd: HWND, combo: &KeyCombo) -> Result<(), String> {
    let parts = ComboEditorParts::from_key_combo(combo);
    populate_modifier_combos(hwnd, &SOURCE_MODIFIER_IDS, &parts.modifiers)?;
    refresh_modifier_editor_visibility(hwnd, &SOURCE_MODIFIER_IDS)?;
    populate_combo_values(hwnd, ID_BINDING_SOURCE, KEY_CHOICES, &parts.key)?;
    set_status(hwnd, state_language(), "settings.capture_source_saved")
}

fn apply_captured_target(hwnd: HWND, combo: &KeyCombo) -> Result<(), String> {
    let language = state_language();
    if combo.modifiers.is_empty() {
        populate_binding_action_kind(hwnd, language, KeyBindingActionKind::KeyTap)?;
        move_action_value_editor(hwnd, ACTION_VALUE_DEFAULT_Y)?;
        show_combo_modifier_editor(hwnd, &ACTION_MODIFIER_IDS, false)?;
        show_builtin_count_editor(hwnd, false)?;
        populate_combo_values(hwnd, ID_BINDING_ACTION_VALUE, KEY_CHOICES, combo.key.name)?;
    } else {
        populate_binding_action_kind(hwnd, language, KeyBindingActionKind::KeyCombo)?;
        move_action_value_editor(hwnd, ACTION_VALUE_COMBO_Y)?;
        show_builtin_count_editor(hwnd, false)?;
        let parts = ComboEditorParts::from_key_combo(combo);
        populate_modifier_combos(hwnd, &ACTION_MODIFIER_IDS, &parts.modifiers)?;
        refresh_modifier_editor_visibility(hwnd, &ACTION_MODIFIER_IDS)?;
        populate_combo_values(hwnd, ID_BINDING_ACTION_VALUE, KEY_CHOICES, &parts.key)?;
    }
    update_capture_controls(hwnd)?;
    set_status(hwnd, language, "settings.capture_target_saved")
}

fn handle_capture_rejection(
    hwnd: HWND,
    mode: KeyCaptureMode,
    reason: KeyCaptureRejectReason,
) -> Result<(), String> {
    let status_key = match reason {
        KeyCaptureRejectReason::MissingCapsLock => "settings.capture_missing_caps",
        KeyCaptureRejectReason::UnsupportedKey(vk) => {
            logging::log_line(format!(
                "key capture rejected mode={} unsupported_vk={vk}",
                capture_mode_name(mode)
            ));
            "settings.capture_unsupported_key"
        }
        KeyCaptureRejectReason::InvalidCombo(error) => {
            logging::log_line(format!(
                "key capture rejected mode={} error={error}",
                capture_mode_name(mode)
            ));
            "settings.capture_invalid_combo"
        }
    };
    set_status(hwnd, state_language(), status_key)
}

fn update_capture_controls(hwnd: HWND) -> Result<(), String> {
    let language = state_language();
    let active = active_capture_mode()?;
    let source_text = if active == Some(KeyCaptureMode::Source) {
        i18n::text(language, "settings.binding.listen_cancel")
    } else {
        i18n::text(language, "settings.binding.listen_source")
    };
    let target_text = if active == Some(KeyCaptureMode::Target) {
        i18n::text(language, "settings.binding.listen_cancel")
    } else {
        i18n::text(language, "settings.binding.listen_target")
    };
    set_control_text(hwnd, ID_BINDING_SOURCE_CAPTURE, source_text)?;
    set_control_text(hwnd, ID_BINDING_ACTION_CAPTURE, target_text)?;

    let action_kind = combo_index(hwnd, ID_BINDING_ACTION_KIND)
        .and_then(KeyBindingActionKind::from_combo_index)
        .unwrap_or(KeyBindingActionKind::BuiltIn);
    show_control(
        hwnd,
        ID_BINDING_ACTION_CAPTURE,
        action_kind != KeyBindingActionKind::BuiltIn || active == Some(KeyCaptureMode::Target),
    )
}

fn active_capture_mode() -> Result<Option<KeyCaptureMode>, String> {
    with_state(|state| state.active_capture)
}

fn set_active_capture(mode: Option<KeyCaptureMode>) -> Result<(), String> {
    let mut state = state_holder()
        .lock()
        .map_err(|_| "settings window lock is poisoned".to_string())?;
    let state = state
        .as_mut()
        .ok_or_else(|| "settings window is not initialized".to_string())?;
    state.active_capture = mode;
    Ok(())
}

fn capture_started_status_key(mode: KeyCaptureMode) -> &'static str {
    match mode {
        KeyCaptureMode::Source => "settings.capture_source_started",
        KeyCaptureMode::Target => "settings.capture_target_started",
    }
}

fn capture_mode_name(mode: KeyCaptureMode) -> &'static str {
    match mode {
        KeyCaptureMode::Source => "source",
        KeyCaptureMode::Target => "target",
    }
}

fn collect_binding_editor_row(hwnd: HWND) -> Result<KeyBindingRow, String> {
    let source_modifiers = selected_modifier_values(hwnd, &SOURCE_MODIFIER_IDS)?;
    let source_key = control_text(hwnd, ID_BINDING_SOURCE)?;
    let source = caps_source_from_parts(&source_modifiers, &source_key)?;
    let action_kind =
        KeyBindingActionKind::from_combo_index(combo_index(hwnd, ID_BINDING_ACTION_KIND)?)?;
    let action_value = match action_kind {
        KeyBindingActionKind::BuiltIn => builtin_action_value(
            &control_text(hwnd, ID_BINDING_ACTION_VALUE)?,
            &control_text(hwnd, ID_BINDING_ACTION_COUNT)?,
        )?,
        KeyBindingActionKind::KeyTap => control_text(hwnd, ID_BINDING_ACTION_VALUE)?,
        KeyBindingActionKind::KeyCombo => {
            let action_modifiers = selected_modifier_values(hwnd, &ACTION_MODIFIER_IDS)?;
            let action_key = control_text(hwnd, ID_BINDING_ACTION_VALUE)?;
            normalized_combo_suffix_from_parts(&action_modifiers, &action_key)?
        }
    };

    Ok(KeyBindingRow::new(source, action_kind, action_value))
}

fn handle_binding_error(hwnd: HWND, error: String) {
    let language = state_language();
    logging::log_line(format!("failed to update key binding list: {error}"));
    let _ = set_status(hwnd, language, "settings.binding_failed");
    show_settings_error(language, "error.update_binding_failed", &error);
}
fn register_class() -> Result<(), String> {
    CLASS_REGISTERED
        .get_or_init(|| {
            let class_name = win::to_wide_null(CLASS_NAME);
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: win::module_handle(),
                hCursor: unsafe { LoadCursorW(null_mut(), IDC_ARROW) },
                hbrBackground: (COLOR_WINDOW + 1) as isize as HBRUSH,
                lpszClassName: class_name.as_ptr(),
                ..unsafe { zeroed() }
            };

            let atom = unsafe { RegisterClassW(&window_class) };
            if atom == 0 {
                let error = unsafe { GetLastError() };
                if error != ERROR_CLASS_ALREADY_EXISTS {
                    return Err(format!("failed to register settings window class: {error}"));
                }
            }

            Ok(())
        })
        .clone()
}

fn create_controls(hwnd: HWND) -> Result<(), String> {
    create_checkbox(hwnd, ID_ENABLED, 18, 18, 260, 24)?;
    create_checkbox(hwnd, ID_START_WITH_WINDOWS, 18, 48, 260, 24)?;
    create_checkbox(hwnd, ID_RUN_AS_ADMIN, 18, 78, 260, 24)?;
    create_checkbox(hwnd, ID_SHOW_TRAY_ICON, 18, 108, 260, 24)?;

    create_static(hwnd, ID_TAP_CAPSLOCK_LABEL, 470, 20, 130, 22)?;
    create_combo(hwnd, ID_TAP_CAPSLOCK, 610, 16, 220, 160)?;

    create_static(hwnd, ID_LANGUAGE_LABEL, 470, 60, 130, 22)?;
    create_combo(hwnd, ID_LANGUAGE, 610, 56, 220, 160)?;

    create_static(hwnd, ID_CONFIG_PATH_LABEL, 18, 148, 700, 22)?;
    create_readonly_edit(hwnd, ID_CONFIG_PATH, 18, 174, 700, 24)?;
    create_button(hwnd, ID_OPEN_CONFIG, 732, 173, 98, 26)?;

    create_static(hwnd, ID_LOG_PATH_LABEL, 18, 212, 700, 22)?;
    create_readonly_edit(hwnd, ID_LOG_PATH, 18, 238, 700, 24)?;
    create_button(hwnd, ID_OPEN_LOG, 732, 237, 98, 26)?;

    create_static(hwnd, ID_BINDINGS_LABEL, 18, 286, 450, 22)?;
    create_listbox(hwnd, ID_BINDING_LIST, 18, 314, 450, 340)?;

    create_static(hwnd, ID_BINDING_SOURCE_LABEL, 500, 314, 330, 22)?;
    create_static(hwnd, ID_BINDING_SOURCE_PREFIX, 500, 340, 84, 24)?;
    create_button(hwnd, ID_BINDING_SOURCE_MODIFIER_ADD, 590, 336, 28, 26)?;
    create_combo(hwnd, ID_BINDING_SOURCE_MODIFIER_1, 590, 336, 110, 180)?;
    create_combo(hwnd, ID_BINDING_SOURCE_MODIFIER_2, 710, 336, 110, 180)?;
    create_combo(hwnd, ID_BINDING_SOURCE_MODIFIER_3, 500, 366, 110, 180)?;
    create_combo(hwnd, ID_BINDING_SOURCE_MODIFIER_4, 620, 366, 110, 180)?;
    create_combo(hwnd, ID_BINDING_SOURCE, 500, 396, 210, 260)?;
    create_button(hwnd, ID_BINDING_SOURCE_CAPTURE, 720, 396, 110, 26)?;

    create_static(hwnd, ID_BINDING_ACTION_KIND_LABEL, 500, 438, 330, 22)?;
    create_combo(hwnd, ID_BINDING_ACTION_KIND, 500, 464, 330, 160)?;

    create_static(hwnd, ID_BINDING_ACTION_VALUE_LABEL, 500, 502, 330, 22)?;
    create_static(hwnd, ID_BINDING_ACTION_COUNT_LABEL, 500, 566, 72, 22)?;
    create_text_edit(hwnd, ID_BINDING_ACTION_COUNT, 500, 590, 58, 24)?;
    create_button(hwnd, ID_BINDING_ACTION_MODIFIER_ADD, 500, 566, 28, 26)?;
    create_combo(hwnd, ID_BINDING_ACTION_MODIFIER_1, 500, 566, 110, 180)?;
    create_combo(hwnd, ID_BINDING_ACTION_MODIFIER_2, 610, 566, 110, 180)?;
    create_combo(hwnd, ID_BINDING_ACTION_MODIFIER_3, 500, 596, 110, 180)?;
    create_combo(hwnd, ID_BINDING_ACTION_MODIFIER_4, 610, 596, 110, 180)?;
    create_combo(hwnd, ID_BINDING_ACTION_VALUE, 500, 528, 210, 260)?;
    create_button(hwnd, ID_BINDING_ACTION_CAPTURE, 720, 528, 110, 26)?;

    create_button(hwnd, ID_BINDING_ADD, 500, 632, 88, 28)?;
    create_button(hwnd, ID_BINDING_UPDATE, 604, 632, 88, 28)?;
    create_button(hwnd, ID_BINDING_DELETE, 708, 632, 88, 28)?;

    create_static(hwnd, ID_STATUS, 18, 690, 500, 24)?;
    create_button(hwnd, ID_SAVE, 632, 688, 88, 28)?;
    create_button(hwnd, ID_CLOSE, 742, 688, 88, 28)?;
    Ok(())
}

fn refresh_window(hwnd: HWND, status_key: Option<&str>) -> Result<(), String> {
    let (model, config_path, log_path) = with_state(|state| {
        (
            state.model.clone(),
            state.config_path.clone(),
            state.log_path.clone(),
        )
    })?;
    let language = model.language;

    set_window_text(hwnd, i18n::text(language, "settings.title"));
    set_control_text(hwnd, ID_ENABLED, i18n::text(language, "settings.enabled"))?;
    set_control_text(
        hwnd,
        ID_START_WITH_WINDOWS,
        i18n::text(language, "settings.start_with_windows"),
    )?;
    set_control_text(
        hwnd,
        ID_RUN_AS_ADMIN,
        i18n::text(language, "settings.run_as_admin"),
    )?;
    set_control_text(
        hwnd,
        ID_SHOW_TRAY_ICON,
        i18n::text(language, "settings.show_tray_icon"),
    )?;
    set_control_text(
        hwnd,
        ID_TAP_CAPSLOCK_LABEL,
        i18n::text(language, "settings.tap_capslock"),
    )?;
    set_control_text(
        hwnd,
        ID_LANGUAGE_LABEL,
        i18n::text(language, "settings.language"),
    )?;
    set_control_text(
        hwnd,
        ID_CONFIG_PATH_LABEL,
        i18n::text(language, "settings.config_path"),
    )?;
    set_control_text(
        hwnd,
        ID_LOG_PATH_LABEL,
        i18n::text(language, "settings.log_path"),
    )?;
    set_control_text(hwnd, ID_OPEN_CONFIG, i18n::text(language, "settings.open"))?;
    set_control_text(hwnd, ID_OPEN_LOG, i18n::text(language, "settings.open"))?;
    set_control_text(hwnd, ID_SAVE, i18n::text(language, "settings.save"))?;
    set_control_text(hwnd, ID_CLOSE, i18n::text(language, "settings.close"))?;
    set_control_text(
        hwnd,
        ID_BINDINGS_LABEL,
        i18n::text(language, "settings.bindings"),
    )?;
    set_control_text(
        hwnd,
        ID_BINDING_SOURCE_LABEL,
        i18n::text(language, "settings.binding.source"),
    )?;
    set_control_text(hwnd, ID_BINDING_SOURCE_PREFIX, "CapsLock +")?;
    set_control_text(
        hwnd,
        ID_BINDING_ACTION_KIND_LABEL,
        i18n::text(language, "settings.binding.action_kind"),
    )?;
    set_control_text(
        hwnd,
        ID_BINDING_ACTION_VALUE_LABEL,
        i18n::text(language, "settings.binding.action_value"),
    )?;
    set_control_text(
        hwnd,
        ID_BINDING_ACTION_COUNT_LABEL,
        i18n::text(language, "settings.binding.action_count"),
    )?;
    set_control_text(hwnd, ID_BINDING_SOURCE_MODIFIER_ADD, "+")?;
    set_control_text(hwnd, ID_BINDING_ACTION_MODIFIER_ADD, "+")?;
    set_control_text(
        hwnd,
        ID_BINDING_SOURCE_CAPTURE,
        i18n::text(language, "settings.binding.listen_source"),
    )?;
    set_control_text(
        hwnd,
        ID_BINDING_ACTION_CAPTURE,
        i18n::text(language, "settings.binding.listen_target"),
    )?;
    set_control_text(
        hwnd,
        ID_BINDING_ADD,
        i18n::text(language, "settings.binding.add"),
    )?;
    set_control_text(
        hwnd,
        ID_BINDING_UPDATE,
        i18n::text(language, "settings.binding.update"),
    )?;
    set_control_text(
        hwnd,
        ID_BINDING_DELETE,
        i18n::text(language, "settings.binding.delete"),
    )?;

    set_checkbox(hwnd, ID_ENABLED, model.enabled)?;
    set_checkbox(hwnd, ID_START_WITH_WINDOWS, model.start_with_windows)?;
    set_checkbox(hwnd, ID_RUN_AS_ADMIN, model.run_as_admin)?;
    set_checkbox(hwnd, ID_SHOW_TRAY_ICON, model.show_tray_icon)?;
    populate_tap_capslock(hwnd, language, model.tap_capslock)?;
    populate_language(hwnd, language, model.language)?;

    set_control_text(hwnd, ID_CONFIG_PATH, &config_path.to_string_lossy())?;
    set_control_text(hwnd, ID_LOG_PATH, &log_path.to_string_lossy())?;
    refresh_bindings_area(hwnd)?;

    match status_key {
        Some(key) => set_control_text(hwnd, ID_STATUS, i18n::text(language, key))?,
        None => set_control_text(hwnd, ID_STATUS, "")?,
    }

    Ok(())
}

fn refresh_bindings_area(hwnd: HWND) -> Result<(), String> {
    let (model, selected) = current_bindings_view()?;
    let language = model.language;
    populate_binding_list(hwnd, language, &model, selected)?;
    populate_binding_editor(hwnd)
}

fn current_bindings_view() -> Result<(SettingsModel, Option<usize>), String> {
    let mut state = state_holder()
        .lock()
        .map_err(|_| "settings window lock is poisoned".to_string())?;
    let state = state
        .as_mut()
        .ok_or_else(|| "settings window is not initialized".to_string())?;
    let len = state.model.capslock_layer.len();
    if state
        .selected_binding_index
        .map(|index| index >= len)
        .unwrap_or(true)
    {
        state.selected_binding_index = if len == 0 { None } else { Some(0) };
    }
    Ok((state.model.clone(), state.selected_binding_index))
}

fn populate_binding_list(
    hwnd: HWND,
    language: Language,
    model: &SettingsModel,
    selected: Option<usize>,
) -> Result<(), String> {
    let control = control(hwnd, ID_BINDING_LIST)?;
    unsafe {
        SendMessageW(control, LB_RESETCONTENT, 0, 0);
    }

    for row in model.binding_rows() {
        let item = win::to_wide_null(&binding_list_text(language, &row));
        unsafe {
            SendMessageW(control, LB_ADDSTRING, 0, item.as_ptr() as LPARAM);
        }
    }

    let add_item = win::to_wide_null(&binding_add_list_text(language));
    unsafe {
        SendMessageW(control, LB_ADDSTRING, 0, add_item.as_ptr() as LPARAM);
    }

    set_list_selection(hwnd, selected)
}

fn populate_binding_editor(hwnd: HWND) -> Result<(), String> {
    let (model, selected) = current_bindings_view()?;
    let language = model.language;
    let row = selected
        .and_then(|index| model.binding_rows().get(index).cloned())
        .unwrap_or_else(|| {
            KeyBindingRow::new(
                "caps_space",
                KeyBindingActionKind::BuiltIn,
                default_action_value(KeyBindingActionKind::BuiltIn),
            )
        });

    populate_binding_source_editor(hwnd, &row)?;
    populate_binding_action_kind(hwnd, language, row.action_kind)?;
    populate_binding_action_editor(hwnd, language, &row)?;
    update_capture_controls(hwnd)?;
    Ok(())
}

fn populate_binding_source_editor(hwnd: HWND, row: &KeyBindingRow) -> Result<(), String> {
    let parts = combo_parts_from_source(&row.source);
    populate_modifier_combos(hwnd, &SOURCE_MODIFIER_IDS, &parts.modifiers)?;
    refresh_modifier_editor_visibility(hwnd, &SOURCE_MODIFIER_IDS)?;
    populate_combo_values(hwnd, ID_BINDING_SOURCE, KEY_CHOICES, &parts.key)
}

fn populate_binding_action_editor(
    hwnd: HWND,
    _language: Language,
    row: &KeyBindingRow,
) -> Result<(), String> {
    match row.action_kind {
        KeyBindingActionKind::BuiltIn => {
            move_action_value_editor(hwnd, ACTION_VALUE_DEFAULT_Y)?;
            show_combo_modifier_editor(hwnd, &ACTION_MODIFIER_IDS, false)?;
            show_builtin_count_editor(hwnd, true)?;
            let builtin = builtin_editor_value(&row.action_value);
            set_control_text(hwnd, ID_BINDING_ACTION_COUNT, &builtin.count.to_string())?;
            populate_combo_values(
                hwnd,
                ID_BINDING_ACTION_VALUE,
                BUILTIN_ACTION_CHOICES,
                &builtin.name,
            )
        }
        KeyBindingActionKind::KeyTap => {
            move_action_value_editor(hwnd, ACTION_VALUE_DEFAULT_Y)?;
            show_combo_modifier_editor(hwnd, &ACTION_MODIFIER_IDS, false)?;
            show_builtin_count_editor(hwnd, false)?;
            populate_combo_values(
                hwnd,
                ID_BINDING_ACTION_VALUE,
                KEY_CHOICES,
                &row.action_value,
            )
        }
        KeyBindingActionKind::KeyCombo => {
            move_action_value_editor(hwnd, ACTION_VALUE_COMBO_Y)?;
            show_builtin_count_editor(hwnd, false)?;
            let parts = combo_parts_from_suffix(&row.action_value);
            populate_modifier_combos(hwnd, &ACTION_MODIFIER_IDS, &parts.modifiers)?;
            refresh_modifier_editor_visibility(hwnd, &ACTION_MODIFIER_IDS)?;
            populate_combo_values(hwnd, ID_BINDING_ACTION_VALUE, KEY_CHOICES, &parts.key)
        }
    }
}

fn populate_modifier_combos(
    hwnd: HWND,
    ids: &[i32],
    selected_modifiers: &[String],
) -> Result<(), String> {
    for (index, id) in ids.iter().enumerate() {
        let selected = selected_modifiers
            .get(index)
            .map(String::as_str)
            .unwrap_or(NO_MODIFIER_CHOICE);
        populate_combo_values(hwnd, *id, MODIFIER_CHOICES, selected)?;
    }
    Ok(())
}

fn selected_modifier_values(hwnd: HWND, ids: &[i32]) -> Result<Vec<String>, String> {
    let mut modifiers = Vec::new();
    for id in ids {
        let value = modifier_choice_value(&control_text(hwnd, *id)?);
        if !value.is_empty() {
            modifiers.push(value);
        }
    }
    Ok(modifiers)
}

fn add_modifier_to_editor(hwnd: HWND, ids: &[i32]) {
    let result = (|| {
        let mut modifiers = selected_modifier_values(hwnd, ids)?;
        if modifiers.len() < ids.len() {
            modifiers.push(next_modifier_choice(&modifiers).to_string());
        }
        populate_modifier_combos(hwnd, ids, &modifiers)?;
        refresh_modifier_editor_visibility(hwnd, ids)
    })();

    if let Err(error) = result {
        handle_binding_error(hwnd, error);
    }
}

fn normalize_modifier_editor(hwnd: HWND, changed_id: i32) {
    let ids = if SOURCE_MODIFIER_IDS.contains(&changed_id) {
        &SOURCE_MODIFIER_IDS
    } else {
        &ACTION_MODIFIER_IDS
    };
    let result = (|| {
        let modifiers = selected_modifier_values(hwnd, ids)?;
        populate_modifier_combos(hwnd, ids, &modifiers)?;
        refresh_modifier_editor_visibility(hwnd, ids)
    })();

    if let Err(error) = result {
        handle_binding_error(hwnd, error);
    }
}

fn refresh_modifier_editor_visibility(hwnd: HWND, ids: &[i32]) -> Result<(), String> {
    let count = selected_modifier_values(hwnd, ids)?.len();
    for (index, id) in ids.iter().enumerate() {
        let (x, y) = modifier_position(ids, index);
        move_control(hwnd, *id, x, y, 110, COMBO_DROPDOWN_HEIGHT)?;
        show_control(hwnd, *id, index < count)?;
    }

    let add_id = modifier_add_button_id(ids);
    show_control(hwnd, add_id, count < ids.len())?;
    let (x, y) = modifier_position(ids, count);
    move_control(hwnd, add_id, x, y, 28, CONTROL_HEIGHT)
}

fn show_combo_modifier_editor(hwnd: HWND, ids: &[i32], visible: bool) -> Result<(), String> {
    for id in ids {
        show_control(hwnd, *id, visible)?;
    }
    show_control(hwnd, modifier_add_button_id(ids), visible)
}

fn show_builtin_count_editor(hwnd: HWND, visible: bool) -> Result<(), String> {
    show_control(hwnd, ID_BINDING_ACTION_COUNT_LABEL, visible)?;
    show_control(hwnd, ID_BINDING_ACTION_COUNT, visible)
}

fn is_modifier_combo(id: i32) -> bool {
    SOURCE_MODIFIER_IDS.contains(&id) || ACTION_MODIFIER_IDS.contains(&id)
}

fn next_modifier_choice(existing: &[String]) -> &'static str {
    ["ctrl", "alt", "shift", "win"]
        .into_iter()
        .find(|candidate| !existing.iter().any(|value| value == candidate))
        .unwrap_or("ctrl")
}

fn modifier_add_button_id(ids: &[i32]) -> i32 {
    if ids.first() == SOURCE_MODIFIER_IDS.first() {
        ID_BINDING_SOURCE_MODIFIER_ADD
    } else {
        ID_BINDING_ACTION_MODIFIER_ADD
    }
}

fn modifier_position(ids: &[i32], index: usize) -> (i32, i32) {
    let positions = if ids.first() == SOURCE_MODIFIER_IDS.first() {
        &SOURCE_MODIFIER_POSITIONS
    } else {
        &ACTION_MODIFIER_POSITIONS
    };
    positions[index.min(positions.len() - 1)]
}

fn move_action_value_editor(hwnd: HWND, y: i32) -> Result<(), String> {
    move_control(
        hwnd,
        ID_BINDING_ACTION_VALUE,
        500,
        y,
        210,
        COMBO_DROPDOWN_HEIGHT,
    )?;
    move_control(hwnd, ID_BINDING_ACTION_CAPTURE, 720, y, 110, CONTROL_HEIGHT)
}

fn populate_combo_values(
    hwnd: HWND,
    id: i32,
    base_items: &[&str],
    selected: &str,
) -> Result<(), String> {
    let control = control(hwnd, id)?;
    unsafe {
        SendMessageW(control, CB_RESETCONTENT, 0, 0);
    }

    let mut selected_index = 0usize;
    let mut found_selected = false;
    for (index, item) in base_items.iter().enumerate() {
        if item.eq_ignore_ascii_case(selected) {
            selected_index = index;
            found_selected = true;
        }
        let item = win::to_wide_null(item);
        unsafe {
            SendMessageW(control, CB_ADDSTRING, 0, item.as_ptr() as LPARAM);
        }
    }

    if !found_selected && !selected.trim().is_empty() {
        selected_index = base_items.len();
        let item = win::to_wide_null(selected.trim());
        unsafe {
            SendMessageW(control, CB_ADDSTRING, 0, item.as_ptr() as LPARAM);
        }
    }

    unsafe {
        SendMessageW(control, CB_SETCURSEL, selected_index, 0);
    }
    Ok(())
}

fn default_action_value(kind: KeyBindingActionKind) -> &'static str {
    match kind {
        KeyBindingActionKind::BuiltIn => BUILTIN_ACTION_CHOICES[0],
        KeyBindingActionKind::KeyTap | KeyBindingActionKind::KeyCombo => KEY_CHOICES[0],
    }
}
fn populate_binding_action_kind(
    hwnd: HWND,
    language: Language,
    selected: KeyBindingActionKind,
) -> Result<(), String> {
    let items = [
        binding_action_kind_text(language, KeyBindingActionKind::BuiltIn),
        binding_action_kind_text(language, KeyBindingActionKind::KeyTap),
        binding_action_kind_text(language, KeyBindingActionKind::KeyCombo),
    ];
    populate_combo(hwnd, ID_BINDING_ACTION_KIND, &items, selected.combo_index())
}

fn binding_list_text(language: Language, row: &KeyBindingRow) -> String {
    format!(
        "{} | {} | {}",
        row.source,
        binding_action_kind_text(language, row.action_kind),
        row.action_value
    )
}
fn binding_add_list_text(language: Language) -> String {
    format!("+ {}", i18n::text(language, "settings.binding.add"))
}

fn next_default_source(model: &SettingsModel) -> String {
    let existing: Vec<String> = model
        .binding_rows()
        .into_iter()
        .map(|row| row.source.to_ascii_lowercase())
        .collect();
    KEY_CHOICES
        .iter()
        .map(|key| format!("caps_{key}"))
        .find(|source| {
            !existing
                .iter()
                .any(|existing| existing == &source.to_ascii_lowercase())
        })
        .unwrap_or_else(|| "caps_space".to_string())
}

fn binding_action_kind_text(language: Language, kind: KeyBindingActionKind) -> &'static str {
    match kind {
        KeyBindingActionKind::BuiltIn => i18n::text(language, "settings.binding.type.builtin"),
        KeyBindingActionKind::KeyTap => i18n::text(language, "settings.binding.type.key_tap"),
        KeyBindingActionKind::KeyCombo => i18n::text(language, "settings.binding.type.key_combo"),
    }
}

fn selected_binding_index(hwnd: HWND) -> Result<Option<usize>, String> {
    let control = control(hwnd, ID_BINDING_LIST)?;
    let selected = unsafe { SendMessageW(control, LB_GETCURSEL, 0, 0) };
    if selected == LB_ERR {
        Ok(None)
    } else {
        Ok(Some(selected as usize))
    }
}

fn set_list_selection(hwnd: HWND, selected: Option<usize>) -> Result<(), String> {
    let control = control(hwnd, ID_BINDING_LIST)?;
    let index = selected.map(|index| index as WPARAM).unwrap_or(usize::MAX);
    unsafe {
        SendMessageW(control, LB_SETCURSEL, index, 0);
    }
    Ok(())
}

fn state_language() -> Language {
    with_state(|state| state.model.language).unwrap_or_else(|_| app::current_language())
}
fn collect_model(hwnd: HWND) -> Result<SettingsModel, String> {
    commit_current_binding_editor(hwnd)?;
    let mut model = with_state(|state| state.model.clone())?;
    model.enabled = checkbox_checked(hwnd, ID_ENABLED)?;
    model.start_with_windows = checkbox_checked(hwnd, ID_START_WITH_WINDOWS)?;
    model.run_as_admin = checkbox_checked(hwnd, ID_RUN_AS_ADMIN)?;
    model.show_tray_icon = checkbox_checked(hwnd, ID_SHOW_TRAY_ICON)?;
    model.tap_capslock = match combo_index(hwnd, ID_TAP_CAPSLOCK)? {
        0 => TapCapsLock::Toggle,
        1 => TapCapsLock::Escape,
        2 => TapCapsLock::None,
        _ => model.tap_capslock,
    };
    model.language = match combo_index(hwnd, ID_LANGUAGE)? {
        0 => Language::System,
        1 => Language::ZhCn,
        2 => Language::EnUs,
        _ => model.language,
    };
    if let Ok(mut state) = state_holder().lock() {
        if let Some(state) = state.as_mut() {
            state.model = model.clone();
        }
    }
    Ok(model)
}

fn populate_tap_capslock(
    hwnd: HWND,
    language: Language,
    selected: TapCapsLock,
) -> Result<(), String> {
    let items = [
        i18n::text(language, "settings.tap_capslock.toggle"),
        i18n::text(language, "settings.tap_capslock.escape"),
        i18n::text(language, "settings.tap_capslock.none"),
    ];
    let selected_index = match selected {
        TapCapsLock::Toggle => 0,
        TapCapsLock::Escape => 1,
        TapCapsLock::None => 2,
    };
    populate_combo(hwnd, ID_TAP_CAPSLOCK, &items, selected_index)
}

fn populate_language(hwnd: HWND, language: Language, selected: Language) -> Result<(), String> {
    let items = [
        i18n::text(language, "settings.language.system"),
        i18n::text(language, "settings.language.zh_cn"),
        i18n::text(language, "settings.language.en_us"),
    ];
    let selected_index = match selected {
        Language::System => 0,
        Language::ZhCn => 1,
        Language::EnUs => 2,
    };
    populate_combo(hwnd, ID_LANGUAGE, &items, selected_index)
}

fn populate_combo(
    hwnd: HWND,
    id: i32,
    items: &[&str],
    selected_index: usize,
) -> Result<(), String> {
    let control = control(hwnd, id)?;
    unsafe {
        SendMessageW(control, CB_RESETCONTENT, 0, 0);
    }
    for item in items {
        let item = win::to_wide_null(item);
        unsafe {
            SendMessageW(control, CB_ADDSTRING, 0, item.as_ptr() as LPARAM);
        }
    }
    unsafe {
        SendMessageW(control, CB_SETCURSEL, selected_index, 0);
    }
    Ok(())
}

fn set_status(hwnd: HWND, language: Language, key: &str) -> Result<(), String> {
    set_control_text(hwnd, ID_STATUS, i18n::text(language, key))
}

fn create_checkbox(
    hwnd: HWND,
    id: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    create_control(
        hwnd,
        "BUTTON",
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX as u32,
        id,
        x,
        y,
        width,
        height,
    )
}

fn create_button(
    hwnd: HWND,
    id: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    create_control(
        hwnd,
        "BUTTON",
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_PUSHBUTTON as u32,
        id,
        x,
        y,
        width,
        height,
    )
}

fn create_static(
    hwnd: HWND,
    id: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    create_control(
        hwnd,
        "STATIC",
        "",
        WS_CHILD | WS_VISIBLE | SS_LEFT,
        id,
        x,
        y,
        width,
        height,
    )
}

fn create_readonly_edit(
    hwnd: HWND,
    id: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    create_control(
        hwnd,
        "EDIT",
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32 | ES_READONLY as u32,
        id,
        x,
        y,
        width,
        height,
    )
}

fn create_text_edit(
    hwnd: HWND,
    id: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    create_control(
        hwnd,
        "EDIT",
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32,
        id,
        x,
        y,
        width,
        height,
    )
}

fn create_listbox(
    hwnd: HWND,
    id: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    create_control(
        hwnd,
        "LISTBOX",
        "",
        WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | WS_BORDER
            | WS_VSCROLL
            | LBS_NOTIFY
            | LBS_NOINTEGRALHEIGHT,
        id,
        x,
        y,
        width,
        height,
    )
}

fn create_combo(
    hwnd: HWND,
    id: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    create_control(
        hwnd,
        "COMBOBOX",
        "",
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
        id,
        x,
        y,
        width,
        height,
    )
}

fn create_control(
    parent: HWND,
    class_name: &str,
    text: &str,
    style: u32,
    id: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    let class_name = win::to_wide_null(class_name);
    let text = win::to_wide_null(text);
    let control = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            text.as_ptr(),
            style,
            x,
            y,
            width,
            height,
            parent,
            id as isize as HMENU,
            win::module_handle(),
            null(),
        )
    };

    if control.is_null() {
        return Err(last_os_error("failed to create settings control"));
    }

    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    unsafe {
        SendMessageW(control, WM_SETFONT, font as WPARAM, 1);
    }
    Ok(())
}

fn checkbox_checked(hwnd: HWND, id: i32) -> Result<bool, String> {
    let control = control(hwnd, id)?;
    let checked = unsafe { SendMessageW(control, BM_GETCHECK, 0, 0) };
    Ok(checked == BST_CHECKED as isize)
}

fn set_checkbox(hwnd: HWND, id: i32, checked: bool) -> Result<(), String> {
    let control = control(hwnd, id)?;
    let value = if checked { BST_CHECKED } else { BST_UNCHECKED };
    unsafe {
        SendMessageW(control, BM_SETCHECK, value as WPARAM, 0);
    }
    Ok(())
}

fn combo_index(hwnd: HWND, id: i32) -> Result<isize, String> {
    let control = control(hwnd, id)?;
    Ok(unsafe { SendMessageW(control, CB_GETCURSEL, 0, 0) })
}

fn set_window_text(hwnd: HWND, text: &str) {
    let text = win::to_wide_null(text);
    unsafe {
        SetWindowTextW(hwnd, text.as_ptr());
    }
}

fn set_control_text(hwnd: HWND, id: i32, text: &str) -> Result<(), String> {
    let control = control(hwnd, id)?;
    set_window_text(control, text);
    Ok(())
}

fn control_text(hwnd: HWND, id: i32) -> Result<String, String> {
    let control = control(hwnd, id)?;
    let length = unsafe { GetWindowTextLengthW(control) };
    let mut buffer = vec![0u16; length as usize + 1];
    let copied = unsafe { GetWindowTextW(control, buffer.as_mut_ptr(), buffer.len() as i32) };
    Ok(String::from_utf16_lossy(&buffer[..copied as usize]))
}

fn show_control(hwnd: HWND, id: i32, visible: bool) -> Result<(), String> {
    let control = control(hwnd, id)?;
    unsafe {
        ShowWindow(control, if visible { SW_SHOW } else { SW_HIDE });
    }
    Ok(())
}

fn move_control(
    hwnd: HWND,
    id: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    let control = control(hwnd, id)?;
    unsafe {
        MoveWindow(control, x, y, width, height, 1);
    }
    Ok(())
}

fn control(hwnd: HWND, id: i32) -> Result<HWND, String> {
    let control = unsafe { GetDlgItem(hwnd, id) };
    if control.is_null() {
        Err(format!("settings control {id} was not found"))
    } else {
        Ok(control)
    }
}

fn show_settings_error(language: Language, summary_key: &str, detail: &str) {
    win::message_box(
        i18n::text(language, "app.title"),
        &i18n::message_with_detail(language, summary_key, detail),
    );
}

fn show_settings_validation(language: Language, report: &SettingsValidationReport) {
    win::message_box(
        i18n::text(language, "app.title"),
        &report.format_for_language(language),
    );
}

fn log_settings_validation_report(report: &SettingsValidationReport) {
    logging::log_line(format!(
        "settings validation summary expected_mapping_count={} parsed_mapping_count={} issue_count={}",
        report.expected_mapping_count,
        report.parsed_mapping_count,
        report.validation.issues.len()
    ));

    for issue in &report.validation.issues {
        logging::log_line(format!(
            "settings validation issue blocking={} parser_severity={} kind={} line={} section={} key={} value={} message={}",
            settings_issue_blocks_save(issue),
            config_issue_severity_name(issue.severity),
            config_issue_kind_name(issue.kind),
            issue
                .line
                .map(|line| line.to_string())
                .unwrap_or_else(|| "none".to_string()),
            issue.section.as_deref().unwrap_or("none"),
            issue.key.as_deref().unwrap_or("none"),
            issue.value.as_deref().unwrap_or("none"),
            issue.message
        ));
    }
}

fn config_issue_severity_name(severity: ConfigIssueSeverity) -> &'static str {
    match severity {
        ConfigIssueSeverity::Error => "error",
        ConfigIssueSeverity::Warning => "warning",
    }
}

fn config_issue_kind_name(kind: ConfigIssueKind) -> &'static str {
    match kind {
        ConfigIssueKind::Syntax => "syntax",
        ConfigIssueKind::InvalidValue => "invalid_value",
        ConfigIssueKind::InvalidMapping => "invalid_mapping",
        ConfigIssueKind::DuplicateMapping => "duplicate_mapping",
        ConfigIssueKind::UnknownAction => "unknown_action",
    }
}

fn active_window() -> Option<HWND> {
    let mut state = state_holder().lock().ok()?;
    let hwnd = state.as_ref()?.hwnd as HWND;
    if unsafe { IsWindow(hwnd) } != 0 {
        Some(hwnd)
    } else {
        *state = None;
        None
    }
}

fn clear_window_state(hwnd: HWND) {
    if let Ok(mut state) = state_holder().lock() {
        if state
            .as_ref()
            .map(|state| state.hwnd == hwnd as isize)
            .unwrap_or(false)
        {
            *state = None;
        }
    }
}

fn with_state<T>(read: impl FnOnce(&SettingsWindowState) -> T) -> Result<T, String> {
    let state = state_holder()
        .lock()
        .map_err(|_| "settings window lock is poisoned".to_string())?;
    let state = state
        .as_ref()
        .ok_or_else(|| "settings window is not initialized".to_string())?;
    Ok(read(state))
}

fn state_holder() -> &'static Mutex<Option<SettingsWindowState>> {
    WINDOW_STATE.get_or_init(|| Mutex::new(None))
}

fn last_os_error(context: &str) -> String {
    format!("{context}: {}", std::io::Error::last_os_error())
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

    #[test]
    fn settings_model_round_trips_basic_page_fields() {
        let mut config = Config::default();
        let original_mappings = config.capslock_layer.clone();
        let mut model = SettingsModel::from_config(&config);

        model.enabled = false;
        model.start_with_windows = true;
        model.run_as_admin = true;
        model.show_tray_icon = false;
        model.tap_capslock = TapCapsLock::Escape;
        model.language = Language::ZhCn;
        model.apply_to_config(&mut config);

        assert!(!config.general.enabled);
        assert!(config.general.start_with_windows);
        assert!(config.general.run_as_admin);
        assert!(!config.general.show_tray_icon);
        assert_eq!(config.general.tap_capslock, TapCapsLock::Escape);
        assert_eq!(config.ui.language, Language::ZhCn);
        assert_eq!(config.capslock_layer, original_mappings);
    }

    #[test]
    fn settings_model_adds_updates_and_deletes_binding_rows() {
        let mut model = SettingsModel::from_config(&Config::default());
        let original_len = model.capslock_layer.len();

        model
            .add_binding_row(KeyBindingRow::new(
                "caps_r",
                KeyBindingActionKind::KeyTap,
                "f5",
            ))
            .unwrap();

        assert_eq!(model.capslock_layer.len(), original_len + 1);
        assert_eq!(
            model.binding_rows().last().unwrap(),
            &KeyBindingRow::new("caps_r", KeyBindingActionKind::KeyTap, "f5")
        );

        let index = model.capslock_layer.len() - 1;
        model
            .update_binding_row(
                index,
                KeyBindingRow::new(
                    "caps_lalt_shift_j",
                    KeyBindingActionKind::KeyCombo,
                    "ctrl_c",
                ),
            )
            .unwrap();

        assert_eq!(
            model.binding_rows().get(index).unwrap(),
            &KeyBindingRow::new(
                "caps_lalt_shift_j",
                KeyBindingActionKind::KeyCombo,
                "ctrl_c"
            )
        );

        model.delete_binding_row(index).unwrap();
        assert_eq!(model.capslock_layer.len(), original_len);
    }

    #[test]
    fn settings_model_binding_rows_round_trip_through_config_ini() {
        let mut model = SettingsModel::from_config(&Config::default());
        let rows = vec![
            KeyBindingRow::new("caps_h", KeyBindingActionKind::BuiltIn, "moveLeft"),
            KeyBindingRow::new("caps_r", KeyBindingActionKind::KeyTap, "f5"),
            KeyBindingRow::new("caps_c", KeyBindingActionKind::KeyCombo, "ctrl_c"),
        ];

        model.replace_binding_rows(rows.clone()).unwrap();

        let mut config = Config::default();
        model.apply_to_config(&mut config);
        let ini = config.to_ini_string();
        let reparsed = Config::from_ini(&ini).unwrap();
        let reparsed_model = SettingsModel::from_config(&reparsed);

        assert!(ini.contains("caps_h=keyFunc_moveLeft"));
        assert!(ini.contains("caps_r=keyTarget_f5"));
        assert!(ini.contains("caps_c=keyCombo_ctrl_c"));
        assert_eq!(reparsed_model.binding_rows(), rows);
    }

    #[test]
    fn gui_save_keeps_recognized_manual_ini_fields_and_mappings() {
        let mut config = Config::from_ini(
            r#"
            [general]
            enabled = false
            start_with_windows = true
            run_as_admin = false
            show_tray_icon = false
            tap_capslock = escape

            [Keys]
            caps_h=keyFunc_moveLeft
            caps_r=keyTarget_f5
            caps_c=keyCombo_ctrl_c

            [ui]
            language = zh-CN
            settings_backend = IniBackendWithCASE
            settings_page = ManualPageWithCASE
            "#,
        )
        .unwrap();

        let mut model = SettingsModel::from_config(&config);
        model.enabled = true;
        model
            .add_binding_row(KeyBindingRow::new(
                "caps_lalt_d",
                KeyBindingActionKind::BuiltIn,
                "moveWordRight",
            ))
            .unwrap();
        model.apply_to_config(&mut config);

        let saved = config.to_ini_string();
        let reparsed = Config::from_ini(&saved).unwrap();
        let reparsed_model = SettingsModel::from_config(&reparsed);

        assert!(reparsed.general.enabled);
        assert!(reparsed.general.start_with_windows);
        assert!(!reparsed.general.run_as_admin);
        assert!(!reparsed.general.show_tray_icon);
        assert_eq!(reparsed.general.tap_capslock, TapCapsLock::Escape);
        assert_eq!(reparsed.ui.language, Language::ZhCn);
        assert_eq!(reparsed.ui.settings_backend, "IniBackendWithCASE");
        assert_eq!(reparsed.ui.settings_page, "ManualPageWithCASE");

        let rows = reparsed_model.binding_rows();
        for expected in [
            KeyBindingRow::new("caps_h", KeyBindingActionKind::BuiltIn, "moveLeft"),
            KeyBindingRow::new("caps_r", KeyBindingActionKind::KeyTap, "f5"),
            KeyBindingRow::new("caps_c", KeyBindingActionKind::KeyCombo, "ctrl_c"),
            KeyBindingRow::new(
                "caps_lalt_d",
                KeyBindingActionKind::BuiltIn,
                "moveWordRight",
            ),
        ] {
            assert!(rows.contains(&expected), "missing row: {expected:?}");
        }
    }
    #[test]
    fn combo_dropdown_parts_build_normalized_binding_values() {
        let source_modifiers = vec![String::new()];
        let action_modifiers = vec!["ctrl".to_string()];

        assert_eq!(
            caps_source_from_parts(&source_modifiers, "space").unwrap(),
            "caps_space"
        );
        assert_eq!(
            normalized_combo_suffix_from_parts(&action_modifiers, "space").unwrap(),
            "ctrl_space"
        );
    }

    #[test]
    fn target_combo_layout_places_modifiers_before_final_key() {
        assert!(ACTION_MODIFIER_POSITIONS
            .iter()
            .all(|(_, y)| *y < ACTION_VALUE_COMBO_Y));
    }

    #[test]
    fn builtin_dropdown_value_round_trips_custom_count() {
        assert_eq!(
            builtin_action_value("moveLeft", "12").unwrap(),
            "moveLeft(12)"
        );
        assert_eq!(builtin_action_value("moveLeft", "1").unwrap(), "moveLeft");

        let parsed = builtin_editor_value("moveDown(7)");
        assert_eq!(parsed.name, "moveDown");
        assert_eq!(parsed.count, 7);
    }

    #[test]
    fn binding_save_validation_reports_duplicate_and_invalid_input_combo() {
        let rows = vec![
            KeyBindingRow::new("caps_h", KeyBindingActionKind::BuiltIn, "moveLeft"),
            KeyBindingRow::new("caps_h", KeyBindingActionKind::BuiltIn, "moveRight"),
            KeyBindingRow::new("caps_lctrl_ctrl_j", KeyBindingActionKind::BuiltIn, "moveUp"),
        ];

        let report = validate_binding_rows_for_save(&rows).unwrap();

        assert!(report.has_errors());
        assert!(has_validation_issue(
            &report,
            ConfigIssueKind::DuplicateMapping
        ));
        assert!(has_validation_issue(
            &report,
            ConfigIssueKind::InvalidMapping
        ));
        assert!(report
            .format_for_language(Language::ZhCn)
            .contains("重复映射"));
        assert!(report
            .format_for_language(Language::ZhCn)
            .contains("非法输入组合"));
    }

    #[test]
    fn binding_save_validation_reports_unknown_actions_by_type() {
        let rows = vec![
            KeyBindingRow::new("caps_j", KeyBindingActionKind::BuiltIn, "noSuchAction"),
            KeyBindingRow::new("caps_r", KeyBindingActionKind::KeyTap, "no_such_key"),
            KeyBindingRow::new("caps_c", KeyBindingActionKind::KeyCombo, "ctrl_no_such_key"),
            KeyBindingRow::new("caps_d", KeyBindingActionKind::BuiltIn, "moveLeft(nope)"),
        ];

        let report = validate_binding_rows_for_save(&rows).unwrap();
        let formatted = report.format_for_language(Language::EnUs);

        assert!(report.has_errors());
        assert_eq!(
            report
                .validation
                .issues
                .iter()
                .filter(|issue| issue.kind == ConfigIssueKind::UnknownAction)
                .count(),
            4
        );
        assert!(formatted.contains("Unknown action"));
        assert!(formatted.contains("caps_j=keyFunc_noSuchAction"));
        assert!(formatted.contains("caps_r=keyTarget_no_such_key"));
        assert!(formatted.contains("caps_c=keyCombo_ctrl_no_such_key"));
        assert!(formatted.contains("invalid key function count"));
    }

    #[test]
    fn settings_model_save_validation_rejects_error_config() {
        let mapping = Config::from_ini("[Keys]\ncaps_h=keyFunc_moveLeft\n")
            .unwrap()
            .capslock_layer
            .remove(0);
        let mut model = SettingsModel::from_config(&Config::default());
        model.capslock_layer = vec![mapping.clone(), mapping];

        let report = model.validate_for_save();
        let formatted = report.format_for_language(Language::ZhCn);

        assert!(report.has_errors());
        assert_eq!(report.expected_mapping_count, 2);
        assert_eq!(report.parsed_mapping_count, 1);
        assert!(has_validation_issue(
            &report,
            ConfigIssueKind::DuplicateMapping
        ));
        assert!(formatted.contains("校验失败"));
        assert!(formatted.contains("caps_h=keyFunc_moveLeft"));
    }

    #[test]
    fn default_added_binding_uses_first_free_key_choice() {
        let model = SettingsModel::from_config(&Config::default());

        assert_ne!(next_default_source(&model), "caps_space");
    }

    fn has_validation_issue(report: &SettingsValidationReport, kind: ConfigIssueKind) -> bool {
        report
            .validation
            .issues
            .iter()
            .any(|issue| issue.kind == kind)
    }
}
