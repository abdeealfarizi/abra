#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Keys(&'static [u16]),
    Guide,
}

#[derive(Clone, Copy, Debug)]
pub enum Line {
    Sep,
    Cmd { id: &'static str, en: &'static str, accel: &'static str, act: Act },
}

#[derive(Clone, Copy, Debug)]
pub struct Menu {
    pub id: &'static str,
    pub en: &'static str,
    pub lines: &'static [Line],
}

impl Menu {
    pub fn title(&self, lang: crate::i18n::Lang) -> &'static str {
        match lang {
            crate::i18n::Lang::Id => self.id,
            crate::i18n::Lang::En => self.en,
            crate::i18n::Lang::Es => crate::i18n::menu_es(self.en).unwrap_or(self.id),
        }
    }
}

impl Line {
    pub fn label(&self, lang: crate::i18n::Lang) -> &'static str {
        match self {
            Line::Sep => "",
            Line::Cmd { id, en, .. } => match lang {
                crate::i18n::Lang::Id => id,
                crate::i18n::Lang::En => en,
                crate::i18n::Lang::Es => crate::i18n::menu_es(en).unwrap_or(id),
            },
        }
    }
}

const C: u16 = 0x11;
const S: u16 = 0x10;
const A: u16 = 0x12;
const W: u16 = 0x5B;
const TAB: u16 = 0x09;
const LEFT: u16 = 0x25;
const UP: u16 = 0x26;
const RIGHT: u16 = 0x27;
const DOWN: u16 = 0x28;
const DEL: u16 = 0x2E;
const ENTER: u16 = 0x0D;
const PGUP: u16 = 0x21;
const PGDN: u16 = 0x22;
const PLUS: u16 = 0xBB;
const MINUS: u16 = 0xBD;
const SLASH: u16 = 0xBF;
const TICK: u16 = 0xC0;
const SPACE: u16 = 0x20;
const F1: u16 = 0x70;
const F2: u16 = 0x71;
const F4: u16 = 0x73;
const F5: u16 = 0x74;
const F11: u16 = 0x7A;
const F12: u16 = 0x7B;

const fn k(id: &'static str, en: &'static str, accel: &'static str, keys: &'static [u16]) -> Line {
    Line::Cmd { id, en, accel, act: Act::Keys(keys) }
}

const UNDO: Line = k("Urungkan", "Undo", "Ctrl+Z", &[C, 0x5A]);
const REDO: Line = k("Ulangi", "Redo", "Ctrl+Y", &[C, 0x59]);
const CUT: Line = k("Potong", "Cut", "Ctrl+X", &[C, 0x58]);
const COPY: Line = k("Salin", "Copy", "Ctrl+C", &[C, 0x43]);
const PASTE: Line = k("Tempel", "Paste", "Ctrl+V", &[C, 0x56]);
const ALL: Line = k("Pilih Semua", "Select All", "Ctrl+A", &[C, 0x41]);
const FIND: Line = k("Cari…", "Find…", "Ctrl+F", &[C, 0x46]);
const ZOOM_IN: Line = k("Perbesar", "Zoom In", "Ctrl++", &[C, PLUS]);
const ZOOM_OUT: Line = k("Perkecil", "Zoom Out", "Ctrl+-", &[C, MINUS]);
const ZOOM_0: Line = k("Ukuran Asli", "Actual Size", "Ctrl+0", &[C, 0x30]);
const FULL: Line = k("Layar Penuh", "Full Screen", "F11", &[F11]);
const BACK: Line = k("Mundur", "Back", "Alt+←", &[A, LEFT]);
const FWD: Line = k("Maju", "Forward", "Alt+→", &[A, RIGHT]);

const WINDOW: Menu = Menu {
    id: "Jendela",
    en: "Window",
    lines: &[
        k("Minimalkan", "Minimize", "Win+↓", &[W, DOWN]),
        k("Zoom", "Zoom", "Win+↑", &[W, UP]),
        Line::Sep,
        k("Pindah ke Kiri Layar", "Move to Left of Screen", "Win+←", &[W, LEFT]),
        k("Pindah ke Kanan Layar", "Move to Right of Screen", "Win+→", &[W, RIGHT]),
        Line::Sep,
        k("Tutup Jendela", "Close Window", "Alt+F4", &[A, F4]),
    ],
};

