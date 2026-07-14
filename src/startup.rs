use std::env;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

use crate::logging;
use crate::win;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "CapsLockRS";

pub fn apply_startup(enabled: bool) -> Result<(), String> {
    if enabled {
        enable_startup()
    } else {
        disable_startup()
    }
}

fn enable_startup() -> Result<(), String> {
    let exe = env::current_exe().map_err(|error| format!("failed to get exe path: {error}"))?;
    let command = format!("\"{}\"", exe.display());
    let mut key: HKEY = null_mut();
    let mut disposition = 0;
    let subkey = win::to_wide_null(RUN_KEY);

    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            null(),
            &mut key,
            &mut disposition,
        )
    };

    if status != 0 {
        return Err(format!("failed to open startup registry key: {status}"));
    }

    let name = win::to_wide_null(VALUE_NAME);
    let data = win::to_wide_null(&command);
    let byte_len = (data.len() * std::mem::size_of::<u16>()) as u32;
    let set_status = unsafe {
        RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_SZ,
            data.as_ptr() as *const u8,
            byte_len,
        )
    };

    unsafe {
        RegCloseKey(key);
    }

    if set_status != 0 {
        return Err(format!(
            "failed to write startup registry value: {set_status}"
        ));
    }

    logging::log_line("startup registry value enabled");
    Ok(())
}

fn disable_startup() -> Result<(), String> {
    let mut key: HKEY = null_mut();
    let mut disposition = 0;
    let subkey = win::to_wide_null(RUN_KEY);

    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            null(),
            &mut key,
            &mut disposition,
        )
    };

    if status != 0 {
        return Err(format!("failed to open startup registry key: {status}"));
    }

    let name = win::to_wide_null(VALUE_NAME);
    let delete_status = unsafe { RegDeleteValueW(key, name.as_ptr()) };

    unsafe {
        RegCloseKey(key);
    }

    if delete_status != 0 && delete_status != ERROR_FILE_NOT_FOUND {
        return Err(format!(
            "failed to delete startup registry value: {delete_status}"
        ));
    }

    logging::log_line("startup registry value disabled");
    Ok(())
}
