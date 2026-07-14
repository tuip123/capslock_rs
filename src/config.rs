use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub general: GeneralConfig,
    pub capslock_layer: Vec<KeyMapping>,
    pub ui: UiConfig,
}

#[derive(Clone, Debug)]
pub struct GeneralConfig {
    pub enabled: bool,
    pub start_with_windows: bool,
    pub run_as_admin: bool,
    pub show_tray_icon: bool,
    pub tap_capslock: TapCapsLock,
}

#[derive(Clone, Debug)]
pub struct UiConfig {
    pub settings_backend: String,
    pub settings_page: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerAction {
    MoveLeft(u32),
    MoveDown(u32),
    MoveUp(u32),
    MoveRight(u32),
    MoveWordLeft(u32),
    MoveWordRight(u32),
    Enter(u32),
    Backspace(u32),
    Delete(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TapCapsLock {
    Toggle,
    Escape,
    None,
}

#[derive(Clone, Debug)]
pub struct KeyMapping {
    pub source_key: String,
    pub source_vk: u32,
    pub require_lalt: bool,
    pub action: LayerAction,
}

#[derive(Debug)]
pub struct ConfigPaths {
    pub config_path: PathBuf,
    pub log_path: PathBuf,
}

#[derive(Debug)]
struct KeysSectionConfig {
    mappings: Vec<KeyMapping>,
    tap_capslock: Option<TapCapsLock>,
}

impl ConfigPaths {
    pub fn resolve() -> Result<Self, String> {
        let config_path = if let Ok(path) = env::var("CAPSLOCK_RS_CONFIG") {
            PathBuf::from(path)
        } else {
            let current_dir = env::current_dir()
                .map_err(|error| format!("failed to get current directory: {error}"))?;
            let current_config = current_dir.join("capslock_rs.ini");
            if current_config.exists() {
                current_config
            } else {
                let exe_dir = current_exe_dir()?;
                let exe_config = exe_dir.join("capslock_rs.ini");
                if exe_config.exists() {
                    exe_config
                } else if current_dir.join("Cargo.toml").exists() {
                    current_config
                } else {
                    exe_config
                }
            }
        };

        let log_path = config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("capslock_rs.log");

        Ok(Self {
            config_path,
            log_path,
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                enabled: true,
                start_with_windows: false,
                run_as_admin: false,
                show_tray_icon: true,
                tap_capslock: TapCapsLock::Toggle,
            },
            capslock_layer: vec![
                KeyMapping::new("h", false, LayerAction::MoveLeft(1)),
                KeyMapping::new("j", false, LayerAction::MoveDown(1)),
                KeyMapping::new("k", false, LayerAction::MoveUp(1)),
                KeyMapping::new("l", false, LayerAction::MoveRight(1)),
                KeyMapping::new("space", false, LayerAction::Enter(1)),
                KeyMapping::new("q", false, LayerAction::Backspace(1)),
                KeyMapping::new("e", false, LayerAction::Delete(1)),
                KeyMapping::new("z", false, LayerAction::MoveUp(5)),
                KeyMapping::new("x", false, LayerAction::MoveDown(5)),
                KeyMapping::new("a", true, LayerAction::MoveWordLeft(1)),
                KeyMapping::new("d", true, LayerAction::MoveWordRight(1)),
            ],
            ui: UiConfig {
                settings_backend: "ini".to_string(),
                settings_page: "future".to_string(),
            },
        }
    }
}

impl Config {
    pub fn load_or_create(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            let config = Self::default();
            config.save(path)?;
            return Ok(config);
        }

        Self::load(path)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let content = read_config_text(path)?;
        Self::from_ini(&content)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }

        fs::write(path, self.to_ini_string())
            .map_err(|error| format!("failed to write {}: {error}", path.display()))
    }

    pub fn from_ini(content: &str) -> Result<Self, String> {
        let parsed = parse_ini(content)?;
        let mut config = Self::default();

        if let Some(global) = parsed.get("global") {
            config.general.start_with_windows =
                read_bool(global, "autostart", config.general.start_with_windows)?;
        }

        if let Some(general) = parsed.get("general") {
            config.general.enabled = read_bool(general, "enabled", config.general.enabled)?;
            config.general.start_with_windows = read_bool(
                general,
                "start_with_windows",
                config.general.start_with_windows,
            )?;
            config.general.run_as_admin =
                read_bool(general, "run_as_admin", config.general.run_as_admin)?;
            config.general.show_tray_icon =
                read_bool(general, "show_tray_icon", config.general.show_tray_icon)?;
            if let Some(value) = general.get("tap_capslock") {
                config.general.tap_capslock = parse_tap_capslock(value)?;
            }
        }

        if let Some(layer) = parsed.get("layer.capslock") {
            let mut mappings = Vec::new();
            for (source, action_name) in layer {
                let Some(source_vk) = parse_source_key(source) else {
                    return Err(format!("unknown source key in [layer.capslock]: {source}"));
                };
                let Some(action) = parse_layer_action(action_name)? else {
                    continue;
                };

                mappings.push(KeyMapping {
                    source_key: source.clone(),
                    source_vk,
                    require_lalt: false,
                    action,
                });
            }

            if !mappings.is_empty() {
                config.capslock_layer = mappings;
            }
        }

        if let Some(keys) = parse_capslock_plus_keys_section(content)? {
            if let Some(tap_capslock) = keys.tap_capslock {
                config.general.tap_capslock = tap_capslock;
            }
            if !keys.mappings.is_empty() {
                config.capslock_layer = keys.mappings;
            }
        }

        if let Some(ui) = parsed.get("ui") {
            if let Some(value) = ui.get("settings_backend") {
                config.ui.settings_backend = value.clone();
            }
            if let Some(value) = ui.get("settings_page") {
                config.ui.settings_page = value.clone();
            }
        }

        Ok(config)
    }

    pub fn to_ini_string(&self) -> String {
        let mut output = String::new();
        output.push_str("; CapsLock RS configuration.\n");
        output.push_str(
            "; [Keys] follows CapsLock+ style: caps_key=keyFunc_name(optional_count).\n\n",
        );
        output.push_str("[general]\n");
        output.push_str(&format!("enabled = {}\n", bool_text(self.general.enabled)));
        output.push_str(&format!(
            "start_with_windows = {}\n",
            bool_text(self.general.start_with_windows)
        ));
        output.push_str(&format!(
            "run_as_admin = {}\n",
            bool_text(self.general.run_as_admin)
        ));
        output.push_str(&format!(
            "show_tray_icon = {}\n",
            bool_text(self.general.show_tray_icon)
        ));
        output.push_str(&format!(
            "tap_capslock = {}\n\n",
            self.general.tap_capslock.as_str()
        ));

        output.push_str("[Keys]\n");
        for mapping in &self.capslock_layer {
            output.push_str(&format!(
                "{}={}\n",
                mapping.capslock_plus_key_name(),
                mapping.action.as_key_func()
            ));
        }

        output.push_str("\n[ui]\n");
        output.push_str(&format!(
            "settings_backend = {}\n",
            self.ui.settings_backend
        ));
        output.push_str(&format!("settings_page = {}\n", self.ui.settings_page));
        output
    }
}

impl LayerAction {
    pub fn as_key_func(self) -> String {
        match self {
            LayerAction::MoveLeft(count) => key_func_with_count("moveLeft", count),
            LayerAction::MoveDown(count) => key_func_with_count("moveDown", count),
            LayerAction::MoveUp(count) => key_func_with_count("moveUp", count),
            LayerAction::MoveRight(count) => key_func_with_count("moveRight", count),
            LayerAction::MoveWordLeft(count) => key_func_with_count("moveWordLeft", count),
            LayerAction::MoveWordRight(count) => key_func_with_count("moveWordRight", count),
            LayerAction::Enter(count) => key_func_with_count("enter", count),
            LayerAction::Backspace(count) => key_func_with_count("backspace", count),
            LayerAction::Delete(count) => key_func_with_count("delete", count),
        }
    }
}

impl TapCapsLock {
    pub fn as_str(self) -> &'static str {
        match self {
            TapCapsLock::Toggle => "toggle",
            TapCapsLock::Escape => "escape",
            TapCapsLock::None => "none",
        }
    }
}

