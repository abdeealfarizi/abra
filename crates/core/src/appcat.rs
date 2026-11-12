//! Kategori aplikasi untuk jendela "Aplikasi" (pengganti Launchpad): tebakan dari nama/jalur, murni dan diuji.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cat {
    Productivity,
    Creative,
    Entertainment,
    Utility,
    System,
}

pub const ALL: [Cat; 5] = [Cat::Productivity, Cat::Creative, Cat::Entertainment, Cat::Utility, Cat::System];

impl Cat {
    /// Judul (kunci terjemahan Indonesia).
    pub fn title(self) -> &'static str {
        match self {
            Cat::Productivity => "Produktivitas",
            Cat::Creative => "Kreativitas",
            Cat::Entertainment => "Hiburan",
            Cat::Utility => "Utilitas",
            Cat::System => "Sistem",
        }
    }
}

const SYSTEM: &[&str] = &[
    "settings", "pengaturan", "control panel", "panel kontrol", "task manager", "device manager", "registry", "powershell", "command prompt", "terminal",
    "windows tools", "event viewer", "services", "defender", "security", "update", "microsoft store", "disk", "resource monitor", "performance", "system", "msconfig", "regedit",
];
const CREATIVE: &[&str] = &[
    "photoshop", "illustrator", "premiere", "after effects", "figma", "blender", "paint", "gimp", "inkscape", "canva", "davinci", "obs ", "obs studio", "audacity", "capcut",
    "lightroom", "krita", "photos", "video editor", "clipchamp", "snipping", "sketch", "design", "animate", "indesign", "fl studio",
];
const ENTERTAINMENT: &[&str] = &[
    "game", "steam", "epic games", "spotify", "music", "media player", "vlc", "netflix", "youtube", "movies", "xbox", "discord", "whatsapp", "telegram", "tiktok", "twitch",
    "pes", "fifa", "minecraft", "roblox", "tv", "podcast", "prime video",
];
const PRODUCTIVITY: &[&str] = &[
    "word", "excel", "powerpoint", "outlook", "onenote", "office", "notepad", "notion", "teams", "slack", "zoom", "acrobat", "pdf", "mail", "calendar", "todo", "to do", "sticky",
    "libreoffice", "obsidian", "visual studio", "code", "browser", "chrome", "edge", "firefox", "brave", "opera", "claude", "chatgpt", "trello", "drive", "dropbox", "onedrive",
];

fn has(name: &str, list: &[&str]) -> bool {
    list.iter().any(|k| name.contains(k))
}

/// Tebak kategori dari nama dan jalur (huruf kecil dibandingkan). Urutan: sistem, kreatif, hiburan, produktivitas; sisanya utilitas.
pub fn guess(name: &str, path: &str) -> Cat {
    let n = name.to_lowercase();
    let p = path.to_lowercase().replace(char::from(92u8), "/");
    let both = format!("{n} {p}");
    if has(&n, SYSTEM) || p.contains("/windows/system32/") && !n.contains("notepad") && !n.contains("paint") && !n.contains("calc") {
        return Cat::System;
    }
    if has(&n, CREATIVE) {
        return Cat::Creative;
    }
    if has(&n, ENTERTAINMENT) || p.contains("/steamapps/") || p.contains("/games/") {
        return Cat::Entertainment;
    }
    if has(&n, PRODUCTIVITY) {
        return Cat::Productivity;
    }
    let _ = both;
    Cat::Utility
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typical_apps_land_in_expected_categories() {
        assert_eq!(guess("Microsoft Word", ""), Cat::Productivity);
        assert_eq!(guess("Adobe Photoshop 2025", ""), Cat::Creative);
        assert_eq!(guess("Spotify", ""), Cat::Entertainment);
        assert_eq!(guess("Task Manager", ""), Cat::System);
        assert_eq!(guess("7-Zip File Manager", ""), Cat::Utility);
        assert_eq!(guess("Foo", "C:/Program Files/Steam/steamapps/common/foo.exe"), Cat::Entertainment);
    }

    #[test]
    fn every_category_has_a_title() {
        for c in ALL {
            assert!(!c.title().is_empty());
        }
    }
}
