use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetCursorPos, GetMessageW, LoadIconW, PostQuitMessage, RegisterClassW, SetForegroundWindow,
    TrackPopupMenu, TranslateMessage, HMENU, IDI_APPLICATION, MF_CHECKED, MF_GRAYED, MF_SEPARATOR,
    MF_STRING, MSG, TPM_RIGHTBUTTON, WM_APP, WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP,
    WNDCLASSW,
};

use crate::{app, logging, win};

const CLASS_NAME: &str = "CapsLockRSMessageWindow";
const WM_TRAYICON: u32 = WM_APP + 1;

const MENU_TOGGLE_ENABLED: usize = 1001;
const MENU_TOGGLE_STARTUP: usize = 1002;
const MENU_RELOAD_CONFIG: usize = 1003;
const MENU_OPEN_CONFIG: usize = 1004;
const MENU_OPEN_LOG: usize = 1005;
const MENU_SETTINGS: usize = 1006;
const MENU_EXIT: usize = 1007;

pub struct TrayIcon {
    hwnd: HWND,
}

pub fn create_message_window() -> Result<HWND, String> {
    let class_name = win::to_wide_null(CLASS_NAME);
    let window_name = win::to_wide_null("CapsLock RS");
    let instance = win::module_handle();

    let window_class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..unsafe { zeroed() }
    };

    let atom = unsafe { RegisterClassW(&window_class) };
    if atom == 0 {
        return Err("failed to register message window class".to_string());
    }

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        )
    };

    if hwnd.is_null() {
        return Err("failed to create message window".to_string());
    }

    Ok(hwnd)
}

impl TrayIcon {
    pub fn install(hwnd: HWND) -> Result<Self, String> {
        let mut nid: NOTIFYICONDATAW = unsafe { zeroed() };
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };
        copy_tip(&mut nid.szTip, "CapsLock RS");

        let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &mut nid) };
        if ok == 0 {
            return Err("failed to add tray icon".to_string());
        }

        logging::log_line("tray icon installed");
        Ok(Self { hwnd })
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        let mut nid: NOTIFYICONDATAW = unsafe { zeroed() };
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd;
        nid.uID = 1;

        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &mut nid);
        }
        logging::log_line("tray icon removed");
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    match message {
        WM_TRAYICON => {
            let mouse_message = l_param as u32;
            if mouse_message == WM_RBUTTONUP || mouse_message == WM_LBUTTONUP {
                show_menu(hwnd);
            }
            0
        }
        WM_COMMAND => {
            handle_menu_command(w_param & 0xffff);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, w_param, l_param),
    }
}

fn show_menu(hwnd: HWND) {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        logging::log_line("failed to create tray popup menu");
        return;
    }

    let enabled_flags = if app::is_enabled() {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING
    };
    let startup_flags = if app::start_with_windows() {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING
    };

    append_menu(menu, enabled_flags, MENU_TOGGLE_ENABLED, "Enabled");
    append_menu(
        menu,
        startup_flags,
        MENU_TOGGLE_STARTUP,
        "Start with Windows",
    );
    append_separator(menu);
    append_menu(menu, MF_STRING, MENU_RELOAD_CONFIG, "Reload config");
    append_menu(menu, MF_STRING, MENU_OPEN_CONFIG, "Open config");
    append_menu(menu, MF_STRING, MENU_OPEN_LOG, "Open log");
    append_menu(
        menu,
        MF_STRING | MF_GRAYED,
        MENU_SETTINGS,
        "Settings page (future)",
    );
    append_separator(menu);
    append_menu(menu, MF_STRING, MENU_EXIT, "Exit");

    let mut point = POINT { x: 0, y: 0 };
    unsafe {
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON, point.x, point.y, 0, hwnd, null());
        DestroyMenu(menu);
    }
}

fn handle_menu_command(command: usize) {
    match command {
        MENU_TOGGLE_ENABLED => app::toggle_enabled(),
        MENU_TOGGLE_STARTUP => app::toggle_startup(),
        MENU_RELOAD_CONFIG => app::reload_config(),
        MENU_OPEN_CONFIG => app::open_config(),
        MENU_OPEN_LOG => app::open_log(),
        MENU_EXIT => unsafe { PostQuitMessage(0) },
        _ => {}
    }
}

fn append_menu(menu: HMENU, flags: u32, id: usize, text: &str) {
    let text = win::to_wide_null(text);
    unsafe {
        AppendMenuW(menu, flags, id, text.as_ptr());
    }
}

fn append_separator(menu: HMENU) {
    unsafe {
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
    }
}

fn copy_tip(target: &mut [u16], text: &str) {
    let wide = win::to_wide_null(text);
    let copy_len = target.len().min(wide.len());
    target[..copy_len].copy_from_slice(&wide[..copy_len]);
}

pub fn message_loop() {
    let mut message: MSG = unsafe { zeroed() };
    while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