impl KeyMapping {
    fn new(source_key: &str, require_lalt: bool, action: LayerAction) -> Self {
        Self {
            source_key: source_key.to_string(),
            source_vk: parse_source_key(source_key).unwrap_or_default(),
            require_lalt,
            action,
        }
    }

    fn capslock_plus_key_name(&self) -> String {
        if self.require_lalt {
            format!("caps_lalt_{}", self.source_key)
        } else {
            format!("caps_{}", self.source_key)
        }
    }
}

fn read_config_text(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_bytes(path, &bytes[2..], false);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_bytes(path, &bytes[2..], true);
    }

    String::from_utf8(bytes)
        .map_err(|error| format!("failed to decode {} as UTF-8: {error}", path.display()))
}

fn decode_utf16_bytes(path: &Path, bytes: &[u8], big_endian: bool) -> Result<String, String> {
    if bytes.len() % 2 != 0 {
        return Err(format!("invalid UTF-16 byte length in {}", path.display()));
    }

    let code_units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if big_endian {
                u16::from_be_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_le_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();

    String::from_utf16(&code_units)
        .map_err(|error| format!("failed to decode {} as UTF-16: {error}", path.display()))
}

fn parse_ini(content: &str) -> Result<BTreeMap<String, BTreeMap<String, String>>, String> {
    let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current_section = String::new();

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') {
            if !line.ends_with(']') {
                return Err(format!("invalid section header at line {line_number}"));
            }

            current_section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            sections.entry(current_section.clone()).or_default();
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("invalid key/value at line {line_number}"));
        };

        let key = key.trim().to_ascii_lowercase();
        let value = strip_inline_comment(value.trim())
            .trim()
            .to_ascii_lowercase();
        sections
            .entry(current_section.clone())
            .or_default()
            .insert(key, value);
    }

    Ok(sections)
}

