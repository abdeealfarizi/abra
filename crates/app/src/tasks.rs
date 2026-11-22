use nacre_core::todo::{Civil, List};
use std::sync::Mutex;

static LIST: Mutex<Option<List>> = Mutex::new(None);

fn file() -> std::path::PathBuf {
    crate::paths::config_dir().join("todo.txt")
}

pub fn now() -> Civil {
    let n = nacre_platform::time::local_now();
    Civil { year: n.year, month: n.month, day: n.day, hour: n.hour, minute: n.minute }
}

pub fn now_minutes() -> i64 {
    now().minutes()
}

pub fn ms_to_minute(target_m: i64) -> u32 {
    let n = nacre_platform::time::local_now();
    let now_m = Civil { year: n.year, month: n.month, day: n.day, hour: n.hour, minute: n.minute }.minutes();
    let ms = (target_m - now_m) * 60_000 - n.second as i64 * 1000 + 250;
    ms.clamp(250, 3_600_000) as u32
}

pub fn with<R>(f: impl FnOnce(&mut List) -> R) -> R {
    let mut g = LIST.lock().unwrap_or_else(|e| e.into_inner());
    let list = g.get_or_insert_with(|| std::fs::read_to_string(file()).map(|t| List::from_text(&t)).unwrap_or_default());
    f(list)
}

pub fn snapshot() -> List {
    with(|l| l.clone())
}

pub fn save() {
    let text = with(|l| l.to_text());
    let path = file();
    let write = || -> std::io::Result<()> {
        use std::io::Write;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, &path)
    };
    if let Err(e) = write() {
        nacre_platform::log::line("tugas", &format!("gagal menyimpan: {e}"));
    }
}

pub fn changed() {
    save();
    crate::notch::tasks_changed();
}

pub fn lang() -> nacre_core::i18n::Lang {
    crate::prefs::get().lang
}