const HELP: Menu = Menu {
    id: "Bantuan",
    en: "Help",
    lines: &[
        Line::Cmd { id: "Panduan ABRA", en: "ABRA Guide", accel: "", act: Act::Guide },
        Line::Sep,
        k("Bantuan Aplikasi", "App Help", "F1", &[F1]),
    ],
};

const GENERIC: &[Menu] = &[
    Menu {
        id: "Berkas",
        en: "File",
        lines: &[
            k("Baru", "New", "Ctrl+N", &[C, 0x4E]),
            k("Buka…", "Open…", "Ctrl+O", &[C, 0x4F]),
            Line::Sep,
            k("Simpan", "Save", "Ctrl+S", &[C, 0x53]),
            k("Tutup", "Close", "Ctrl+W", &[C, 0x57]),
            Line::Sep,
            k("Cetak…", "Print…", "Ctrl+P", &[C, 0x50]),
        ],
    },
    Menu { id: "Edit", en: "Edit", lines: &[UNDO, REDO, Line::Sep, CUT, COPY, PASTE, ALL, Line::Sep, FIND] },
    Menu { id: "Tampilan", en: "View", lines: &[FULL, Line::Sep, ZOOM_IN, ZOOM_OUT, ZOOM_0] },
    WINDOW,
    HELP,
];

const BROWSER: &[Menu] = &[
    Menu {
        id: "Berkas",
        en: "File",
        lines: &[
            k("Tab Baru", "New Tab", "Ctrl+T", &[C, 0x54]),
            k("Jendela Baru", "New Window", "Ctrl+N", &[C, 0x4E]),
            k("Jendela Penyamaran Baru", "New Incognito Window", "Ctrl+Shift+N", &[C, S, 0x4E]),
            k("Buka Lagi Tab yang Ditutup", "Reopen Closed Tab", "Ctrl+Shift+T", &[C, S, 0x54]),
            Line::Sep,
            k("Buka Berkas…", "Open File…", "Ctrl+O", &[C, 0x4F]),
            k("Simpan Halaman…", "Save Page As…", "Ctrl+S", &[C, 0x53]),
            k("Cetak…", "Print…", "Ctrl+P", &[C, 0x50]),
            Line::Sep,
            k("Tutup Tab", "Close Tab", "Ctrl+W", &[C, 0x57]),
            k("Tutup Jendela", "Close Window", "Ctrl+Shift+W", &[C, S, 0x57]),
        ],
    },
    Menu {
        id: "Edit",
        en: "Edit",
        lines: &[UNDO, REDO, Line::Sep, CUT, COPY, PASTE, k("Tempel Teks Polos", "Paste as Plain Text", "Ctrl+Shift+V", &[C, S, 0x56]), ALL, Line::Sep, FIND],
    },
    Menu {
        id: "Tampilan",
        en: "View",
        lines: &[
            k("Muat Ulang", "Reload", "Ctrl+R", &[C, 0x52]),
            k("Muat Ulang Penuh", "Hard Reload", "Ctrl+Shift+R", &[C, S, 0x52]),
            Line::Sep,
            FULL,
            ZOOM_IN,
            ZOOM_OUT,
            ZOOM_0,
            Line::Sep,
            k("Alat Pengembang", "Developer Tools", "Ctrl+Shift+I", &[C, S, 0x49]),
            k("Lihat Sumber", "View Source", "Ctrl+U", &[C, 0x55]),
        ],
    },
    Menu {
        id: "Riwayat",
        en: "History",
        lines: &[
            BACK,
            FWD,
            Line::Sep,
            k("Tampilkan Riwayat", "Show History", "Ctrl+H", &[C, 0x48]),
            k("Unduhan", "Downloads", "Ctrl+J", &[C, 0x4A]),
            k("Hapus Data Penjelajahan…", "Clear Browsing Data…", "Ctrl+Shift+Del", &[C, S, DEL]),
        ],
    },
    Menu {
        id: "Bookmark",
        en: "Bookmarks",
        lines: &[
            k("Bookmark Tab Ini", "Bookmark This Tab", "Ctrl+D", &[C, 0x44]),
            k("Bookmark Semua Tab", "Bookmark All Tabs", "Ctrl+Shift+D", &[C, S, 0x44]),
            Line::Sep,
            k("Tampilkan Bilah Bookmark", "Show Bookmarks Bar", "Ctrl+Shift+B", &[C, S, 0x42]),
            k("Pengelola Bookmark", "Bookmark Manager", "Ctrl+Shift+O", &[C, S, 0x4F]),
        ],
    },
    Menu {
        id: "Tab",
        en: "Tab",
        lines: &[
            k("Tab Berikutnya", "Next Tab", "Ctrl+Tab", &[C, TAB]),
            k("Tab Sebelumnya", "Previous Tab", "Ctrl+Shift+Tab", &[C, S, TAB]),
            Line::Sep,
            k("Geser Tab ke Kiri", "Move Tab Left", "Ctrl+Shift+PgUp", &[C, S, PGUP]),
            k("Geser Tab ke Kanan", "Move Tab Right", "Ctrl+Shift+PgDn", &[C, S, PGDN]),
            Line::Sep,
            k("Tab Pertama", "First Tab", "Ctrl+1", &[C, 0x31]),
            k("Tab Terakhir", "Last Tab", "Ctrl+9", &[C, 0x39]),
            Line::Sep,
            k("Tutup Tab", "Close Tab", "Ctrl+W", &[C, 0x57]),
        ],
    },
    WINDOW,
    HELP,
];