fn parse_capslock_plus_keys_section(content: &str) -> Result<Option<KeysSectionConfig>, String> {
    let mut current_section = String::new();
    let mut mappings = Vec::new();
    let mut tap_capslock = None;
    let mut seen_keys = BTreeSet::new();
    let mut found_keys_section = false;

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') {
            if !line.ends_with(']') {
                return Err(format!("invalid section header at line {line_number}"));
            }
            current_section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            continue;
        }

        if current_section != "keys" {
            continue;
        }

        found_keys_section = true;
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("invalid key/value at line {line_number}"));
        };

        let key = key.trim().to_ascii_lowercase();
        let value = strip_inline_comment(value.trim())
            .trim()
            .to_ascii_lowercase();

        // User settings often put custom mappings above a copied doNothing block.
        if !seen_keys.insert(key.clone()) {
            continue;
        }

        if key == "press_caps" {
            tap_capslock = Some(parse_key_func_tap_capslock(&value)?);
            continue;
        }

        if is_do_nothing_action(&value) {
            continue;
        }

        let Some((source_key, require_lalt)) = parse_capslock_plus_source_key(&key) else {
            return Err(format!(
                "unknown CapsLock+ key name at line {line_number}: {key}"
            ));
        };
        let Some(action) = parse_layer_action(&value)? else {
            continue;
        };
        let Some(source_vk) = parse_source_key(&source_key) else {
            return Err(format!(
                "unknown source key in [Keys] at line {line_number}: {key}"
            ));
        };

        mappings.push(KeyMapping {
            source_key,
            source_vk,
            require_lalt,
            action,
        });
    }

    if found_keys_section {
        Ok(Some(KeysSectionConfig {
            mappings,
            tap_capslock,
        }))
    } else {
        Ok(None)
    }
}

fn strip_inline_comment(value: &str) -> &str {
    for marker in [" ;", " #"] {
        if let Some(index) = value.find(marker) {
            return &value[..index];
        }
    }

    value
}

