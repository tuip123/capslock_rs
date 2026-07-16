use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const RESOURCE_SCRIPT: &str = "resources\\capslock_rs.rc";
const RESOURCE_ICON: &str = "assets\\app-icon.ico";

fn main() {
    println!("cargo:rerun-if-changed={RESOURCE_SCRIPT}");
    println!("cargo:rerun-if-changed={RESOURCE_ICON}");

    if env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        println!("cargo:warning=app icon embedding currently supports the MSVC Windows target");
        return;
    }

    let Some(rc_exe) = find_resource_compiler() else {
        panic!("failed to find rc.exe; install the Windows SDK or add rc.exe to PATH");
    };

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set by Cargo"));
    let res_file = out_dir.join("capslock_rs.res");
    let status = Command::new(&rc_exe)
        .arg("/nologo")
        .arg(format!("/fo{}", res_file.display()))
        .arg(RESOURCE_SCRIPT)
        .status()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", rc_exe.display()));

    if !status.success() {
        panic!("resource compiler failed with status {status}");
    }

    println!("cargo:rustc-link-arg-bins={}", res_file.display());
}

fn find_resource_compiler() -> Option<PathBuf> {
    if command_exists("rc.exe") {
        return Some(PathBuf::from("rc.exe"));
    }

    let base = env::var_os("ProgramFiles(x86)")
        .map(PathBuf::from)
        .or_else(|| env::var_os("ProgramFiles").map(PathBuf::from))?;
    let kits_bin = base.join("Windows Kits").join("10").join("bin");
    let arch = resource_compiler_arch();

    let mut candidates = fs::read_dir(kits_bin)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(arch).join("rc.exe"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.pop()
}

fn resource_compiler_arch() -> &'static str {
    match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => "x64",
    }
}

fn command_exists(command: &str) -> bool {
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&paths)
        .map(|path| path.join(command))
        .any(|path| is_executable_file(&path))
}

fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}
