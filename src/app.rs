use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::thread;

use windows_sys::Win32::Foundation::HWND;

use crate::actions;
use crate::config::{Config, ConfigPaths, Language};
use crate::gui_settings::SettingsModel;
use crate::hook::{HookState, KeyCaptureMode, KeyCaptureOutcome, KeyboardHook};
use crate::{gui_settings, i18n, logging, startup, tray, win};

pub static APP_CONTEXT: OnceLock<AppContext> = OnceLock::new();

pub struct AppContext {
    pub config_path: PathBuf,
    pub log_path: PathBuf,
    pub message_hwnd: Mutex<isize>,
    pub runtime: Mutex<RuntimeState>,
    pub hook_state: Mutex<HookState>,
}

pub struct RuntimeState {
    pub config: Config,
}

pub struct SettingsSnapshot {
    pub model: SettingsModel,
    pub config_path: PathBuf,
    pub log_path: PathBuf,
}

pub fn run() -> Result<(), String> {
    let paths = ConfigPaths::resolve()?;
    logging::init(paths.log_path.clone())?;
    logging::log_line("starting CapsLock RS");

    let config = Config::load_or_create(&paths.config_path)?;
    if config.general.run_as_admin && !win::is_user_admin() {
        // Elevate before installing the global hook so the normal instance never stays resident.
        logging::log_line("run_as_admin=true, relaunching with UAC elevation");
        win::relaunch_as_admin()?;
        return Ok(());
    }

    let _instance = win::SingleInstance::acquire("Local\\CapsLockRS")?;
    startup::apply_startup(config.general.start_with_windows)?;

    let (action_sender, action_receiver) = mpsc::channel();
    thread::Builder::new()
        .name("capslock-rs-actions".to_string())
        .spawn(move || actions::run_action_worker(action_receiver))
        .map_err(|error| format!("failed to start action worker: {error}"))?;

    let show_tray_icon = config.general.show_tray_icon;
    APP_CONTEXT
        .set(AppContext {
            config_path: paths.config_path.clone(),
            log_path: paths.log_path.clone(),
            message_hwnd: Mutex::new(0),
            hook_state: Mutex::new(HookState::from_config(&config, action_sender.clone())),
            runtime: Mutex::new(RuntimeState { config }),
        })
        .map_err(|_| "failed to initialize app context".to_string())?;

    let _hook = KeyboardHook::install()?;
    let hwnd = tray::create_message_window()?;
    set_message_hwnd(hwnd)?;
    tray::sync_icon(hwnd, show_tray_icon)?;

    logging::log_line(format!(
        "ready config={} log={}",
        paths.config_path.display(),
        paths.log_path.display()
    ));

    win::message_loop();
    tray::remove_icon();
    logging::log_line("exiting CapsLock RS");
    Ok(())
}

pub fn current_config() -> Result<Config, String> {
    let context = APP_CONTEXT
        .get()
        .ok_or_else(|| "app context is not initialized".to_string())?;
    let runtime = context
        .runtime
        .lock()
        .map_err(|_| "runtime state lock is poisoned".to_string())?;
    Ok(runtime.config.clone())
}

pub fn settings_snapshot() -> Result<SettingsSnapshot, String> {
    let context = APP_CONTEXT
        .get()
        .ok_or_else(|| "app context is not initialized".to_string())?;
    let runtime_config = {
        let runtime = context
            .runtime
            .lock()
            .map_err(|_| "runtime state lock is poisoned".to_string())?;
        runtime.config.clone()
    };

    let model = match Config::load_with_validation(&context.config_path) {
        Ok(result) => SettingsModel::from_parse_result(&result),
        Err(error) => {
            logging::log_line(format!(
                "failed to load settings validation snapshot: {error}"
            ));
            SettingsModel::from_config(&runtime_config)
        }
    };

    Ok(SettingsSnapshot {
        model,
        config_path: context.config_path.clone(),
        log_path: context.log_path.clone(),
    })
}

pub fn save_settings_model(model: &SettingsModel) -> Result<(), String> {
    let context = APP_CONTEXT
        .get()
        .ok_or_else(|| "app context is not initialized".to_string())?;

    // Round-trip editable rows through the parser before writing so GUI rules cannot drift.
    let validation = model.validate_for_save();
    if validation.has_errors() {
        return Err(validation.format_for_language(model.language));
    }

    let mut config = current_config()?;
    model.apply_to_config(&mut config);
    config.save(&context.config_path)?;
    reload_config();
    Ok(())
}

pub fn is_enabled() -> bool {
    let Some(context) = APP_CONTEXT.get() else {
        return false;
    };
    let Ok(hook_state) = context.hook_state.lock() else {
        return false;
    };
    hook_state.enabled()
}

pub fn start_with_windows() -> bool {
    current_config()
        .map(|config| config.general.start_with_windows)
        .unwrap_or(false)
}

pub fn current_language() -> Language {
    current_config()
        .map(|config| config.ui.language)
        .unwrap_or(Language::System)
}

pub fn message_hwnd() -> Option<HWND> {
    let context = APP_CONTEXT.get()?;
    let hwnd = context.message_hwnd.lock().ok()?;
    if *hwnd == 0 {
        None
    } else {
        Some(*hwnd as HWND)
    }
}

