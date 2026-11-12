//! Sesi media sistem (SMTC) berbasis event: tanpa polling. Satu thread pekerja memegang manajer sesi,
//! berlangganan perubahan, menggabungkan (debounce) lalu mengirim `Snapshot` ke thread UI.

use crate::icons::Bitmap;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    CurrentSessionChangedEventArgs, GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager, GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    MediaPropertiesChangedEventArgs, PlaybackInfoChangedEventArgs, SessionsChangedEventArgs,
    TimelinePropertiesChangedEventArgs,
};
use windows::Storage::Streams::{DataReader, IRandomAccessStreamReference};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

pub enum ArtUpdate {
    /// Lagu sama: artwork lama tetap dipakai.
    Unchanged,
    None,
    New(Bitmap),
}

pub struct Snapshot {
    pub has_media: bool,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub can_seek: bool,
    pub art: ArtUpdate,
}

pub enum Command {
    Refresh,
    PlayPause,
    Next,
    Prev,
    SeekMs(i64),
}

pub struct Media {
    pub commands: Sender<Command>,
    pub snapshots: Receiver<Snapshot>,
}

fn read_art(r: Option<IRandomAccessStreamReference>) -> Option<Vec<u8>> {
    let stream = r?.OpenReadAsync().ok()?.join().ok()?;
    let reader = DataReader::CreateDataReader(&stream).ok()?;
    let mut all = Vec::new();
    loop {
        let n = reader.LoadAsync(65536).ok()?.join().ok()?;
        if n == 0 {
            break;
        }
        let mut chunk = vec![0u8; n as usize];
        reader.ReadBytes(&mut chunk).ok()?;
        all.extend_from_slice(&chunk);
        if all.len() > 8 * 1024 * 1024 {
            return None; // artwork tak masuk akal besarnya
        }
    }
    (!all.is_empty()).then_some(all)
}

struct Subscriptions {
    session: Option<GlobalSystemMediaTransportControlsSession>,
    tokens: (i64, i64, i64),
}

impl Subscriptions {
    fn drop_session(&mut self) {
        if let Some(s) = self.session.take() {
            let _ = s.RemoveMediaPropertiesChanged(self.tokens.0);
            let _ = s.RemovePlaybackInfoChanged(self.tokens.1);
            let _ = s.RemoveTimelinePropertiesChanged(self.tokens.2);
        }
    }