fn read_bool(
    section: &BTreeMap<String, String>,
    key: &str,
    default_value: bool,
) -> Result<bool, String> {
    let Some(value) = section.get(key) else {
        return Ok(default_value);
    };

    parse_bool(value).ok_or_else(|| format!("invalid bool for {key}: {value}"))
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn bool_text(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn parse_tap_capslock(value: &str) -> Result<TapCapsLock, String> {
    match value {
        "toggle" => Ok(TapCapsLock::Toggle),
        "escape" => Ok(TapCapsLock::Escape),
        "none" | "off" | "disabled" => Ok(TapCapsLock::None),
        _ => Err(format!("invalid tap_capslock value: {value}")),
    }
}

fn parse_key_func_tap_capslock(value: &str) -> Result<TapCapsLock, String> {
    let (name, _) = parse_key_func_call(value)?;
    match name.as_str() {
        "togglecapslock" => Ok(TapCapsLock::Toggle),
        "esc" | "escape" => Ok(TapCapsLock::Escape),
        "donothing" => Ok(TapCapsLock::None),
        _ => Err(format!("unsupported press_caps action: {value}")),
    }
}

fn parse_layer_action(value: &str) -> Result<Option<LayerAction>, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if is_do_nothing_action(&normalized) {
        return Ok(None);
    }

    let (name, count) = parse_key_func_call(&normalized)?;
    let count = count.unwrap_or(1).max(1);
    let action = match name.as_str() {
        "left" | "arrow_left" | "moveleft" => LayerAction::MoveLeft(count),
        "down" | "arrow_down" | "movedown" => LayerAction::MoveDown(count),
        "up" | "arrow_up" | "moveup" => LayerAction::MoveUp(count),
        "right" | "arrow_right" | "moveright" => LayerAction::MoveRight(count),
        "movewordleft" => LayerAction::MoveWordLeft(count),
        "movewordright" => LayerAction::MoveWordRight(count),
        "enter" | "return" => LayerAction::Enter(count),
        "backspace" | "back" | "bs" => LayerAction::Backspace(count),
        "delete" | "del" => LayerAction::Delete(count),
        _ => return Err(format!("unsupported key action: {value}")),
    };

    Ok(Some(action))
}

fn parse_key_func_call(value: &str) -> Result<(String, Option<u32>), String> {
    let value = value.trim().to_ascii_lowercase();
    let value = value.strip_prefix("keyfunc_").unwrap_or(&value);

    let Some(open_index) = value.find('(') else {
        return Ok((value.to_string(), None));
    };

    if !value.ends_with(')') {
        return Err(format!("invalid key function call: {value}"));
    }

    let name = value[..open_index].trim().to_string();
    let raw_count = value[open_index + 1..value.len() - 1].trim();
    if raw_count.is_empty() {
        return Ok((name, Some(1)));
    }

    let count = raw_count
        .parse::<u32>()
        .map_err(|error| format!("invalid key function count in {value}: {error}"))?;
    Ok((name, Some(count)))
}

fn is_do_nothing_action(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    matches!(
        normalized.strip_prefix("keyfunc_").unwrap_or(&normalized),
        "donothing" | "none" | "off" | "disabled"
    )
}

fn parse_capslock_plus_source_key(value: &str) -> Option<(String, bool)> {
    let rest = value.strip_prefix("caps_")?;
    if let Some(source) = rest.strip_prefix("lalt_") {
        Some((normalize_capslock_plus_key_name(source)?, true))
    } else {
        Some((normalize_capslock_plus_key_name(rest)?, false))
    }
}

fn normalize_capslock_plus_key_name(value: &str) -> Option<String> {
    let normalized = match value {
        "leftsquarebracket" => "left_square_bracket",
        "rightsquarebracket" => "right_square_bracket",
        other => other,
    };

    Some(normalized.to_string())
}

fn parse_source_key(value: &str) -> Option<u32> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() == 1 {
        let byte = normalized.as_bytes()[0];
        if byte.is_ascii_lowercase() {
            return Some((byte.to_ascii_uppercase()) as u32);
        }
        if byte.is_ascii_digit() {
            return Some(byte as u32);
        }
    }

    match normalized.as_str() {
        "space" => Some(0x20),
        "tab" => Some(0x09),
        "enter" => Some(0x0D),
        "escape" | "esc" => Some(0x1B),
        "semicolon" => Some(0xBA),
        "comma" => Some(0xBC),
        "slash" => Some(0xBF),
        "left_square_bracket" => Some(0xDB),
        "right_square_bracket" => Some(0xDD),
        _ => None,
    }
}

