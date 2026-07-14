use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HINSTANCE, HWND,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::{IsUserAnAdmin, ShellExecuteW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, PostQuitMessage, MB_ICONERROR, MB_OK, SW_SHOWNORMAL,
};

pub struct SingleInstance {
    handle: HANDLE,
}

impl SingleInstance {
    pub fn acquire(name: &str) -> Result<Self, String> {
        let wide_name = to_wide_null(name);
        let handle = unsafe { CreateMutexW(null_mut(), 1, wide_name.as_ptr()) };
        if handle.is_null() {
            return Err("failed to create single instance mutex".to_string());
        }

        let last_error = unsafe { GetLastError() };
        if last_error == ERROR_ALREADY_EXISTS {
            unsafe {
                CloseHandle(handle);
            }
            return Err("CapsLock RS is already running.".to_string());
        }

        Ok(Self { handle })
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

pub fn module_handle() -> HINSTANCE {
    unsafe { GetModuleHandleW(null()) }
}

pub fn message_loop() {
    crate::tray::message_loop();
}

pub fn quit_message_loop() {
    unsafe {
        PostQuitMessage(0);
    }
}

pub fn message_box(title: &str, message: &str) {
    let title = to_wide_null(title);
    let message = to_wide_null(message);
    unsafe {
        MessageBoxW(
            0 as HWND,
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn open_path(path: &Path) {
    let operation = to_wide_null("open");
    let file = to_wide_null(&path.to_string_lossy());
    unsafe {
        ShellExecuteW(
            null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
    }
}

pub fn is_user_admin() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

pub fn relaunch_as_admin() -> Result<(), String> {
    let exe =
        std::env::current_exe().map_err(|error| format!("failed to get exe path: {error}"))?;
    let directory = std::env::current_dir()
        .map_err(|error| format!("failed to get current directory: {error}"))?;

    let operation = to_wide_null("runas");
    let file = to_wide_null(&exe.to_string_lossy());
    let directory = to_wide_null(&directory.to_string_lossy());
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            null(),
            directory.as_ptr(),
            SW_SHOWNORMAL,
        )
    };

    if (result as isize) <= 32 {
        return Err(format!(
            "failed to relaunch as administrator: {}",
            result as isize
        ));
    }

    Ok(())
}

pub fn to_wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}
