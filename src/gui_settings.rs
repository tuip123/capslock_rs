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
    GetWindowTextW, IsWindow, LoadCursorW, RegisterClassW, SendMessageW, SetForegroundWindow,
    SetWindowTextW, ShowWindow, BS_AUTOCHECKBOX, BS_PUSHBUTTON, CBS_DROPDOWNLIST, CB_ADDSTRING,
    CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL, CW_USEDEFAULT, ES_AUTOHSCROLL, ES_READONLY, HMENU,
    IDC_ARROW, SW_SHOW, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_SETFONT, WNDCLASSW, WS_BORDER,
    WS_CAPTION, WS_CHILD, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use crate::config::{Config, ConfigIssue, KeyMapping, Language, LayerAction, TapCapsLock};
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

struct SettingsWindowState {
    hwnd: isize,
    model: SettingsModel,
    config_path: PathBuf,
    log_path: PathBuf,
    selected_binding_index: Option<usize>,
}

const CLASS_NAME: &str = "CapsLockRSSettingsWindow";
const WINDOW_WIDTH: i32 = 760;
const WINDOW_HEIGHT: i32 = 700;

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
const WS_VSCROLL: u32 = 0x00200000;

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

    let mut content = String::from("[Keys]\n");
    for row in rows {
        content.push_str(&row.source_ini_key()?);
        content.push('=');
        content.push_str(&row.action_ini_value()?);
        content.push('\n');
    }

    // Use the real config parser so GUI validation cannot drift from INI semantics.
    let parsed = Config::from_ini_with_validation(&content);
    if !parsed.validation.issues.is_empty() {
        return Err(format_config_issues(&parsed.validation.issues));
    }
    if parsed.config.capslock_layer.len() != rows.len() {
        return Err("some binding rows were ignored by the config parser".to_string());
    }

    Ok(parsed.config.capslock_layer)
}

fn format_config_issues(issues: &[ConfigIssue]) -> String {
    let messages: Vec<String> = issues
        .iter()
        .map(|issue| match issue.line {
            Some(line) => format!("line {line}: {}", issue.message),
            None => issue.message.clone(),
        })
        .collect();
    messages.join("; ")
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
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
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
        ID_BINDING_ADD => add_binding_from_window(hwnd),
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
                    });
                }
                let _ = refresh_window(hwnd, Some("settings.saved"));
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

fn add_binding_from_window(hwnd: HWND) {
    match add_binding_from_window_result(hwnd) {
        Ok(()) => {
            let _ = refresh_bindings_area(hwnd);
            let _ = set_status(hwnd, state_language(), "settings.binding_added");
        }
        Err(error) => handle_binding_error(hwnd, error),
    }
}

fn add_binding_from_window_result(hwnd: HWND) -> Result<(), String> {
    let row = collect_binding_editor_row(hwnd)?;
    let mut state = state_holder()
        .lock()
        .map_err(|_| "settings window lock is poisoned".to_string())?;
    let state = state
        .as_mut()
        .ok_or_else(|| "settings window is not initialized".to_string())?;
    state.model.add_binding_row(row)?;
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
        let new_index = selected_binding_index(hwnd)?;
        let old_index = with_state(|state| state.selected_binding_index)?;
        if old_index != new_index {
            commit_binding_editor_for_index(hwnd, old_index)?;
            set_selected_binding_index(new_index)?;
        }
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

fn collect_binding_editor_row(hwnd: HWND) -> Result<KeyBindingRow, String> {
    Ok(KeyBindingRow::new(
        control_text(hwnd, ID_BINDING_SOURCE)?,
        KeyBindingActionKind::from_combo_index(combo_index(hwnd, ID_BINDING_ACTION_KIND)?)?,
        control_text(hwnd, ID_BINDING_ACTION_VALUE)?,
    ))
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

    create_static(hwnd, ID_TAP_CAPSLOCK_LABEL, 386, 20, 130, 22)?;
    create_combo(hwnd, ID_TAP_CAPSLOCK, 522, 16, 190, 160)?;

    create_static(hwnd, ID_LANGUAGE_LABEL, 386, 60, 130, 22)?;
    create_combo(hwnd, ID_LANGUAGE, 522, 56, 190, 160)?;

    create_static(hwnd, ID_CONFIG_PATH_LABEL, 18, 148, 620, 22)?;
    create_readonly_edit(hwnd, ID_CONFIG_PATH, 18, 174, 592, 24)?;
    create_button(hwnd, ID_OPEN_CONFIG, 624, 173, 88, 26)?;

    create_static(hwnd, ID_LOG_PATH_LABEL, 18, 212, 620, 22)?;
    create_readonly_edit(hwnd, ID_LOG_PATH, 18, 238, 592, 24)?;
    create_button(hwnd, ID_OPEN_LOG, 624, 237, 88, 26)?;

    create_static(hwnd, ID_BINDINGS_LABEL, 18, 286, 420, 22)?;
    create_listbox(hwnd, ID_BINDING_LIST, 18, 314, 428, 280)?;

    create_static(hwnd, ID_BINDING_SOURCE_LABEL, 466, 314, 246, 22)?;
    create_text_edit(hwnd, ID_BINDING_SOURCE, 466, 340, 246, 24)?;

    create_static(hwnd, ID_BINDING_ACTION_KIND_LABEL, 466, 378, 246, 22)?;
    create_combo(hwnd, ID_BINDING_ACTION_KIND, 466, 404, 246, 160)?;

    create_static(hwnd, ID_BINDING_ACTION_VALUE_LABEL, 466, 442, 246, 22)?;
    create_text_edit(hwnd, ID_BINDING_ACTION_VALUE, 466, 468, 246, 24)?;

    create_button(hwnd, ID_BINDING_UPDATE, 466, 518, 76, 28)?;
    create_button(hwnd, ID_BINDING_ADD, 552, 518, 76, 28)?;
    create_button(hwnd, ID_BINDING_DELETE, 636, 518, 76, 28)?;

    create_static(hwnd, ID_STATUS, 18, 624, 420, 24)?;
    create_button(hwnd, ID_SAVE, 524, 622, 88, 28)?;
    create_button(hwnd, ID_CLOSE, 624, 622, 88, 28)?;
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

    set_list_selection(hwnd, selected)
}

fn populate_binding_editor(hwnd: HWND) -> Result<(), String> {
    let (model, selected) = current_bindings_view()?;
    let language = model.language;
    let row = selected
        .and_then(|index| model.binding_rows().get(index).cloned())
        .unwrap_or_else(|| KeyBindingRow::new("", KeyBindingActionKind::BuiltIn, ""));

    set_control_text(hwnd, ID_BINDING_SOURCE, &row.source)?;
    populate_binding_action_kind(hwnd, language, row.action_kind)?;
    set_control_text(hwnd, ID_BINDING_ACTION_VALUE, &row.action_value)?;
    Ok(())
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
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | CBS_DROPDOWNLIST as u32,
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
}
