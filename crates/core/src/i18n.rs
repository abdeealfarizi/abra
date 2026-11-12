//! Terjemahan teks antarmuka. Sumber teks adalah Bahasa Indonesia; `tr` memetakannya ke bahasa terpilih
//! dan jatuh kembali ke teks sumber bila tidak ada terjemahan (tidak pernah gagal/panik).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Id,
    En,
}

impl Lang {
    pub fn parse(s: &str) -> Option<Lang> {
        match s.trim().to_ascii_lowercase().as_str() {
            "id" | "indonesia" | "indonesian" => Some(Lang::Id),
            "en" | "english" | "inggris" => Some(Lang::En),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Lang::Id => "id",
            Lang::En => "en",
        }
    }
    pub const ALL: [Lang; 2] = [Lang::Id, Lang::En];
}

const EN: &[(&str, &str)] = &[
    ("Pengaturan", "Settings"),
    ("Pengaturan ABRA…", "ABRA Settings…"),
    ("MODE PENUH", "FULL MODE"),
    ("Sembunyikan taskbar Windows", "Hide the Windows taskbar"),
    ("DOCK", "DOCK"),
    ("NOTCH", "NOTCH"),
    ("Tetap", "Fixed"),
    ("Menimpa", "Overlay"),
    ("Auto-hide", "Auto-hide"),
    ("Sentuh tepi", "Edge peek"),
    ("TAMPILAN KACA", "GLASS APPEARANCE"),
    ("Tingkat blur", "Blur level"),
    ("Transparansi", "Transparency"),
    ("Gaya blur", "Blur style"),
    ("Buram", "Frosted"),
    ("Bening", "Clear"),
    ("Hidup", "Vivid"),
    ("Tema", "Theme"),
    ("Otomatis", "Automatic"),
    ("Terang", "Light"),
    ("Gelap", "Dark"),
    ("Bahasa", "Language"),
    ("Umum", "General"),
    ("Hot Corners", "Hot Corners"),
    ("Kiri atas", "Top left"),
    ("Kanan atas", "Top right"),
    ("Kiri bawah", "Bottom left"),
    ("Kanan bawah", "Bottom right"),
    ("Tidak ada", "None"),
    ("Tampilkan Desktop", "Show Desktop"),
    ("Stage Manager", "Stage Manager"),
    ("Pusat Notifikasi", "Notification Center"),
    ("Aktifkan Stage Manager", "Enable Stage Manager"),
    ("Ganti Alt+Tab dengan pengalih ABRA", "Replace Alt+Tab with the ABRA switcher"),
    ("Klik wallpaper menyimpan semua jendela", "Click wallpaper to hide all windows"),
    ("Judul jendela saat hover", "Show window title on hover"),
    ("Jeda media app di latar", "Pause media of background apps"),
    ("Posisi daftar jendela", "Window list position"),
    ("Kiri", "Left"),
    ("Kanan", "Right"),
    ("Ikon desktop", "Desktop icons"),
    ("Sembunyi", "Hidden"),
    ("Tampil", "Shown"),
    ("Ukuran kartu", "Card size"),
    ("Jarak antar kartu", "Card spacing"),
    ("Jumlah kartu", "Number of cards"),
    ("Kecepatan animasi", "Animation speed"),
    ("Stage", "Stage"),
    ("Latar menu bar", "Menu bar background"),
    ("Buka Apps", "Open Apps"),
    ("Paket ikon (folder iconpack)", "Icon pack (iconpack folder)"),
    ("Unduhan", "Downloads"),
    ("Buka folder Unduhan", "Open Downloads folder"),
    ("Tumpukan Unduhan", "Downloads stack"),
    ("App terakhir dipakai", "Recent apps"),
    ("Pintasan ala Mac (Alt+` ganti jendela app)", "Mac-style shortcuts (Alt+` switches app windows)"),
    ("Warna aksen", "Accent color"),
    ("Logo menu", "Menu logo"),
    ("Lingkaran", "Circle"),
    ("Apel", "Apple"),
    ("Dock", "Dock"),
    ("Notch", "Notch"),
    ("Menu Bar", "Menu Bar"),
    ("Sudut Layar", "Hot Corners"),
    ("Ikon", "Icons and cursor"),
    ("Daftar jendela", "Window list"),
    ("Gaya ikon", "Icon style"),
    ("Berwarna", "Tinted"),
    ("Warna tint", "Tint color"),
    ("Biru", "Blue"),
    ("Ungu", "Purple"),
    ("Merah Muda", "Pink"),
    ("Oranye", "Orange"),
    ("Hijau", "Green"),
    ("Cari Spotlight", "Spotlight Search"),
    ("Berkas Saya", "Files"),
    ("Aksi", "Actions"),
    ("Papan Klip", "Clipboard"),
    ("Tambah kunci cepat", "Add quick keys"),
    ("Tidak ada hasil", "No results"),
    ("Belum ada yang disalin dalam 8 jam terakhir.", "Nothing copied in the last 8 hours."),
    ("Ketik untuk mencari berkas.", "Type to search files."),
    ("Riwayat Papan Klip di Spotlight", "Clipboard history in Spotlight"),
    ("Buat Catatan", "Create Note"),
    ("Mulai Timer", "Start Timer"),
    ("Kirim Email", "Send Email"),
    ("Ambil Tangkapan Layar", "Take Screenshot"),
    ("Ganti Mode Gelap", "Toggle Dark Mode"),
    ("Nyalakan/Matikan Stage Manager", "Toggle Stage Manager"),
    ("Buka Kalkulator", "Open Calculator"),
    ("Buka Task Manager", "Open Task Manager"),
    ("Buka Pengaturan Windows", "Open Windows Settings"),
    ("Hapus Riwayat Papan Klip", "Clear Clipboard History"),
    ("Jam", "Clock"),
    ("Berbagi Terdekat", "Nearby"),
    ("Sedang", "Medium"),
    ("Besar", "Large"),
    ("Apps", "Apps"),
    ("Selalu di atas jendela layar penuh", "Always on top of full-screen windows"),
    ("Ikon seragam ala macOS (squircle)", "Uniform macOS-style icons (squircle)"),
    ("Kaca", "Glass"),
    ("Sembunyikan ikon desktop saat ABRA berjalan", "Hide desktop icons while ABRA runs"),
    ("Cari", "Search"),
    ("Cari Aplikasi", "Search Applications"),
    ("Tidak ada aplikasi", "No apps found"),
    ("Launchpad", "Launchpad"),
    ("Mission Control", "Mission Control"),
    ("Spotlight", "Spotlight"),
    ("Hasil kalkulator", "Calculator result"),
    ("Cari di web:", "Search the web for"),
    ("Pencarian web", "Web search"),
    ("Latar kaca di menu bar", "Glass background for the menu bar"),
    ("Aktifkan menu bar", "Enable menu bar"),
    ("Jam 24 jam", "24-hour clock"),
    ("Tentang PC Ini", "About This PC"),
    ("Pengaturan Sistem…", "System Settings…"),
    ("Toko Microsoft…", "Microsoft Store…"),
    ("Paksa Keluar…", "Force Quit…"),
    ("Tidur", "Sleep"),
    ("Mulai Ulang…", "Restart…"),
    ("Matikan…", "Shut Down…"),
    ("Keluar…", "Log Out…"),
    ("Tentang", "About"),
    ("Sembunyikan", "Hide"),
    ("Sembunyikan Lainnya", "Hide Others"),
    ("Tampilkan Semua", "Show All"),
    ("Keluar dari", "Quit"),
    ("Berkas", "File"),
    ("Baru", "New"),
    ("Buka…", "Open…"),
    ("Simpan", "Save"),
    ("Tutup Jendela", "Close Window"),
    ("Cetak…", "Print…"),
    ("Edit", "Edit"),
    ("Urungkan", "Undo"),
    ("Ulangi", "Redo"),
    ("Potong", "Cut"),
    ("Salin", "Copy"),
    ("Tempel", "Paste"),
    ("Pilih Semua", "Select All"),
    ("Cari…", "Find…"),
    ("Menu:Tampilan", "View"),
    ("Layar Penuh", "Enter Full Screen"),
    ("Perbesar", "Zoom In"),
    ("Perkecil", "Zoom Out"),
    ("Ukuran Asli", "Actual Size"),
    ("Menu:Jendela", "Window"),
    ("Minimalkan", "Minimize"),
    ("Zoom", "Zoom"),
    ("Bantuan", "Help"),
    ("Cari Bantuan", "Search Help"),
    ("Pusat Kontrol", "Control Center"),

    ("Kustom", "Custom"),
    ("Radius sudut jendela", "Window corner radius"),
    ("Sudut jendela app", "App window corners"),
    ("Bawaan", "Default"),
    ("Kotak", "Square"),
    ("Kecil", "Small"),
    ("Bulat", "Round"),
    ("Bayangan halus di bawah jendela aktif", "Soft shadow under the active window"),
    ("Jendela", "Windows"),
    ("Buka otomatis saat kursor di notch", "Open when the cursor is on the notch"),
    ("Aktifkan dock", "Enable dock"),
    ("Aktifkan notch", "Enable notch"),
    ("Refraksi tepi kaca (dock)", "Glass edge refraction (dock)"),
    ("Tempat sampah", "Trash"),
    ("Kosongkan tempat sampah", "Empty Trash"),
    ("Layar", "Display"),
    ("Suara", "Sound"),
    ("Aktif", "On"),
    ("Mati", "Off"),
    ("Semua", "All"),
    ("Produktivitas", "Productivity"),
    ("Kreativitas", "Creativity"),
    ("Hiburan", "Entertainment"),
    ("Sistem", "System"),
    ("Tidak diputar", "Not Playing"),
    ("Selesai", "Done"),
    ("Edit Kontrol…", "Edit Controls…"),
    ("Cari Kontrol", "Search Controls"),
    ("Semua Kontrol", "All Controls"),
    ("Konektivitas", "Connectivity"),
    ("Layar & Kecerahan", "Display & Brightness"),
    ("Sedang Diputar", "Now Playing"),
    ("Utilitas", "Utilities"),
    ("Mode Pesawat", "Airplane Mode"),
    ("Pencerminan Layar", "Screen Mirroring"),
    ("Fokus", "Focus"),
    ("Mode Gelap", "Dark Mode"),
    ("Tangkapan Layar", "Screenshot"),
    //("Layar", "Display"),
    ("Baterai", "Battery"),
    ("Kunci Layar", "Lock Screen"),
    //("Aplikasi", "Apps"),
    ("Kalkulator", "Calculator"),
    ("Buka Pengaturan", "Open Settings"),
    ("Klik kontrol untuk menambahkannya ke Control Center.", "Click a control to add it to Control Center."),

    ("Volume", "Volume"),
    ("Kecerahan", "Brightness"),
    ("Naikkan Volume", "Volume Up"),
    ("Turunkan Volume", "Volume Down"),
    ("Bisukan / Bunyikan", "Mute / Unmute"),
    ("Lebih Terang", "Brighter"),
    ("Lebih Redup", "Dimmer"),
    ("Tidak tersedia", "Unavailable"),
    ("Animasi jendela (minimize, tutup, buka)", "Window animations (minimize, close, open)"),
    ("Ukuran", "Layout"),
    ("Ukuran ikon", "Icon size"),
    ("Jarak antar ikon", "Icon spacing"),
    ("Zoom saat hover", "Hover zoom"),
    ("Jarak dari tepi bawah", "Distance from bottom edge"),
    ("Tampilan", "Appearance"),
    ("Perilaku", "Behavior"),
    ("Material", "Material"),
    ("Material notch", "Notch material"),
    ("Liquid Glass", "Liquid Glass"),
    ("Hitam buram", "Black blur"),
    ("Transparan", "Transparent"),
    ("Kebulatan sudut", "Corner roundness"),
    ("Jalankan saat login Windows", "Launch at Windows login"),
    ("Buka", "Open"),
    ("Tutup", "Close"),
    ("Simpan di dock", "Keep in dock"),
    ("Hapus dari dock", "Remove from dock"),
    ("Ubah ikon…", "Change icon…"),
    ("Buka menu Mulai", "Open Start menu"),
    ("Kembalikan ikon asli", "Restore original icon"),
    ("Tambah app ke dock…", "Add app to dock…"),
    ("Pilih aplikasi", "Choose an app"),
    ("Pilih gambar ikon", "Choose an icon image"),
    ("Aplikasi", "Applications"),
    ("Gambar", "Images"),
    ("Keluar dari ABRA", "Quit ABRA"),
    ("Sembunyikan taskbar (uji)", "Hide taskbar (test)"),
    ("Tidak ada media", "No media playing"),
];