const EXPLORER: &[Menu] = &[
    Menu {
        id: "Berkas",
        en: "File",
        lines: &[
            k("Jendela Baru", "New Window", "Ctrl+N", &[C, 0x4E]),
            k("Tab Baru", "New Tab", "Ctrl+T", &[C, 0x54]),
            k("Folder Baru", "New Folder", "Ctrl+Shift+N", &[C, S, 0x4E]),
            Line::Sep,
            k("Properti", "Properties", "Alt+Enter", &[A, ENTER]),
            Line::Sep,
            k("Tutup Tab", "Close Tab", "Ctrl+W", &[C, 0x57]),
        ],
    },
    Menu {
        id: "Edit",
        en: "Edit",
        lines: &[
            UNDO,
            REDO,
            Line::Sep,
            CUT,
            COPY,
            PASTE,
            k("Salin Jalur", "Copy as Path", "Ctrl+Shift+C", &[C, S, 0x43]),
            Line::Sep,
            k("Ganti Nama", "Rename", "F2", &[F2]),
            k("Hapus", "Delete", "Del", &[DEL]),
            Line::Sep,
            ALL,
        ],
    },
    Menu {
        id: "Tampilan",
        en: "View",
        lines: &[
            k("Segarkan", "Refresh", "F5", &[F5]),
            Line::Sep,
            k("Ikon Besar", "Large Icons", "Ctrl+Shift+2", &[C, S, 0x32]),
            k("Ikon Sedang", "Medium Icons", "Ctrl+Shift+3", &[C, S, 0x33]),
            k("Ikon Kecil", "Small Icons", "Ctrl+Shift+4", &[C, S, 0x34]),
            k("Daftar", "List", "Ctrl+Shift+5", &[C, S, 0x35]),
            k("Detail", "Details", "Ctrl+Shift+6", &[C, S, 0x36]),
            k("Ubin", "Tiles", "Ctrl+Shift+7", &[C, S, 0x37]),
            Line::Sep,
            k("Panel Pratinjau", "Preview Pane", "Alt+P", &[A, 0x50]),
            k("Panel Detail", "Details Pane", "Alt+Shift+P", &[A, S, 0x50]),
        ],
    },
    Menu {
        id: "Buka",
        en: "Go",
        lines: &[
            BACK,
            FWD,
            k("Folder Induk", "Enclosing Folder", "Alt+↑", &[A, UP]),
            Line::Sep,
            k("Bilah Alamat", "Address Bar", "Alt+D", &[A, 0x44]),
            k("Cari", "Search", "Ctrl+E", &[C, 0x45]),
        ],
    },
    WINDOW,
    HELP,
];

