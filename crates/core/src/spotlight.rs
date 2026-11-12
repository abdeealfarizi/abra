//! Model Spotlight ala macOS Tahoe: mode (Cari, Apps, Berkas, Aksi, Papan Klip), daftar Aksi dengan quick keys, dan
//! pengurutan hasil. Murni (tanpa Win32) dan diuji.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Search,
    Apps,
    Files,
    Actions,
    Clipboard,
}

impl Mode {
    /// Pintasan Ctrl+angka seperti ⌘1…⌘4 di Tahoe.
    pub fn from_digit(d: u32) -> Option<Mode> {
        match d {
            1 => Some(Mode::Apps),
            2 => Some(Mode::Files),
            3 => Some(Mode::Actions),
            4 => Some(Mode::Clipboard),
            _ => None,
        }
    }

    pub fn digit(self) -> u32 {
        match self {
            Mode::Search => 0,
            Mode::Apps => 1,
            Mode::Files => 2,
            Mode::Actions => 3,
            Mode::Clipboard => 4,
        }
    }

    /// Judul panel (kunci terjemahan Indonesia).
    pub fn title(self) -> &'static str {
        match self {
            Mode::Search => "Cari Spotlight",
            Mode::Apps => "Aplikasi",
            Mode::Files => "Berkas Saya",
            Mode::Actions => "Aksi",
            Mode::Clipboard => "Papan Klip",
        }
    }
}

pub struct ActionDef {
    pub id: &'static str,
    pub title: &'static str,
    /// App/sumber yang tampil di baris kedua.
    pub app: &'static str,
    /// Quick key bawaan ("" = belum ada).
    pub default_key: &'static str,
}

/// Aksi yang masuk akal di Windows (urutan = urutan Saran).
pub const ACTIONS: &[ActionDef] = &[
    ActionDef { id: "note", title: "Buat Catatan", app: "Notepad", default_key: "nn" },
    ActionDef { id: "timer", title: "Mulai Timer", app: "Jam", default_key: "tm" },
    ActionDef { id: "email", title: "Kirim Email", app: "Email", default_key: "" },
    ActionDef { id: "screenshot", title: "Ambil Tangkapan Layar", app: "Snipping Tool", default_key: "ss" },
    ActionDef { id: "lock", title: "Kunci Layar", app: "Sistem", default_key: "lk" },
    ActionDef { id: "desktop", title: "Tampilkan Desktop", app: "Sistem", default_key: "" },
    ActionDef { id: "dark", title: "Ganti Mode Gelap", app: "ABRA", default_key: "dm" },
    ActionDef { id: "stage", title: "Nyalakan/Matikan Stage Manager", app: "ABRA", default_key: "sm" },
    ActionDef { id: "mission", title: "Mission Control", app: "ABRA", default_key: "mc" },
    ActionDef { id: "calc", title: "Buka Kalkulator", app: "Kalkulator", default_key: "ca" },
    ActionDef { id: "taskmgr", title: "Buka Task Manager", app: "Sistem", default_key: "" },
    ActionDef { id: "settings", title: "Buka Pengaturan Windows", app: "Pengaturan", default_key: "st" },
    ActionDef { id: "clearclip", title: "Hapus Riwayat Papan Klip", app: "ABRA", default_key: "" },
];

/// Baca `actions.txt`: satu baris `id=key`.
pub fn parse_quick_keys(text: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for line in text.lines() {
        let Some((id, key)) = line.split_once('=') else { continue };
        let (id, key) = (id.trim(), key.trim().to_lowercase());
        if ACTIONS.iter().any(|a| a.id == id) && key.chars().all(|c| c.is_ascii_alphanumeric()) && key.len() <= 6 {
            m.insert(id.to_string(), key);
        }
    }
    m
}

pub fn quick_keys_to_text(m: &HashMap<String, String>) -> String {
    let mut lines: Vec<String> = m.iter().map(|(k, v)| format!("{k}={v}")).collect();
    lines.sort();
    lines.join("\n")
}

/// Quick key efektif: pilihan pengguna, atau bawaan bila tidak ada (kosong = tanpa).
pub fn key_of<'a>(a: &'a ActionDef, user: &'a HashMap<String, String>) -> &'a str {
    user.get(a.id).map(|s| s.as_str()).unwrap_or(a.default_key)
}

/// Urutkan aksi untuk `query`. Query kosong = semua (urutan Saran). Quick key yang cocok persis ada di paling atas,
/// lalu judul/app yang diawali query, lalu yang memuatnya.
pub fn rank_actions(query: &str, user: &HashMap<String, String>, titles: &dyn Fn(&ActionDef) -> String) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return (0..ACTIONS.len()).collect();
    }
    let mut scored: Vec<(u32, usize)> = Vec::new();
    for (i, a) in ACTIONS.iter().enumerate() {
        let key = key_of(a, user);
        let title = titles(a).to_lowercase();
        let app = a.app.to_lowercase();
        let s = if !key.is_empty() && key == q {
            0
        } else if title.starts_with(&q) {
            1
        } else if title.split_whitespace().any(|w| w.starts_with(&q)) {
            2
        } else if title.contains(&q) || app.contains(&q) {
            3
        } else {
            continue;
        };
        scored.push((s, i));
    }
    scored.sort();
    scored.into_iter().map(|(_, i)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_map_to_modes_like_cmd_1_to_4() {
        assert_eq!(Mode::from_digit(1), Some(Mode::Apps));
        assert_eq!(Mode::from_digit(4), Some(Mode::Clipboard));
        assert_eq!(Mode::from_digit(0), None);
        assert_eq!(Mode::Actions.digit(), 3);
    }

    #[test]
    fn quick_keys_parse_validate_and_round_trip() {
        let m = parse_quick_keys("note=NN\nbogus=zz\ntimer=t m\nlock=lk\n\nmission=toolongkey");
        assert_eq!(m.get("note").map(|s| s.as_str()), Some("nn"));
        assert!(!m.contains_key("bogus"));
        assert!(!m.contains_key("timer"), "spasi tidak valid");
        assert!(!m.contains_key("mission"), "terlalu panjang");
        let t = quick_keys_to_text(&m);
        assert_eq!(parse_quick_keys(&t), m);
    }

    #[test]
    fn exact_quick_key_comes_first_then_prefix_then_contains() {
        let user = HashMap::new();
        let t = |a: &ActionDef| a.title.to_string();
        let r = rank_actions("nn", &user, &t);
        assert_eq!(ACTIONS[r[0]].id, "note");
        let r = rank_actions("kunci", &user, &t);
        assert_eq!(ACTIONS[r[0]].id, "lock");
        let r = rank_actions("pengaturan", &user, &t);
        assert!(r.iter().any(|i| ACTIONS[*i].id == "settings"));
        assert_eq!(rank_actions("", &user, &t).len(), ACTIONS.len());
        assert!(rank_actions("zzzzqq", &user, &t).is_empty());
    }

    #[test]
    fn user_key_overrides_default() {
        let mut user = HashMap::new();
        user.insert("lock".to_string(), "xx".to_string());
        let a = ACTIONS.iter().find(|a| a.id == "lock").unwrap();
        assert_eq!(key_of(a, &user), "xx");
        let b = ACTIONS.iter().find(|a| a.id == "note").unwrap();
        assert_eq!(key_of(b, &user), "nn");
        let t = |a: &ActionDef| a.title.to_string();
        assert_eq!(ACTIONS[rank_actions("xx", &user, &t)[0]].id, "lock");
    }
}