/// Terjemahkan `src` (teks Indonesia) ke `lang`.
pub fn tr(lang: Lang, src: &'static str) -> &'static str {
    match lang {
        Lang::Id => src,
        Lang::En => EN.iter().find(|(k, _)| *k == src).map(|(_, v)| *v).unwrap_or(src),
    }
}

pub fn month_name(lang: Lang, m: u32, id: &'static str) -> &'static str {
    match lang {
        Lang::Id => id,
        Lang::En => {
            const N: [&str; 12] =
                ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
            N.get((m.max(1) - 1) as usize % 12).copied().unwrap_or(id)
        }
    }
}

pub fn weekday_labels(lang: Lang, monday_first: bool, id: [&'static str; 7]) -> [&'static str; 7] {
    match lang {
        Lang::Id => id,
        Lang::En => {
            if monday_first {
                ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
            } else {
                ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indonesian_is_identity_and_english_translates() {
        assert_eq!(tr(Lang::Id, "Buka"), "Buka");
        assert_eq!(tr(Lang::En, "Buka"), "Open");
        assert_eq!(tr(Lang::En, "tidak ada"), "tidak ada");
    }

    #[test]
    fn no_duplicate_keys() {
        for (i, (k, _)) in EN.iter().enumerate() {
            assert!(!EN[i + 1..].iter().any(|(k2, _)| k2 == k), "duplikat: {k}");
        }
    }

    #[test]
    fn calendar_names() {
        assert_eq!(month_name(Lang::En, 11, "November"), "November");
        assert_eq!(month_name(Lang::En, 1, "Januari"), "January");
        assert_eq!(weekday_labels(Lang::En, true, ["Sen", "Sel", "Rab", "Kam", "Jum", "Sab", "Min"])[0], "Mon");
    }
}
