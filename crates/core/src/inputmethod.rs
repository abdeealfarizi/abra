#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchKey {
    CtrlShift,
    AltZ,
}

impl SwitchKey {
    pub fn label(self) -> &'static str {
        match self {
            SwitchKey::CtrlShift => "Ctrl+Shift",
            SwitchKey::AltZ => "Alt+Z",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineKind {
    EvKey,
    UniKey,
    OpenKey,
}

impl EngineKind {
    pub fn name(self) -> &'static str {
        match self {
            EngineKind::EvKey => "EVKey",
            EngineKind::UniKey => "UniKey",
            EngineKind::OpenKey => "OpenKey",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvKeyCfg {
    pub vietnamese: bool,
    pub switch_key: SwitchKey,
}

impl Default for EvKeyCfg {
    fn default() -> Self {
        Self { vietnamese: true, switch_key: SwitchKey::CtrlShift }
    }
}

pub fn parse_evkey_ini(text: &str) -> EvKeyCfg {
    let mut cfg = EvKeyCfg::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('[') || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = k.trim();
        let v = v.trim();
        if k.eq_ignore_ascii_case("Lang") {
            cfg.vietnamese = v != "0";
        } else if k.eq_ignore_ascii_case("SKey") {
            cfg.switch_key = if v == "1" { SwitchKey::AltZ } else { SwitchKey::CtrlShift };
        }
    }
    cfg
}

pub fn unikey_switch_key(val: u32) -> SwitchKey {
    if val == 1 {
        SwitchKey::AltZ
    } else {
        SwitchKey::CtrlShift
    }
}

pub fn code_from_lang_id(lang_id: u16) -> &'static str {
    match lang_id & 0x03FF {
        0x09 => "EN",
        0x2A => "VI",
        0x21 => "ID",
        0x0A => "ES",
        0x11 => "JA",
        0x12 => "KO",
        0x04 => "ZH",
        0x0C => "FR",
        0x07 => "DE",
        0x10 => "IT",
        0x16 => "PT",
        0x19 => "RU",
        0x1E => "TH",
        0x01 => "AR",
        0x39 => "HI",
        _ => "KB",
    }
}

pub fn short_code(locale: &str, lang_id: u16) -> String {
    let primary = locale.trim().split(['-', '_']).next().unwrap_or("");
    if primary.len() == 2 && primary.bytes().all(|b| b.is_ascii_alphabetic()) {
        primary.to_ascii_uppercase()
    } else {
        code_from_lang_id(lang_id).to_string()
    }
}

pub fn badge(active_code: &str, ime_vi: Option<bool>) -> String {
    if ime_vi == Some(true) && active_code.eq_ignore_ascii_case("EN") {
        "VI".to_string()
    } else {
        active_code.to_ascii_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_evkey_ini_settings() {
        let ini = "[setting]\nSKey=0\nType=1\nLang=1\n";
        assert_eq!(parse_evkey_ini(ini), EvKeyCfg { vietnamese: true, switch_key: SwitchKey::CtrlShift });

        let ini_en = "[setting]\nSKey=1\nLang=0\n";
        assert_eq!(parse_evkey_ini(ini_en), EvKeyCfg { vietnamese: false, switch_key: SwitchKey::AltZ });
    }

    #[test]
    fn maps_lang_id_and_locale_codes() {
        assert_eq!(short_code("vi-VN", 0x042A), "VI");
        assert_eq!(short_code("en-US", 0x0409), "EN");
        assert_eq!(short_code("zh-Hans", 0x0804), "ZH");
        assert_eq!(short_code("", 0x042A), "VI");
        assert_eq!(short_code("", 0x0409), "EN");
    }

    #[test]
    fn badge_prefers_vi_when_external_ime_active_on_en_layout() {
        assert_eq!(badge("EN", Some(true)), "VI");
        assert_eq!(badge("EN", Some(false)), "EN");
        assert_eq!(badge("EN", None), "EN");
        assert_eq!(badge("VI", None), "VI");
        assert_eq!(badge("JA", Some(true)), "JA");
    }
}