fn key_func_with_count(name: &str, count: u32) -> String {
    if count <= 1 {
        format!("keyFunc_{name}")
    } else {
        format!("keyFunc_{name}({count})")
    }
}

fn current_exe_dir() -> Result<PathBuf, String> {
    let exe = env::current_exe().map_err(|error| format!("failed to get exe path: {error}"))?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("failed to get parent directory for {}", exe.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_capslock_layer_actions() {
        let config = Config::from_ini(
            r#"
            [general]
            enabled = true
            start_with_windows = false
            run_as_admin = true

            [layer.capslock]
            h = left
            j = down
            k = up
            l = right
            space = enter
            q = backspace
            e = delete
            "#,
        )
        .unwrap();

        assert!(config.general.enabled);
        assert!(config.general.run_as_admin);
        assert_eq!(config.capslock_layer.len(), 7);
        assert_eq!(
            find_action(&config, "h", false),
            Some(LayerAction::MoveLeft(1))
        );
        assert_eq!(
            find_action(&config, "space", false),
            Some(LayerAction::Enter(1))
        );
        assert_eq!(
            find_action(&config, "q", false),
            Some(LayerAction::Backspace(1))
        );
        assert_eq!(
            find_action(&config, "e", false),
            Some(LayerAction::Delete(1))
        );
    }

    #[test]
    fn parses_capslock_plus_key_func_actions() {
        let config = Config::from_ini(
            r#"
            [Global]
            autostart=1

            [Keys]
            caps_q=keyFunc_backspace
            caps_e=keyFunc_delete
            caps_w=keyFunc_moveUp(1)
            caps_a=keyFunc_moveLeft(1)
            caps_s=keyFunc_moveDown(1)
            caps_d=keyFunc_moveRight(1)
            caps_z=keyFunc_moveUp(5)
            caps_x=keyFunc_moveDown(5)
            caps_lalt_a=keyFunc_moveWordLeft
            caps_lalt_d=keyFunc_moveWordRight(2)
            caps_w=keyFunc_doNothing
            "#,
        )
        .unwrap();

        assert!(config.general.start_with_windows);
        assert_eq!(
            find_action(&config, "q", false),
            Some(LayerAction::Backspace(1))
        );
        assert_eq!(
            find_action(&config, "z", false),
            Some(LayerAction::MoveUp(5))
        );
        assert_eq!(
            find_action(&config, "x", false),
            Some(LayerAction::MoveDown(5))
        );
        assert_eq!(
            find_action(&config, "w", false),
            Some(LayerAction::MoveUp(1))
        );
        assert_eq!(
            find_action(&config, "a", true),
            Some(LayerAction::MoveWordLeft(1))
        );
        assert_eq!(
            find_action(&config, "d", true),
            Some(LayerAction::MoveWordRight(2))
        );
    }

    #[test]
    fn loads_utf16_capslock_plus_ini() {
        let path =
            std::env::temp_dir().join(format!("capslock_rs_utf16_test_{}.ini", std::process::id()));
        let content = "[Keys]\ncaps_z=keyFunc_moveUp(5)\n";
        let mut bytes = vec![0xFF, 0xFE];
        for code_unit in content.encode_utf16() {
            bytes.extend_from_slice(&code_unit.to_le_bytes());
        }

        fs::write(&path, bytes).unwrap();
        let config = Config::load(&path).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(
            find_action(&config, "z", false),
            Some(LayerAction::MoveUp(5))
        );
    }

    #[test]
    fn rejects_unknown_action() {
        let error = Config::from_ini(
            r#"
            [layer.capslock]
            h = launch
            "#,
        )
        .unwrap_err();

        assert!(error.contains("unsupported key action"));
    }

    fn find_action(config: &Config, source_key: &str, require_lalt: bool) -> Option<LayerAction> {
        config.capslock_layer.iter().find_map(|mapping| {
            (mapping.source_key == source_key && mapping.require_lalt == require_lalt)
                .then_some(mapping.action)
        })
    }
}
