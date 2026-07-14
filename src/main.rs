#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("capslock_rs currently targets Windows only.");

mod actions;
mod app;
mod config;
mod gui_settings;
mod hook;
mod keys;
mod logging;
mod startup;
mod tray;
mod win;

fn main() {
    if let Err(error) = app::run() {
        logging::log_line(format!("fatal: {error}"));
        win::message_box("CapsLock RS", &error);
    }
}
