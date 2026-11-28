use nacre_core::dock::items::{is_separator, name_from_path, Pin};
use std::io::Write;
use std::path::Path;

pub fn name_for(path: &str) -> String {
    if is_separator(path) {
        return "---".to_string();
    }
    let n = nacre_platform::wins::names::friendly(path);
    if n.is_empty() {
        name_from_path(path)
    } else {
        n
    }
}

fn default_pins() -> Vec<Pin> {
    let win = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let explorer = format!("{win}\\explorer.exe");
    vec![Pin { name: name_for(&explorer), path: explorer }]
}

pub fn load(path: &Path) -> Vec<Pin> {
    match std::fs::read_to_string(path) {
        Ok(text) => text
            .trim_start_matches('\u{feff}')
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .enumerate()
            .map(|(idx, l)| {
                if l == "---" || l.starts_with("---\t") {
                    Pin { path: format!("sep:{}", idx + 1), name: "---".to_string() }
                } else if l.to_ascii_lowercase().starts_with("sep:") {
                    let p = l.split('\t').next().unwrap_or(l).trim().to_string();
                    Pin { path: p, name: "---".to_string() }
                } else {
                    match l.split_once('\t') {
                        Some((p, n)) if !n.trim().is_empty() => Pin { path: p.to_string(), name: n.trim().to_string() },
                        _ => Pin { path: l.to_string(), name: name_for(l) },
                    }
                }
            })
            .collect(),
        Err(_) => default_pins(),
    }
}

pub fn save(path: &Path, pins: &[Pin]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp)?;
    for p in pins {
        writeln!(f, "{}\t{}", p.path, p.name)?;
    }
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)
}
