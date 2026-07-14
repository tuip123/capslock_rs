use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::thread;

use crate::actions;
use crate::config::{Config, ConfigPaths};
use crate::hook::{HookState, KeyboardHook};
use crate::{logging, startup, tray, win};

pub static APP_CONTEXT: OnceLock<AppContext> = OnceLock::new();

pub struct AppContext {
    pub config_path: PathBuf,
    pub log_path: PathBuf,
    pub runtime: Mutex<RuntimeState>,
    pub hook_state: Mutex<HookState>,
}

pub struct RuntimeState {
    pub config: Config,
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

    APP_CONTEXT
        .set(AppContext {
            config_path: paths.config_path.clone(),
            log_path: paths.log_path.clone(),
            hook_state: Mutex::new(HookState::from_config(&config, action_sender.clone())),
            runtime: Mutex::new(RuntimeState { config }),
        })
        .map_err(|_| "failed to initialize app context".to_string())?;

    let _hook = KeyboardHook::install()?;
    let hwnd = tray::create_message_window()?;
    let _tray_icon = if current_config()?.general.show_tray_icon {
        Some(tray::TrayIcon::install(hwnd)?)
    } else {
        None
    };

    logging::log_line(format!(
        "ready config={} log={}",
        paths.config_path.display(),
        paths.log_path.display()
    ));

    win::message_loop();
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

    match Config::load(&context.config_path) {
        Ok(config) => {
            if config.general.run_as_admin && !win::is_user_admin() {
                logging::log_line("run_as_admin=true after reload, relaunching with UAC elevation");
                match win::relaunch_as_admin() {
                    Ok(()) => win::quit_message_loop(),
                    Err(error) => {
                        logging::log_line(format!("failed to relaunch as admin: {error}"));
                        win::message_box(
                            "CapsLock RS",
                            &format!("Failed to relaunch as admin:\n{error}"),
                        );
                    }
                }
                return;
            }

            if let Err(error) = startup::apply_startup(config.general.start_with_windows) {
                logging::log_line(format!("failed to apply startup setting: {error}"));
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
            win::message_box("CapsLock RS", &format!("Reload failed:\n{error}"));
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
    if let Err(error) = runtime.config.save(&context.config_path) {
        logging::log_line(format!("failed to save startup config: {error}"));
        win::message_box("CapsLock RS", &format!("Failed to save config:\n{error}"));
        return;
    }

    if let Err(error) = startup::apply_startup(runtime.config.general.start_with_windows) {
        logging::log_line(format!("failed to apply startup setting: {error}"));
        win::message_box(
            "CapsLock RS",
            &format!("Failed to update startup:\n{error}"),
        );
        return;
    }

    logging::log_line(format!(
        "start_with_windows={}",
        runtime.config.general.start_with_windows
    ));
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
