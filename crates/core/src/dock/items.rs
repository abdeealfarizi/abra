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
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub key: String,
    pub name: String,
    pub path: String,
    pub pinned: bool,
    pub hwnds: Vec<isize>,
}

impl Item {
    pub fn running(&self) -> bool {
        !self.hwnds.is_empty()
    }
}

pub fn is_separator(path: &str) -> bool {
    let t = path.trim();
    t == "---" || t.to_ascii_lowercase().starts_with("sep:")
}

pub fn next_separator_path(existing: &[Pin]) -> String {
    let mut n = 1usize;
    loop {
        let cand = format!("sep:{n}");
        if !existing.iter().any(|p| p.path.eq_ignore_ascii_case(&cand)) {
            return cand;
        }
        n += 1;
    }
}

pub fn key_of(path: &str) -> String {
    let lower = path.to_lowercase();
    if let Some(rest) = lower.strip_prefix("webapp:") {
        let id = rest.split('|').next().unwrap_or(rest);
        return format!("webapp:{id}");
    }
    path.replace('/', "\\").to_lowercase().split('\\').map(stable_segment).collect::<Vec<_>>().join("\\")
}

fn is_version(s: &str) -> bool {
    s.contains('.') && s.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn package_parts(seg: &str) -> Option<(&str, &str, &str)> {
    let parts: Vec<&str> = seg.split('_').collect();
    if parts.len() == 5 && is_version(parts[1]) && !parts[0].is_empty() && !parts[4].is_empty() {
        Some((parts[0], parts[3], parts[4]))
    } else {
        None
    }
}

fn stable_segment(seg: &str) -> String {
    if let Some(v) = seg.strip_prefix("app-") {
        if is_version(v) || v.chars().all(|c| c.is_ascii_digit()) && !v.is_empty() {
            return "app-*".to_string();
        }
    }
    match package_parts(seg) {
        Some((name, resource, publisher)) => format!("{name}__{resource}__{publisher}"),
        None => seg.to_string(),
    }
}

pub fn package_family(path: &str) -> Option<String> {
    let p = path.replace('/', "\\");
    let lower = p.to_lowercase();
    let at = lower.find("\\windowsapps\\")? + "\\windowsapps\\".len();
    let seg = p[at..].split('\\').next()?;
    let (name, _, publisher) = package_parts(seg)?;
    Some(format!("{name}_{publisher}"))
}

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
    if let Some(rest) = path.strip_prefix("webapp:") {
        if let Some(name) = rest.split('|').nth(3).filter(|s| !s.trim().is_empty()) {
            return name.trim().to_string();
        }
    }
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path);
    let stem = match file.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => file,
    };
    for suffix in [".Root", ".Desktop", ".App", ".Win32", ".x64"] {
        if let Some(base) = stem.strip_suffix(suffix) {
            if !base.is_empty() {
                return base.to_string();
            }
        }
    }
    stem.to_string()
}

pub fn merge(pins: &[Pin], wins: &[Win], recent: &dyn Fn(isize) -> i64, seen: &mut Vec<String>) -> Vec<Item> {
    let mut items: Vec<Item> = pins
        .iter()
        .map(|p| Item { key: key_of(&p.path), name: p.name.clone(), path: p.path.clone(), pinned: true, hwnds: vec![] })
        .collect();

    let mut extra: Vec<Item> = Vec::new();
    for w in wins {
        let key = key_of(&w.path);
        if is_separator(&w.path) {
            continue;
        }
        let slot = items.iter_mut().chain(extra.iter_mut()).find(|i| i.key == key);
        match slot {
            Some(i) => {
                if i.hwnds.is_empty() && !w.path.is_empty() {
                    i.path = w.path.clone();
                }
                i.hwnds.push(w.hwnd);
            }
            None => extra.push(Item {
                key,
                name: display_name(w),
                path: w.path.clone(),
                pinned: false,
                hwnds: vec![w.hwnd],
            }),
        }
    }

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
    fn updated_store_app_still_matches_its_old_pin() {
        let old = "C:\\Program Files\\WindowsApps\\Claude_5.11492.2.0_x64__pzs8sxrjxfjjc\\app\\claude.exe";
        let new = "C:\\Program Files\\WindowsApps\\Claude_5.11492.5.0_x64__pzs8sxrjxfjjc\\app\\claude.exe";
        assert_eq!(key_of(old), key_of(new));
        let mut seen = vec![];
        let items = merge(&[pin(old)], &[win(7, new)], &|_| 0, &mut seen);
        assert_eq!(items.len(), 1);
        assert!(items[0].pinned && items[0].running());
        assert_eq!(items[0].path, new);
        assert_eq!(package_family(old).as_deref(), Some("Claude_pzs8sxrjxfjjc"));
        assert_eq!(package_family("C:\\Windows\\notepad.exe"), None);
    }

    #[test]
    fn user_separators_stay_in_pinned_order_and_webapp_keys_are_distinct() {
        let mut seen = vec![];
        let pins = vec![pin("C:\\a.exe"), Pin { path: "sep:1".into(), name: "---".into() }, pin("C:\\b.exe")];
        let items = merge(&pins, &[win(1, "webapp:tab1|firefox -tab 1|icon1|Docs"), win(2, "webapp:tab2|firefox -tab 2|icon2|Mail")], &|_| 0, &mut seen);
        assert_eq!(items.len(), 5);
        assert!(is_separator(&items[1].path));
        assert!(items[1].pinned);
        assert_ne!(items[3].key, items[4].key);
        assert_eq!(items[3].name, "Docs");
    }

    #[test]
    fn squirrel_versioned_folder_is_ignored_in_key() {
        assert_eq!(key_of("C:\\Users\\u\\AppData\\Local\\Discord\\app-1.0.9001\\Discord.exe"), key_of("C:\\Users\\u\\AppData\\Local\\Discord\\app-1.0.9002\\Discord.exe"));
        assert_ne!(key_of("C:\\a\\app-data\\x.exe"), key_of("C:\\a\\app-1.0\\x.exe"));
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
