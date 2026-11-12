//! Penyedia media palsu dalam-proses untuk pengujian (`NACRE_FAKE_MEDIA=1`): tidak menyentuh sesi media pengguna.
//! Menanggapi perintah seperti sesi sungguhan (putar/jeda, berikutnya, sebelumnya, seek) dan mencatat tiap perintah.

use crate::icons::Bitmap;
use crate::media::{ArtUpdate, Command, Media, Snapshot};
use std::sync::mpsc::channel;
use std::time::Instant;

const TITLES: [&str; 3] = ["Lagu Uji A", "Lagu Uji B", "Lagu Uji C"];
const DUR: i64 = 180_000;

fn cover(idx: usize) -> Bitmap {
    // gradien berwarna per lagu, supaya pergantian lagu terlihat
    let n = 64usize;
    let mut b = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let o = (y * n + x) * 4;
            b[o] = (60 + idx * 70) as u8;
            b[o + 1] = (x * 3) as u8;
            b[o + 2] = (y * 3) as u8;
            b[o + 3] = 255;
        }
    }
    Bitmap { w: n as u32, h: n as u32, bgra: b }
}

pub fn spawn(notify: impl Fn() + Send + 'static) -> Media {
    let (cmd_tx, cmd_rx) = channel::<Command>();
    let (snap_tx, snap_rx) = channel::<Snapshot>();
    std::thread::spawn(move || {
        let (mut idx, mut playing, mut base_ms, mut since) = (0usize, true, 0i64, Instant::now());
        let mut art_dirty = true;
        let pos = |playing: bool, base: i64, since: Instant| (base + if playing { since.elapsed().as_millis() as i64 } else { 0 }).min(DUR);
        loop {
            let art = if art_dirty {
                art_dirty = false;
                ArtUpdate::New(cover(idx))
            } else {
                ArtUpdate::Unchanged
            };
            let _ = snap_tx.send(Snapshot {
                has_media: true,
                title: TITLES[idx].into(),
                artist: "Penguji".into(),
                playing,
                position_ms: pos(playing, base_ms, since),
                duration_ms: DUR,
                can_seek: true,
                art,
            });
            notify();
            let Ok(c) = cmd_rx.recv() else { return };
            match c {
                Command::PlayPause => {
                    base_ms = pos(playing, base_ms, since);
                    playing = !playing;
                    since = Instant::now();
                    crate::log::line("media-palsu", &format!("PlayPause -> playing={playing}"));
                }
                Command::Next | Command::Prev => {
                    let next = matches!(c, Command::Next);
                    idx = (idx + if next { 1 } else { TITLES.len() - 1 }) % TITLES.len();
                    base_ms = 0;
                    since = Instant::now();
                    art_dirty = true;
                    crate::log::line("media-palsu", &format!("{} -> {}", if next { "Next" } else { "Prev" }, TITLES[idx]));
                }
                Command::SeekMs(ms) => {
                    base_ms = ms.clamp(0, DUR);
                    since = Instant::now();
                    crate::log::line("media-palsu", &format!("Seek -> {ms} ms"));
                }
                Command::Refresh => {}
            }
        }
    });
    Media { commands: cmd_tx, snapshots: snap_rx }
}
