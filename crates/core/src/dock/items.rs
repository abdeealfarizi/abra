//! Menggabungkan app yang dipin dengan window yang sedang berjalan menjadi item dock.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    pub path: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Win {
    pub hwnd: isize,
    pub path: String,
    pub title: String,
    /// Nama ramah dari metadata exe (FileDescription); kosong bila tidak ada.
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub key: String,
    pub name: String,
    pub path: String,
    pub pinned: bool,
    /// Window milik item ini, yang paling baru difokus lebih dulu.
    pub hwnds: Vec<isize>,
}

impl Item {
    pub fn running(&self) -> bool {
        !self.hwnds.is_empty()
    }
}

/// Kunci identitas: path dinormalkan (huruf kecil, garis miring ke belakang).
pub fn key_of(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
}

/// App UWP/Store berpath `shell:AppsFolder\<AUMID>`: stem path tidak berarti, pakai judul window.
fn display_name(w: &Win) -> String {
    if !w.name.trim().is_empty() {
        return w.name.trim().to_string();
    }
    if w.path.to_lowercase().starts_with("shell:") {
        let t = w.title.split(" - ").next().unwrap_or(&w.title).trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    name_from_path(&w.path)
}

pub fn name_from_path(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let stem = match file.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => file,
    };
    // buang akhiran teknis pada nama berkas (mis. "WhatsApp.Root" → "WhatsApp")
    for suffix in [".Root", ".Desktop", ".App", ".Win32", ".x64"] {
        if let Some(base) = stem.strip_suffix(suffix) {
            if !base.is_empty() {
                return base.to_string();
            }
        }
    }
    stem.to_string()
}

/// Urutan: semua pin (sesuai urutan pin), lalu app berjalan yang tidak dipin menurut
/// kemunculan pertamanya. `seen` menyimpan urutan itu antar pemanggilan, sehingga
/// ikon tidak berpindah tempat setiap kali ada window baru.
pub fn merge(pins: &[Pin], wins: &[Win], recent: &dyn Fn(isize) -> i64, seen: &mut Vec<String>) -> Vec<Item> {
    let mut items: Vec<Item> = pins
        .iter()
        .map(|p| Item { key: key_of(&p.path), name: p.name.clone(), path: p.path.clone(), pinned: true, hwnds: vec![] })
        .collect();

    let mut extra: Vec<Item> = Vec::new();
    for w in wins {
        let key = key_of(&w.path);
        let slot = items.iter_mut().chain(extra.iter_mut()).find(|i| i.key == key);
        match slot {
            Some(i) => i.hwnds.push(w.hwnd),
            None => extra.push(Item {
                key,
                name: display_name(w),
                path: w.path.clone(),
                pinned: false,
                hwnds: vec![w.hwnd],
            }),
        }
    }

    // urutan kemunculan untuk yang tidak dipin
    seen.retain(|k| extra.iter().any(|i| &i.key == k));
    for i in &extra {
        if !seen.contains(&i.key) {
            seen.push(i.key.clone());
        }
    }
    extra.sort_by_key(|i| seen.iter().position(|k| k == &i.key).unwrap_or(usize::MAX));
    items.extend(extra);

    for i in &mut items {
        i.hwnds.sort_by_key(|h| std::cmp::Reverse(recent(*h)));
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(p: &str) -> Pin {
        Pin { path: p.into(), name: name_from_path(p) }
    }
    fn win(h: isize, p: &str) -> Win {
        Win { hwnd: h, path: p.into(), title: String::new(), name: String::new() }
    }

    #[test]
    fn pins_come_first_in_pin_order_even_when_not_running() {
        let mut seen = vec![];
        let items = merge(&[pin("C:\\a.exe"), pin("C:\\b.exe")], &[], &|_| 0, &mut seen);
        assert_eq!(items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert!(items.iter().all(|i| i.pinned && !i.running()));
    }

    #[test]
    fn running_pinned_app_gets_its_windows_and_stays_in_place() {
        let mut seen = vec![];
        let items = merge(&[pin("C:\\a.exe"), pin("C:\\b.exe")], &[win(5, "c:\\B.EXE")], &|_| 0, &mut seen);
        assert!(!items[0].running());
        assert_eq!(items[1].hwnds, vec![5]);
    }

    #[test]
    fn unpinned_apps_keep_first_seen_order_across_calls() {
        let mut seen = vec![];
        merge(&[], &[win(1, "C:\\x.exe")], &|_| 0, &mut seen);
        let items = merge(&[], &[win(2, "C:\\y.exe"), win(1, "C:\\x.exe")], &|_| 0, &mut seen);
        assert_eq!(items[0].name, "x");
        assert_eq!(items[1].name, "y");
    }

    #[test]
    fn closed_apps_are_forgotten_so_they_reappear_last() {
        let mut seen = vec![];
        merge(&[], &[win(1, "C:\\x.exe"), win(2, "C:\\y.exe")], &|_| 0, &mut seen);
        merge(&[], &[win(2, "C:\\y.exe")], &|_| 0, &mut seen);
        let items = merge(&[], &[win(2, "C:\\y.exe"), win(1, "C:\\x.exe")], &|_| 0, &mut seen);
        assert_eq!(items[0].name, "y");
        assert_eq!(items[1].name, "x");
    }

    #[test]
    fn windows_of_one_app_are_grouped_and_sorted_by_recent_focus() {
        let mut seen = vec![];
        let recent = |h: isize| match h {
            10 => 100,
            11 => 300,
            12 => 200,
            _ => 0,
        };
        let items = merge(&[], &[win(10, "C:\\x.exe"), win(11, "C:\\x.exe"), win(12, "C:\\x.exe")], &recent, &mut seen);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].hwnds, vec![11, 12, 10]);
    }

    #[test]
    fn store_apps_are_named_from_their_window_title() {
        let mut seen = vec![];
        let w = Win { hwnd: 1, path: "shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into(), title: "Calculator - Standard".into(), name: String::new() };
        let items = merge(&[], &[w], &|_| 0, &mut seen);
        assert_eq!(items[0].name, "Calculator");
    }

    #[test]
    fn friendly_name_from_metadata_wins_over_file_stem() {
        let mut seen = vec![];
        let w = Win { hwnd: 1, path: "C:\\Office\\WINWORD.EXE".into(), title: "Doc1 - Word".into(), name: "Microsoft Word".into() };
        let items = merge(&[], &[w], &|_| 0, &mut seen);
        assert_eq!(items[0].name, "Microsoft Word");
    }

    #[test]
    fn names_come_from_file_stem() {
        assert_eq!(name_from_path("C:\\Program Files\\App\\my.app.exe"), "my.app");
        assert_eq!(name_from_path("noext"), "noext");
    }
}