pub fn begin_key_capture(hwnd: HWND, mode: KeyCaptureMode, message_id: u32) -> Result<(), String> {
    let context = APP_CONTEXT
        .get()
        .ok_or_else(|| "app context is not initialized".to_string())?;
    let mut hook_state = context
        .hook_state
        .lock()
        .map_err(|_| "hook state lock is poisoned".to_string())?;
    hook_state.begin_key_capture(hwnd as isize, mode, message_id);
    Ok(())
}

pub fn cancel_key_capture() -> Result<(), String> {
    let context = APP_CONTEXT
        .get()
        .ok_or_else(|| "app context is not initialized".to_string())?;
    let mut hook_state = context
        .hook_state
        .lock()
        .map_err(|_| "hook state lock is poisoned".to_string())?;
    hook_state.cancel_key_capture();
    Ok(())
}

pub fn take_key_capture_result() -> Option<KeyCaptureOutcome> {
    let context = APP_CONTEXT.get()?;
    let mut hook_state = context.hook_state.lock().ok()?;
    hook_state.take_key_capture_result()
}

pub fn toggle_enabled() {
    let Some(context) = APP_CONTEXT.get() else {
        return;
    };
    let Ok(mut hook_state) = context.hook_state.lock() else {
        logging::log_line("failed to toggle enabled state: hook state lock is poisoned");
        return;
    };

    let enabled = !hook_state.enabled();
    hook_state.set_enabled(enabled);
    logging::log_line(format!("runtime enabled={enabled}"));
}

pub fn reload_config() {
    let Some(context) = APP_CONTEXT.get() else {
        return;
    };

    let fallback_language = current_language();
    match Config::load(&context.config_path) {
        Ok(config) => {
            let language = config.ui.language;
            if config.general.run_as_admin && !win::is_user_admin() {
                logging::log_line("run_as_admin=true after reload, relaunching with UAC elevation");
                match win::relaunch_as_admin() {
                    Ok(()) => win::quit_message_loop(),
                    Err(error) => {
                        logging::log_line(format!("failed to relaunch as admin: {error}"));
                        show_error_message(language, "error.relaunch_as_admin_failed", &error);
                    }
                }
                return;
            }

            if let Err(error) = startup::apply_startup(config.general.start_with_windows) {
                logging::log_line(format!("failed to apply startup setting: {error}"));
                show_error_message(language, "error.update_startup_failed", &error);
            }

            if let Some(hwnd) = message_hwnd() {
                if let Err(error) = tray::sync_icon(hwnd, config.general.show_tray_icon) {
                    logging::log_line(format!("failed to apply tray icon setting: {error}"));
                    show_error_message(language, "error.update_tray_icon_failed", &error);
                }
            }

            if let Ok(mut hook_state) = context.hook_state.lock() {
                hook_state.apply_config(&config);
            } else {
                logging::log_line("failed to reload hook state: lock is poisoned");
            }

            if let Ok(mut runtime) = context.runtime.lock() {
                runtime.config = config;
            } else {
                logging::log_line("failed to reload runtime config: lock is poisoned");
            }

            logging::log_line("config reloaded");
        }
        Err(error) => {
            logging::log_line(format!("failed to reload config: {error}"));
            show_error_message(fallback_language, "error.reload_failed", &error);
        }
    }
}

pub fn toggle_startup() {
    let Some(context) = APP_CONTEXT.get() else {
        return;
    };

    let Ok(mut runtime) = context.runtime.lock() else {
        logging::log_line("failed to toggle startup: runtime state lock is poisoned");
        return;
    };

    runtime.config.general.start_with_windows = !runtime.config.general.start_with_windows;
    let language = runtime.config.ui.language;
    if let Err(error) = runtime.config.save(&context.config_path) {
        logging::log_line(format!("failed to save startup config: {error}"));
        show_error_message(language, "error.save_config_failed", &error);
        return;
    }

    if let Err(error) = startup::apply_startup(runtime.config.general.start_with_windows) {
        logging::log_line(format!("failed to apply startup setting: {error}"));
        show_error_message(language, "error.update_startup_failed", &error);
        return;
    }

    logging::log_line(format!(
        "start_with_windows={}",
        runtime.config.general.start_with_windows
    ));
}

fn show_error_message(language: Language, summary_key: &str, detail: &str) {
    win::message_box(
        i18n::text(language, "app.title"),
        &i18n::message_with_detail(language, summary_key, detail),
    );
}

pub fn open_config() {
    let Some(context) = APP_CONTEXT.get() else {
        return;
    };
    win::open_path(&context.config_path);
}

pub fn open_log() {
    let Some(context) = APP_CONTEXT.get() else {
        return;
    };
    win::open_path(&context.log_path);
}

pub fn open_settings() {
    if let Err(error) = gui_settings::open() {
        logging::log_line(format!("failed to open settings window: {error}"));
        show_error_message(current_language(), "error.open_settings_failed", &error);
    }
}

fn set_message_hwnd(hwnd: HWND) -> Result<(), String> {
    let context = APP_CONTEXT
        .get()
        .ok_or_else(|| "app context is not initialized".to_string())?;
    let mut stored = context
        .message_hwnd
        .lock()
        .map_err(|_| "message window lock is poisoned".to_string())?;
    *stored = hwnd as isize;
    Ok(())
}
