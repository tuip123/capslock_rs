use std::mem::zeroed;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT, HBRUSH};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetDlgItem, IsWindow, LoadCursorW,
    RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowTextW, ShowWindow, BS_AUTOCHECKBOX,
    BS_PUSHBUTTON, CBS_DROPDOWNLIST, CB_ADDSTRING, CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL,
    CW_USEDEFAULT, ES_AUTOHSCROLL, ES_READONLY, HMENU, IDC_ARROW, SW_SHOW, WM_CLOSE, WM_COMMAND,
    WM_DESTROY, WM_SETFONT, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_MINIMIZEBOX,
    WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use crate::config::{Config, KeyMapping, Language, TapCapsLock};
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

struct SettingsWindowState {
    hwnd: isize,
    model: SettingsModel,
    config_path: PathBuf,
    log_path: PathBuf,
}

const CLASS_NAME: &str = "CapsLockRSSettingsWindow";
const WINDOW_WIDTH: i32 = 600;
const WINDOW_HEIGHT: i32 = 430;

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

const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;
const BST_CHECKED: u32 = 1;
const BST_UNCHECKED: u32 = 0;
const COLOR_WINDOW: i32 = 5;
const SS_LEFT: u32 = 0;

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
            handle_command(hwnd, (w_param & 0xffff) as i32);
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

fn handle_command(hwnd: HWND, command: i32) {
    match command {
        ID_SAVE => save_from_window(hwnd),
        ID_CLOSE => unsafe {
            DestroyWindow(hwnd);
        },
        ID_OPEN_CONFIG => app::open_config(),
        ID_OPEN_LOG => app::open_log(),
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
                    *state = Some(SettingsWindowState {
                        hwnd: hwnd as isize,
                        model: snapshot.model,
                        config_path: snapshot.config_path,
                        log_path: snapshot.log_path,
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

    create_static(hwnd, ID_TAP_CAPSLOCK_LABEL, 18, 150, 150, 22)?;
    create_combo(hwnd, ID_TAP_CAPSLOCK, 178, 146, 180, 160)?;

    create_static(hwnd, ID_LANGUAGE_LABEL, 18, 186, 150, 22)?;
    create_combo(hwnd, ID_LANGUAGE, 178, 182, 180, 160)?;

    create_static(hwnd, ID_CONFIG_PATH_LABEL, 18, 226, 420, 22)?;
    create_readonly_edit(hwnd, ID_CONFIG_PATH, 18, 252, 452, 24)?;
    create_button(hwnd, ID_OPEN_CONFIG, 482, 251, 82, 26)?;

    create_static(hwnd, ID_LOG_PATH_LABEL, 18, 290, 420, 22)?;
    create_readonly_edit(hwnd, ID_LOG_PATH, 18, 316, 452, 24)?;
    create_button(hwnd, ID_OPEN_LOG, 482, 315, 82, 26)?;

    create_static(hwnd, ID_STATUS, 18, 358, 300, 24)?;
    create_button(hwnd, ID_SAVE, 372, 356, 88, 28)?;
    create_button(hwnd, ID_CLOSE, 476, 356, 88, 28)?;
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

    set_checkbox(hwnd, ID_ENABLED, model.enabled)?;
    set_checkbox(hwnd, ID_START_WITH_WINDOWS, model.start_with_windows)?;
    set_checkbox(hwnd, ID_RUN_AS_ADMIN, model.run_as_admin)?;
    set_checkbox(hwnd, ID_SHOW_TRAY_ICON, model.show_tray_icon)?;
    populate_tap_capslock(hwnd, language, model.tap_capslock)?;
    populate_language(hwnd, language, model.language)?;

    set_control_text(hwnd, ID_CONFIG_PATH, &config_path.to_string_lossy())?;
    set_control_text(hwnd, ID_LOG_PATH, &log_path.to_string_lossy())?;

    match status_key {
        Some(key) => set_control_text(hwnd, ID_STATUS, i18n::text(language, key))?,
        None => set_control_text(hwnd, ID_STATUS, "")?,
    }

    Ok(())
}

fn collect_model(hwnd: HWND) -> Result<SettingsModel, String> {
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
}
