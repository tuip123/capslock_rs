use std::collections::BTreeSet;

pub const VK_BACK: u16 = 0x08;
pub const VK_TAB: u16 = 0x09;
pub const VK_RETURN: u16 = 0x0D;
pub const VK_SHIFT: u16 = 0x10;
pub const VK_CONTROL: u16 = 0x11;
pub const VK_MENU: u16 = 0x12;
pub const VK_PAUSE: u16 = 0x13;
pub const VK_CAPITAL: u16 = 0x14;
pub const VK_ESCAPE: u16 = 0x1B;
pub const VK_SPACE: u16 = 0x20;
pub const VK_PRIOR: u16 = 0x21;
pub const VK_NEXT: u16 = 0x22;
pub const VK_END: u16 = 0x23;
pub const VK_HOME: u16 = 0x24;
pub const VK_LEFT: u16 = 0x25;
pub const VK_UP: u16 = 0x26;
pub const VK_RIGHT: u16 = 0x27;
pub const VK_DOWN: u16 = 0x28;
pub const VK_INSERT: u16 = 0x2D;
pub const VK_DELETE: u16 = 0x2E;
pub const VK_LWIN: u16 = 0x5B;
pub const VK_RWIN: u16 = 0x5C;
pub const VK_APPS: u16 = 0x5D;
pub const VK_NUMPAD0: u16 = 0x60;
pub const VK_NUMPAD1: u16 = 0x61;
pub const VK_NUMPAD2: u16 = 0x62;
pub const VK_NUMPAD3: u16 = 0x63;
pub const VK_NUMPAD4: u16 = 0x64;
pub const VK_NUMPAD5: u16 = 0x65;
pub const VK_NUMPAD6: u16 = 0x66;
pub const VK_NUMPAD7: u16 = 0x67;
pub const VK_NUMPAD8: u16 = 0x68;
pub const VK_NUMPAD9: u16 = 0x69;
pub const VK_MULTIPLY: u16 = 0x6A;
pub const VK_ADD: u16 = 0x6B;
pub const VK_SUBTRACT: u16 = 0x6D;
pub const VK_DECIMAL: u16 = 0x6E;
pub const VK_DIVIDE: u16 = 0x6F;
pub const VK_F1: u16 = 0x70;
pub const VK_F24: u16 = 0x87;
pub const VK_LSHIFT: u16 = 0xA0;
pub const VK_RSHIFT: u16 = 0xA1;
pub const VK_LCONTROL: u16 = 0xA2;
pub const VK_RCONTROL: u16 = 0xA3;
pub const VK_LMENU: u16 = 0xA4;
pub const VK_RMENU: u16 = 0xA5;
pub const VK_BROWSER_BACK: u16 = 0xA6;
pub const VK_BROWSER_FORWARD: u16 = 0xA7;
pub const VK_BROWSER_REFRESH: u16 = 0xA8;
pub const VK_VOLUME_MUTE: u16 = 0xAD;
pub const VK_VOLUME_DOWN: u16 = 0xAE;
pub const VK_VOLUME_UP: u16 = 0xAF;
pub const VK_MEDIA_NEXT_TRACK: u16 = 0xB0;
pub const VK_MEDIA_PREV_TRACK: u16 = 0xB1;
pub const VK_MEDIA_STOP: u16 = 0xB2;
pub const VK_MEDIA_PLAY_PAUSE: u16 = 0xB3;
pub const VK_OEM_1: u16 = 0xBA;
pub const VK_OEM_PLUS: u16 = 0xBB;
pub const VK_OEM_COMMA: u16 = 0xBC;
pub const VK_OEM_MINUS: u16 = 0xBD;
pub const VK_OEM_PERIOD: u16 = 0xBE;
pub const VK_OEM_2: u16 = 0xBF;
pub const VK_OEM_3: u16 = 0xC0;
pub const VK_OEM_4: u16 = 0xDB;
pub const VK_OEM_5: u16 = 0xDC;
pub const VK_OEM_6: u16 = 0xDD;
pub const VK_OEM_7: u16 = 0xDE;
pub const VK_OEM_102: u16 = 0xE2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyKind {
    LayoutCharacter,
    VirtualKey,
    PhysicalKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyCode {
    pub name: &'static str,
    pub vk: u16,
    pub kind: KeyKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ModifierFamily {
    Ctrl,
    Alt,
    Shift,
    Win,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum KeyModifier {
    Ctrl,
    LCtrl,
    RCtrl,
    Alt,
    LAlt,
    RAlt,
    Shift,
    LShift,
    RShift,
    Win,
    LWin,
    RWin,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyCombo {
    pub modifiers: Vec<KeyModifier>,
    pub key: KeyCode,
}

impl KeyCombo {
    pub fn new(modifiers: Vec<KeyModifier>, key: KeyCode) -> Result<Self, String> {
        Ok(Self {
            modifiers: normalize_modifiers(modifiers)?,
            key,
        })
    }

    pub fn capslock_ini_key(&self) -> String {
        format!("caps_{}", self.ini_suffix())
    }

    pub fn ini_suffix(&self) -> String {
        let mut parts: Vec<&str> = self
            .modifiers
            .iter()
            .map(|modifier| modifier.canonical_name())
            .collect();
        parts.push(self.key.name);
        parts.join("_")
    }
}

impl KeyModifier {
    pub fn canonical_name(self) -> &'static str {
        match self {
            KeyModifier::Ctrl => "ctrl",
            KeyModifier::LCtrl => "lctrl",
            KeyModifier::RCtrl => "rctrl",
            KeyModifier::Alt => "alt",
            KeyModifier::LAlt => "lalt",
            KeyModifier::RAlt => "ralt",
            KeyModifier::Shift => "shift",
            KeyModifier::LShift => "lshift",
            KeyModifier::RShift => "rshift",
            KeyModifier::Win => "win",
            KeyModifier::LWin => "lwin",
            KeyModifier::RWin => "rwin",
        }
    }

    pub fn family(self) -> ModifierFamily {
        match self {
            KeyModifier::Ctrl | KeyModifier::LCtrl | KeyModifier::RCtrl => ModifierFamily::Ctrl,
            KeyModifier::Alt | KeyModifier::LAlt | KeyModifier::RAlt => ModifierFamily::Alt,
            KeyModifier::Shift | KeyModifier::LShift | KeyModifier::RShift => ModifierFamily::Shift,
            KeyModifier::Win | KeyModifier::LWin | KeyModifier::RWin => ModifierFamily::Win,
        }
    }

    pub fn output_vk(self) -> u16 {
        match self {
            KeyModifier::Ctrl => VK_CONTROL,
            KeyModifier::LCtrl => VK_LCONTROL,
            KeyModifier::RCtrl => VK_RCONTROL,
            KeyModifier::Alt => VK_MENU,
            KeyModifier::LAlt => VK_LMENU,
            KeyModifier::RAlt => VK_RMENU,
            KeyModifier::Shift => VK_SHIFT,
            KeyModifier::LShift => VK_LSHIFT,
            KeyModifier::RShift => VK_RSHIFT,
            KeyModifier::Win | KeyModifier::LWin => VK_LWIN,
            KeyModifier::RWin => VK_RWIN,
        }
    }

    fn sort_key(self) -> (u8, u8) {
        let family = match self.family() {
            ModifierFamily::Ctrl => 0,
            ModifierFamily::Alt => 1,
            ModifierFamily::Shift => 2,
            ModifierFamily::Win => 3,
        };
        let side = match self {
            KeyModifier::Ctrl | KeyModifier::Alt | KeyModifier::Shift | KeyModifier::Win => 0,
            KeyModifier::LCtrl | KeyModifier::LAlt | KeyModifier::LShift | KeyModifier::LWin => 1,
            KeyModifier::RCtrl | KeyModifier::RAlt | KeyModifier::RShift | KeyModifier::RWin => 2,
        };
        (family, side)
    }
}

pub fn parse_capslock_combo_name(value: &str) -> Result<KeyCombo, String> {
    let normalized = normalize_combo_text(value);
    let rest = normalized
        .strip_prefix("caps_")
        .ok_or_else(|| format!("CapsLock combo must start with caps_: {value}"))?;
    parse_combo_suffix(rest)
}

pub fn parse_combo_suffix(value: &str) -> Result<KeyCombo, String> {
    let normalized = normalize_combo_text(value);
    let parts: Vec<&str> = normalized
        .split('_')
        .filter(|part| !part.is_empty())
        .collect();

    if parts.is_empty() {
        return Err("empty key combo".to_string());
    }

    let mut modifiers = Vec::new();
    let mut key_start = 0;
    for (index, part) in parts.iter().enumerate() {
        let Some(modifier) = parse_modifier_token(part) else {
            key_start = index;
            break;
        };
        modifiers.push(modifier);
        key_start = index + 1;
    }

    if key_start >= parts.len() {
        return Err(format!("missing final key in combo: {value}"));
    }

    let key_name = parts[key_start..].join("_");
    let key =
        parse_key_code(&key_name).ok_or_else(|| format!("unknown key in combo: {key_name}"))?;
    KeyCombo::new(modifiers, key)
}

pub fn parse_key_code(value: &str) -> Option<KeyCode> {
    let key_name = canonical_key_name(value);
    KEY_CODES
        .iter()
        .copied()
        .find(|key| key.name == key_name.as_str())
}

pub fn parse_modifier_token(value: &str) -> Option<KeyModifier> {
    match normalize_token(value).as_str() {
        "ctrl" | "control" => Some(KeyModifier::Ctrl),
        "lctrl" | "leftctrl" | "leftcontrol" => Some(KeyModifier::LCtrl),
        "rctrl" | "rightctrl" | "rightcontrol" => Some(KeyModifier::RCtrl),
        "alt" | "menu" => Some(KeyModifier::Alt),
        "lalt" | "leftalt" | "leftmenu" => Some(KeyModifier::LAlt),
        "ralt" | "rightalt" | "rightmenu" => Some(KeyModifier::RAlt),
        "shift" => Some(KeyModifier::Shift),
        "lshift" | "leftshift" => Some(KeyModifier::LShift),
        "rshift" | "rightshift" => Some(KeyModifier::RShift),
        "win" | "super" | "meta" => Some(KeyModifier::Win),
        "lwin" | "leftwin" | "leftsuper" | "leftmeta" => Some(KeyModifier::LWin),
        "rwin" | "rightwin" | "rightsuper" | "rightmeta" => Some(KeyModifier::RWin),
        _ => None,
    }
}

pub fn modifier_from_vk(vk: u16) -> Option<KeyModifier> {
    match vk {
        VK_CONTROL => Some(KeyModifier::Ctrl),
        VK_LCONTROL => Some(KeyModifier::LCtrl),
        VK_RCONTROL => Some(KeyModifier::RCtrl),
        VK_MENU => Some(KeyModifier::Alt),
        VK_LMENU => Some(KeyModifier::LAlt),
        VK_RMENU => Some(KeyModifier::RAlt),
        VK_SHIFT => Some(KeyModifier::Shift),
        VK_LSHIFT => Some(KeyModifier::LShift),
        VK_RSHIFT => Some(KeyModifier::RShift),
        VK_LWIN => Some(KeyModifier::LWin),
        VK_RWIN => Some(KeyModifier::RWin),
        _ => None,
    }
}

fn normalize_modifiers(mut modifiers: Vec<KeyModifier>) -> Result<Vec<KeyModifier>, String> {
    modifiers.sort_by_key(|modifier| modifier.sort_key());

    let mut seen_families = BTreeSet::new();
    let mut normalized = Vec::new();
    for modifier in modifiers {
        if !seen_families.insert(modifier.family()) {
            return Err(format!(
                "duplicate modifier family in combo: {:?}",
                modifier.family()
            ));
        }

        normalized.push(modifier);
    }

    Ok(normalized)
}

fn normalize_combo_text(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace(['+', '-'], "_")
}

fn normalize_token(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-', ' '], "")
}

fn canonical_key_name(value: &str) -> String {
    let normalized = value
        .trim()
        .to_ascii_lowercase()
        .replace(['+', '-', ' '], "_");

    match normalized.as_str() {
        "esc" => "escape",
        "return" => "enter",
        "back" | "bs" | "bksp" => "backspace",
        "del" => "delete",
        "ins" => "insert",
        "pgup" | "pageup" => "page_up",
        "pgdn" | "pagedown" => "page_down",
        "leftsquarebracket" | "open_bracket" | "lbracket" => "left_square_bracket",
        "rightsquarebracket" | "close_bracket" | "rbracket" => "right_square_bracket",
        "back_quote" | "backquote" | "grave_accent" => "grave",
        "dash" => "minus",
        "equal" => "equals",
        "period" => "dot",
        "back_slash" => "backslash",
        "quote" | "apostrophe" => "single_quote",
        "apps" | "menu" => "application",
        "multiply" => "numpad_multiply",
        "add" => "numpad_add",
        "subtract" => "numpad_subtract",
        "decimal" => "numpad_decimal",
        "divide" => "numpad_divide",
        "volumemute" | "volume_mute" => "volume_mute",
        "volumedown" | "volume_down" => "volume_down",
        "volumeup" | "volume_up" => "volume_up",
        "medianext" | "media_next" | "next_track" => "media_next",
        "mediaprev" | "media_prev" | "previous_track" => "media_prev",
        "mediastop" | "media_stop" => "media_stop",
        "mediaplaypause" | "media_playpause" | "play_pause" => "media_play_pause",
        other => other,
    }
    .to_string()
}

const fn key(name: &'static str, vk: u16, kind: KeyKind) -> KeyCode {
    KeyCode { name, vk, kind }
}

const KEY_CODES: &[KeyCode] = &[
    key("a", 0x41, KeyKind::LayoutCharacter),
    key("b", 0x42, KeyKind::LayoutCharacter),
    key("c", 0x43, KeyKind::LayoutCharacter),
    key("d", 0x44, KeyKind::LayoutCharacter),
    key("e", 0x45, KeyKind::LayoutCharacter),
    key("f", 0x46, KeyKind::LayoutCharacter),
    key("g", 0x47, KeyKind::LayoutCharacter),
    key("h", 0x48, KeyKind::LayoutCharacter),
    key("i", 0x49, KeyKind::LayoutCharacter),
    key("j", 0x4A, KeyKind::LayoutCharacter),
    key("k", 0x4B, KeyKind::LayoutCharacter),
    key("l", 0x4C, KeyKind::LayoutCharacter),
    key("m", 0x4D, KeyKind::LayoutCharacter),
    key("n", 0x4E, KeyKind::LayoutCharacter),
    key("o", 0x4F, KeyKind::LayoutCharacter),
    key("p", 0x50, KeyKind::LayoutCharacter),
    key("q", 0x51, KeyKind::LayoutCharacter),
    key("r", 0x52, KeyKind::LayoutCharacter),
    key("s", 0x53, KeyKind::LayoutCharacter),
    key("t", 0x54, KeyKind::LayoutCharacter),
    key("u", 0x55, KeyKind::LayoutCharacter),
    key("v", 0x56, KeyKind::LayoutCharacter),
    key("w", 0x57, KeyKind::LayoutCharacter),
    key("x", 0x58, KeyKind::LayoutCharacter),
    key("y", 0x59, KeyKind::LayoutCharacter),
    key("z", 0x5A, KeyKind::LayoutCharacter),
    key("0", 0x30, KeyKind::LayoutCharacter),
    key("1", 0x31, KeyKind::LayoutCharacter),
    key("2", 0x32, KeyKind::LayoutCharacter),
    key("3", 0x33, KeyKind::LayoutCharacter),
    key("4", 0x34, KeyKind::LayoutCharacter),
    key("5", 0x35, KeyKind::LayoutCharacter),
    key("6", 0x36, KeyKind::LayoutCharacter),
    key("7", 0x37, KeyKind::LayoutCharacter),
    key("8", 0x38, KeyKind::LayoutCharacter),
    key("9", 0x39, KeyKind::LayoutCharacter),
    key("space", VK_SPACE, KeyKind::VirtualKey),
    key("tab", VK_TAB, KeyKind::VirtualKey),
    key("enter", VK_RETURN, KeyKind::VirtualKey),
    key("escape", VK_ESCAPE, KeyKind::VirtualKey),
    key("backspace", VK_BACK, KeyKind::VirtualKey),
    key("delete", VK_DELETE, KeyKind::VirtualKey),
    key("insert", VK_INSERT, KeyKind::VirtualKey),
    key("home", VK_HOME, KeyKind::VirtualKey),
    key("end", VK_END, KeyKind::VirtualKey),
    key("page_up", VK_PRIOR, KeyKind::VirtualKey),
    key("page_down", VK_NEXT, KeyKind::VirtualKey),
    key("left", VK_LEFT, KeyKind::VirtualKey),
    key("right", VK_RIGHT, KeyKind::VirtualKey),
    key("up", VK_UP, KeyKind::VirtualKey),
    key("down", VK_DOWN, KeyKind::VirtualKey),
    key("pause", VK_PAUSE, KeyKind::VirtualKey),
    key("application", VK_APPS, KeyKind::VirtualKey),
    key("semicolon", VK_OEM_1, KeyKind::LayoutCharacter),
    key("equals", VK_OEM_PLUS, KeyKind::LayoutCharacter),
    key("comma", VK_OEM_COMMA, KeyKind::LayoutCharacter),
    key("minus", VK_OEM_MINUS, KeyKind::LayoutCharacter),
    key("dot", VK_OEM_PERIOD, KeyKind::LayoutCharacter),
    key("slash", VK_OEM_2, KeyKind::LayoutCharacter),
    key("grave", VK_OEM_3, KeyKind::LayoutCharacter),
    key("left_square_bracket", VK_OEM_4, KeyKind::LayoutCharacter),
    key("backslash", VK_OEM_5, KeyKind::LayoutCharacter),
    key("right_square_bracket", VK_OEM_6, KeyKind::LayoutCharacter),
    key("single_quote", VK_OEM_7, KeyKind::LayoutCharacter),
    key("oem_102", VK_OEM_102, KeyKind::LayoutCharacter),
    key("numpad0", VK_NUMPAD0, KeyKind::PhysicalKey),
    key("numpad1", VK_NUMPAD1, KeyKind::PhysicalKey),
    key("numpad2", VK_NUMPAD2, KeyKind::PhysicalKey),
    key("numpad3", VK_NUMPAD3, KeyKind::PhysicalKey),
    key("numpad4", VK_NUMPAD4, KeyKind::PhysicalKey),
    key("numpad5", VK_NUMPAD5, KeyKind::PhysicalKey),
    key("numpad6", VK_NUMPAD6, KeyKind::PhysicalKey),
    key("numpad7", VK_NUMPAD7, KeyKind::PhysicalKey),
    key("numpad8", VK_NUMPAD8, KeyKind::PhysicalKey),
    key("numpad9", VK_NUMPAD9, KeyKind::PhysicalKey),
    key("numpad_multiply", VK_MULTIPLY, KeyKind::PhysicalKey),
    key("numpad_add", VK_ADD, KeyKind::PhysicalKey),
    key("numpad_subtract", VK_SUBTRACT, KeyKind::PhysicalKey),
    key("numpad_decimal", VK_DECIMAL, KeyKind::PhysicalKey),
    key("numpad_divide", VK_DIVIDE, KeyKind::PhysicalKey),
    key("f1", VK_F1, KeyKind::VirtualKey),
    key("f2", VK_F1 + 1, KeyKind::VirtualKey),
    key("f3", VK_F1 + 2, KeyKind::VirtualKey),
    key("f4", VK_F1 + 3, KeyKind::VirtualKey),
    key("f5", VK_F1 + 4, KeyKind::VirtualKey),
    key("f6", VK_F1 + 5, KeyKind::VirtualKey),
    key("f7", VK_F1 + 6, KeyKind::VirtualKey),
    key("f8", VK_F1 + 7, KeyKind::VirtualKey),
    key("f9", VK_F1 + 8, KeyKind::VirtualKey),
    key("f10", VK_F1 + 9, KeyKind::VirtualKey),
    key("f11", VK_F1 + 10, KeyKind::VirtualKey),
    key("f12", VK_F1 + 11, KeyKind::VirtualKey),
    key("f13", VK_F1 + 12, KeyKind::VirtualKey),
    key("f14", VK_F1 + 13, KeyKind::VirtualKey),
    key("f15", VK_F1 + 14, KeyKind::VirtualKey),
    key("f16", VK_F1 + 15, KeyKind::VirtualKey),
    key("f17", VK_F1 + 16, KeyKind::VirtualKey),
    key("f18", VK_F1 + 17, KeyKind::VirtualKey),
    key("f19", VK_F1 + 18, KeyKind::VirtualKey),
    key("f20", VK_F1 + 19, KeyKind::VirtualKey),
    key("f21", VK_F1 + 20, KeyKind::VirtualKey),
    key("f22", VK_F1 + 21, KeyKind::VirtualKey),
    key("f23", VK_F1 + 22, KeyKind::VirtualKey),
    key("f24", VK_F24, KeyKind::VirtualKey),
    key("browser_back", VK_BROWSER_BACK, KeyKind::VirtualKey),
    key("browser_forward", VK_BROWSER_FORWARD, KeyKind::VirtualKey),
    key("browser_refresh", VK_BROWSER_REFRESH, KeyKind::VirtualKey),
    key("volume_mute", VK_VOLUME_MUTE, KeyKind::VirtualKey),
    key("volume_down", VK_VOLUME_DOWN, KeyKind::VirtualKey),
    key("volume_up", VK_VOLUME_UP, KeyKind::VirtualKey),
    key("media_next", VK_MEDIA_NEXT_TRACK, KeyKind::VirtualKey),
    key("media_prev", VK_MEDIA_PREV_TRACK, KeyKind::VirtualKey),
    key("media_stop", VK_MEDIA_STOP, KeyKind::VirtualKey),
    key("media_play_pause", VK_MEDIA_PLAY_PAUSE, KeyKind::VirtualKey),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_normalizes_source_combo() {
        let combo = parse_capslock_combo_name("caps_shift_lalt_j").unwrap();

        assert_eq!(combo.ini_suffix(), "lalt_shift_j");
        assert_eq!(combo.key.name, "j");
        assert_eq!(combo.modifiers, vec![KeyModifier::LAlt, KeyModifier::Shift]);
    }

    #[test]
    fn parses_target_combo_with_aliases() {
        let combo = parse_combo_suffix("shift+ctrl+pageDown").unwrap();

        assert_eq!(combo.ini_suffix(), "ctrl_shift_page_down");
        assert_eq!(combo.key.vk, VK_NEXT);
    }

    #[test]
    fn parses_common_key_kinds() {
        assert_eq!(
            parse_key_code("f5"),
            Some(key("f5", VK_F1 + 4, KeyKind::VirtualKey))
        );
        assert_eq!(
            parse_key_code("leftSquareBracket"),
            Some(key(
                "left_square_bracket",
                VK_OEM_4,
                KeyKind::LayoutCharacter
            ))
        );
        assert_eq!(
            parse_key_code("numpad4"),
            Some(key("numpad4", VK_NUMPAD4, KeyKind::PhysicalKey))
        );
    }
}