const OFFICE: &[Menu] = &[
    Menu {
        id: "Berkas",
        en: "File",
        lines: &[
            k("Baru", "New", "Ctrl+N", &[C, 0x4E]),
            k("Buka…", "Open…", "Ctrl+O", &[C, 0x4F]),
            Line::Sep,
            k("Simpan", "Save", "Ctrl+S", &[C, 0x53]),
            k("Simpan Sebagai…", "Save As…", "F12", &[F12]),
            Line::Sep,
            k("Cetak…", "Print…", "Ctrl+P", &[C, 0x50]),
            k("Tutup", "Close", "Ctrl+W", &[C, 0x57]),
        ],
    },
    Menu {
        id: "Edit",
        en: "Edit",
        lines: &[UNDO, REDO, Line::Sep, CUT, COPY, PASTE, ALL, Line::Sep, FIND, k("Ganti…", "Replace…", "Ctrl+H", &[C, 0x48])],
    },
    Menu {
        id: "Format",
        en: "Format",
        lines: &[
            k("Tebal", "Bold", "Ctrl+B", &[C, 0x42]),
            k("Miring", "Italic", "Ctrl+I", &[C, 0x49]),
            k("Garis Bawah", "Underline", "Ctrl+U", &[C, 0x55]),
            Line::Sep,
            k("Rata Kiri", "Align Left", "Ctrl+L", &[C, 0x4C]),
            k("Rata Tengah", "Center", "Ctrl+E", &[C, 0x45]),
            k("Rata Kanan", "Align Right", "Ctrl+R", &[C, 0x52]),
        ],
    },
    WINDOW,
    HELP,
];

const CODE: &[Menu] = &[
    Menu {
        id: "Berkas",
        en: "File",
        lines: &[
            k("Berkas Baru", "New File", "Ctrl+N", &[C, 0x4E]),
            k("Jendela Baru", "New Window", "Ctrl+Shift+N", &[C, S, 0x4E]),
            k("Buka Berkas…", "Open File…", "Ctrl+O", &[C, 0x4F]),
            Line::Sep,
            k("Simpan", "Save", "Ctrl+S", &[C, 0x53]),
            k("Simpan Sebagai…", "Save As…", "Ctrl+Shift+S", &[C, S, 0x53]),
            Line::Sep,
            k("Tutup Editor", "Close Editor", "Ctrl+W", &[C, 0x57]),
        ],
    },
    Menu {
        id: "Edit",
        en: "Edit",
        lines: &[
            UNDO,
            REDO,
            Line::Sep,
            CUT,
            COPY,
            PASTE,
            Line::Sep,
            FIND,
            k("Ganti…", "Replace…", "Ctrl+H", &[C, 0x48]),
            k("Cari di Berkas…", "Find in Files…", "Ctrl+Shift+F", &[C, S, 0x46]),
            Line::Sep,
            k("Jadikan Komentar", "Toggle Comment", "Ctrl+/", &[C, SLASH]),
        ],
    },
    Menu {
        id: "Tampilan",
        en: "View",
        lines: &[
            k("Palet Perintah…", "Command Palette…", "Ctrl+Shift+P", &[C, S, 0x50]),
            k("Buka Cepat…", "Quick Open…", "Ctrl+P", &[C, 0x50]),
            Line::Sep,
            k("Penjelajah", "Explorer", "Ctrl+Shift+E", &[C, S, 0x45]),
            k("Sidebar", "Toggle Sidebar", "Ctrl+B", &[C, 0x42]),
            k("Terminal", "Terminal", "Ctrl+`", &[C, TICK]),
            Line::Sep,
            ZOOM_IN,
            ZOOM_OUT,
            FULL,
        ],
    },
    Menu {
        id: "Buka",
        en: "Go",
        lines: &[
            k("Ke Baris…", "Go to Line…", "Ctrl+G", &[C, 0x47]),
            k("Ke Simbol…", "Go to Symbol…", "Ctrl+Shift+O", &[C, S, 0x4F]),
            Line::Sep,
            BACK,
            FWD,
        ],
    },
    Menu {
        id: "Jalankan",
        en: "Run",
        lines: &[
            k("Mulai Debug", "Start Debugging", "F5", &[F5]),
            k("Jalankan Tanpa Debug", "Run Without Debugging", "Ctrl+F5", &[C, F5]),
        ],
    },
    WINDOW,
    HELP,
];