    fn follow(&mut self, session: Option<GlobalSystemMediaTransportControlsSession>, tx: &Sender<Command>) {
        let same = match (&self.session, &session) {
            (Some(a), Some(b)) => a.SourceAppUserModelId().ok() == b.SourceAppUserModelId().ok(),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.drop_session();
        let Some(s) = session else { return };
        let t1 = tx.clone();
        let a = s.MediaPropertiesChanged(&TypedEventHandler::<_, MediaPropertiesChangedEventArgs>::new(move |_, _| {
            let _ = t1.send(Command::Refresh);
            Ok(())
        }));
        let t2 = tx.clone();
        let b = s.PlaybackInfoChanged(&TypedEventHandler::<_, PlaybackInfoChangedEventArgs>::new(move |_, _| {
            let _ = t2.send(Command::Refresh);
            Ok(())
        }));
        let t3 = tx.clone();
        let c = s.TimelinePropertiesChanged(&TypedEventHandler::<_, TimelinePropertiesChangedEventArgs>::new(move |_, _| {
            let _ = t3.send(Command::Refresh);
            Ok(())
        }));
        self.tokens = (a.unwrap_or(0), b.unwrap_or(0), c.unwrap_or(0));
        self.session = Some(s);
    }
}

/// Mulai pekerja. `notify` dipanggil (dari thread pekerja) setelah snapshot masuk antrean.
#[allow(unused_assignments)]
pub fn spawn(notify: impl Fn() + Send + 'static) -> Media {
    let (cmd_tx, cmd_rx) = channel::<Command>();
    let (snap_tx, snap_rx) = channel::<Snapshot>();
    let tx_for_events = cmd_tx.clone();
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let Some(manager) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync().ok().and_then(|o| o.join().ok())
        else {
            crate::log::line("media", "manajer sesi tidak tersedia");
            return;
        };
        let t = tx_for_events.clone();
        let _ = manager.CurrentSessionChanged(&TypedEventHandler::<_, CurrentSessionChangedEventArgs>::new(move |_, _| {
            let _ = t.send(Command::Refresh);
            Ok(())
        }));
        let t = tx_for_events.clone();
        let _ = manager.SessionsChanged(&TypedEventHandler::<_, SessionsChangedEventArgs>::new(move |_, _| {
            let _ = t.send(Command::Refresh);
            Ok(())
        }));

        let mut subs = Subscriptions { session: None, tokens: (0, 0, 0) };
        let mut last_key = String::new();
        let mut pending = true; // snapshot awal

        loop {
            if pending {
                pending = false;
                let current = manager.GetCurrentSession().ok().or_else(|| {
                    manager.GetSessions().ok().and_then(|v| v.GetAt(0).ok())
                });
                subs.follow(current.clone(), &tx_for_events);
                let snap = build(current.as_ref(), &mut last_key);
                if snap_tx.send(snap).is_err() {
                    return;
                }
                notify();
            }
            match cmd_rx.recv() {
                Ok(Command::Refresh) => {
                    // gabungkan semburan event dalam 80 ms menjadi satu pembaruan
                    loop {
                        match cmd_rx.recv_timeout(Duration::from_millis(80)) {
                            Ok(Command::Refresh) => continue,
                            Ok(other) => {
                                act(&manager, other);
                                break;
                            }
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    pending = true;
                }
                Ok(other) => {
                    act(&manager, other);
                    // beri waktu aplikasi memperbarui statusnya, lalu baca ulang
                    std::thread::sleep(Duration::from_millis(120));
                    pending = true;
                }
                Err(_) => return,
            }
        }
    });
    Media { commands: cmd_tx, snapshots: snap_rx }
}

fn act(manager: &GlobalSystemMediaTransportControlsSessionManager, cmd: Command) {
    let Ok(s) = manager.GetCurrentSession() else { return };
    match cmd {
        Command::PlayPause => {
            let _ = s.TryTogglePlayPauseAsync();
        }
        Command::Next => {
            let _ = s.TrySkipNextAsync();
        }
        Command::Prev => {
            let _ = s.TrySkipPreviousAsync();
        }
        // satuan SMTC: tick 100 ns
        Command::SeekMs(ms) => {
            let _ = s.TryChangePlaybackPositionAsync(ms * 10_000);
        }
        Command::Refresh => {}
    }
}

fn build(session: Option<&GlobalSystemMediaTransportControlsSession>, last_key: &mut String) -> Snapshot {
    let empty = |last_key: &mut String| {
        last_key.clear();
        Snapshot {
            has_media: false,
            title: String::new(),
            artist: String::new(),
            playing: false,
            position_ms: 0,
            duration_ms: 0,
            can_seek: false,
            art: ArtUpdate::None,
        }
    };
    let Some(s) = session else { return empty(last_key) };
    let Some(props) = s.TryGetMediaPropertiesAsync().ok().and_then(|o| o.join().ok()) else { return empty(last_key) };
    let title = props.Title().map(|t| t.to_string()).unwrap_or_default();
    if title.is_empty() {
        return empty(last_key);
    }
    let artist = props.Artist().map(|t| t.to_string()).unwrap_or_default();

    let key = format!("{title}\u{1}{artist}");
    let art = if *last_key == key {
        ArtUpdate::Unchanged
    } else {
        *last_key = key;
        match read_art(props.Thumbnail().ok()).and_then(|b| crate::wic::decode_square(&b, 128)) {
            Some(bmp) => ArtUpdate::New(bmp),
            None => ArtUpdate::None,
        }
    };

    let playing = s
        .GetPlaybackInfo()
        .ok()
        .and_then(|p| p.PlaybackStatus().ok())
        .is_some_and(|st| st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing);
    let can_seek = s
        .GetPlaybackInfo()
        .ok()
        .and_then(|p| p.Controls().ok())
        .and_then(|c| c.IsPlaybackPositionEnabled().ok())
        .unwrap_or(false);
    let (position_ms, duration_ms) = match s.GetTimelineProperties() {
        Ok(t) => {
            let ms = |r: windows::core::Result<windows::Foundation::TimeSpan>| r.map(|v| v.Duration / 10_000).unwrap_or(0);
            let (start, end, pos) = (ms(t.StartTime()), ms(t.EndTime()), ms(t.Position()));
            (pos, (end - start).max(0))
        }
        Err(_) => (0, 0),
    };
    Snapshot { has_media: true, title, artist, playing, position_ms, duration_ms, can_seek, art }
}



/// Jeda sesi media milik app `exe_stem` (mis. "chrome", "spotify") yang sedang memutar. Berjalan di thread sendiri;
/// mengembalikan lewat `done` daftar id sumber yang dijeda (untuk dilanjutkan nanti).
pub fn pause_app(exe_stem: String, done: impl FnOnce(Vec<String>) + Send + 'static) {
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut paused = Vec::new();
        if let Some(m) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync().ok().and_then(|o| o.join().ok()) {
            if let Ok(list) = m.GetSessions() {
                for s in list {
                    let id = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
                    if !id.to_ascii_lowercase().contains(&exe_stem) {
                        continue;
                    }
                    let playing = s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).map(|p| p == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing).unwrap_or(false);
                    if playing && s.TryPauseAsync().and_then(|o| o.join()).unwrap_or(false) {
                        paused.push(id);
                    }
                }
            }
        }
        done(paused);
    });
}

/// Lanjutkan sesi media yang sebelumnya dijeda oleh `pause_app` (berdasarkan id sumber).
pub fn resume_ids(ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        if let Some(m) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync().ok().and_then(|o| o.join().ok()) {
            if let Ok(list) = m.GetSessions() {
                for s in list {
                    let id = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
                    if ids.contains(&id) {
                        let _ = s.TryPlayAsync().and_then(|o| o.join());
                    }
                }
            }
        }
    });
}