const MUSIC: &[Menu] = &[
    Menu {
        id: "Putar",
        en: "Playback",
        lines: &[
            k("Putar / Jeda", "Play / Pause", "Space", &[SPACE]),
            k("Berikutnya", "Next", "Ctrl+→", &[C, RIGHT]),
            k("Sebelumnya", "Previous", "Ctrl+←", &[C, LEFT]),
            Line::Sep,
            k("Naikkan Volume", "Volume Up", "Ctrl+↑", &[C, UP]),
            k("Turunkan Volume", "Volume Down", "Ctrl+↓", &[C, DOWN]),
        ],
    },
    Menu { id: "Edit", en: "Edit", lines: &[CUT, COPY, PASTE, ALL, Line::Sep, k("Cari", "Search", "Ctrl+L", &[C, 0x4C])] },
    WINDOW,
    HELP,
];

pub fn exe_of(path: &str) -> String {
    let lower = path.to_lowercase();
    lower.rsplit(['\\', '/']).next().unwrap_or("").to_string()
}

pub fn for_path(path: &str) -> &'static [Menu] {
    match exe_of(path).as_str() {
        "chrome.exe" | "msedge.exe" | "brave.exe" | "firefox.exe" | "opera.exe" | "vivaldi.exe" | "arc.exe" | "zen.exe" | "librewolf.exe" | "thorium.exe" => BROWSER,
        "explorer.exe" => EXPLORER,
        "winword.exe" | "excel.exe" | "powerpnt.exe" | "onenote.exe" => OFFICE,
        "code.exe" | "cursor.exe" | "windsurf.exe" | "code - insiders.exe" | "vscodium.exe" | "antigravity.exe" | "zed.exe" => CODE,
        "spotify.exe" => MUSIC,
        _ => GENERIC,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_follow_the_app() {
        assert_eq!(for_path(r"C:\Program Files\Google\Chrome\Application\chrome.exe")[0].lines.len(), 11);
        assert_eq!(for_path(r"C:\Windows\explorer.exe")[3].en, "Go");
        assert_eq!(for_path(r"C:\Apps\WINWORD.EXE")[2].en, "Format");
        assert_eq!(for_path("shell:AppsFolder\\Something!App")[0].en, "File");
    }

    #[test]
    fn every_profile_ends_with_window_and_help() {
        for exe in ["chrome.exe", "explorer.exe", "winword.exe", "code.exe", "spotify.exe", "notepad.exe"] {
            let m = for_path(exe);
            assert_eq!(m[m.len() - 2].en, "Window");
            assert_eq!(m[m.len() - 1].en, "Help");
            assert!(m.len() <= 12);
            for menu in m {
                assert!(menu.lines.len() <= 20);
                for l in menu.lines {
                    if let Line::Cmd { act: Act::Keys(keys), .. } = l {
                        assert!(!keys.is_empty());
                    }
                }
            }
        }
    }

    #[test]
    fn labels_switch_language() {
        let m = &for_path("brave.exe")[0];
        assert_eq!((m.title(crate::i18n::Lang::Id), m.title(crate::i18n::Lang::En)), ("Berkas", "File"));
        assert_eq!((m.lines[0].label(crate::i18n::Lang::Id), m.lines[0].label(crate::i18n::Lang::En)), ("Tab Baru", "New Tab"));
        assert_eq!(m.title(crate::i18n::Lang::Es), "Archivo");
        assert_eq!(m.lines[0].label(crate::i18n::Lang::Es), "Nueva pestaña");
    }
}
