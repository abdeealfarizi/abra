//! LRCLIB (lrclib.net): basis data lirik terbuka, gratis, tanpa akun. Modul ini hanya menyusun alamat kueri, membaca
//! jawabannya, dan membuat kunci cache; permintaan jaringannya dilakukan di lapisan platform.
//!
//! Data yang dikirim hanya judul, artis, album, dan durasi lagu.

use super::json::{self, Json};

pub const API: &str = "https://lrclib.net/api";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_s: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub synced: Option<String>,
    pub plain: Option<String>,
    pub instrumental: bool,
}

impl Found {
    /// Teks untuk disimpan di cache dan diurai `lyrics::parse` (lirik tersinkron bila ada).
    pub fn text(&self) -> String {
        if self.instrumental {
            return String::new();
        }
        self.synced.clone().filter(|s| !s.trim().is_empty()).or_else(|| self.plain.clone()).unwrap_or_default()
    }
}

/// Percent-encoding RFC 3986 (selain karakter "unreserved").
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Kata yang menandai tambahan di judul video/streaming, bukan bagian nama lagu.
const NOISE: [&str; 16] = [
    "official", "video", "audio", "lyric", "lyrics", "visualizer", "visualiser", "mv", "m/v", "hd", "4k", "remaster", "remastered",
    "explicit", "clean", "color coded",
];

fn is_noise(inner: &str) -> bool {
    let l = inner.to_lowercase();
    NOISE.iter().any(|w| l.split(|c: char| !c.is_alphanumeric() && c != '/').any(|t| t == *w) || (w.contains(' ') && l.contains(w)))
}

/// Rapikan judul dan artis dari sesi media: "Artis - Judul (Official Video)" dari browser, "Artis - Topic", "ArtisVEVO",
/// atau "Judul - Remastered 2011".
pub fn clean(title: &str, artist: &str) -> (String, String) {
    let mut artist = artist.trim().to_string();
    for suffix in [" - Topic", "VEVO"] {
        if artist.len() > suffix.len() && artist.to_lowercase().ends_with(&suffix.to_lowercase()) {
            artist.truncate(artist.len() - suffix.len());
            artist = artist.trim().to_string();
        }
    }
    // buang (…) / […] yang berisi kata tambahan
    let mut t = String::new();
    let mut rest = title.trim();
    while let Some(i) = rest.find(['(', '[']) {
        let close = if rest.as_bytes()[i] == b'(' { ')' } else { ']' };
        match rest[i + 1..].find(close) {
            Some(j) if is_noise(&rest[i + 1..i + 1 + j]) => {
                t.push_str(&rest[..i]);
                rest = &rest[i + 2 + j..];
            }
            _ => {
                t.push_str(&rest[..=i]);
                rest = &rest[i + 1..];
            }
        }
    }
    t.push_str(rest);
    let mut title = t.split_whitespace().collect::<Vec<_>>().join(" ");
    // "Artis - Judul": pisahkan bila artis kosong atau judul diawali nama artis
    for sep in [" - ", " – ", " — "] {
        if let Some((left, right)) = title.split_once(sep) {
            let (l, r) = (left.trim().to_string(), right.trim().to_string());
            if artist.is_empty() {
                artist = l;
                title = r;
            } else if l.eq_ignore_ascii_case(&artist) {
                title = r;
            }
            break;
        }
    }
    // "Judul - Remastered 2011"
    for sep in [" - ", " – "] {
        if let Some((left, right)) = title.split_once(sep) {
            if is_noise(right) {
                title = left.trim().to_string();
            }
        }
    }
    (title, artist)
}

fn query(t: &Track, with_album: bool) -> String {
    let mut q = format!("track_name={}&artist_name={}", encode(&t.title), encode(&t.artist));
    if with_album && !t.album.trim().is_empty() {
        q.push_str(&format!("&album_name={}", encode(&t.album)));
    }
    if let Some(d) = t.duration_s {
        q.push_str(&format!("&duration={d}"));
    }
    q
}

/// Pencocokan tepat (butuh album dan durasi); `None` bila datanya tidak cukup.
pub fn get_url(t: &Track) -> Option<String> {
    (!t.title.is_empty() && !t.artist.is_empty() && !t.album.trim().is_empty() && t.duration_s.is_some())
        .then(|| format!("{API}/get?{}", query(t, true)))
}

/// Pencarian longgar (judul + artis); hasilnya dipilih dengan [`pick_search`].
pub fn search_url(t: &Track) -> Option<String> {
    (!t.title.is_empty()).then(|| {
        let mut q = format!("track_name={}", encode(&t.title));
        if !t.artist.is_empty() {
            q.push_str(&format!("&artist_name={}", encode(&t.artist)));
        }
        format!("{API}/search?{q}")
    })
}

fn found(o: &Json) -> Option<(Found, Option<f64>)> {
    if !matches!(o, Json::Obj(_)) {
        return None;
    }
    let s = |k: &str| o.get(k).and_then(Json::as_str).map(str::to_string).filter(|s| !s.trim().is_empty());
    let f = Found { synced: s("syncedLyrics"), plain: s("plainLyrics"), instrumental: o.get("instrumental").and_then(Json::as_bool).unwrap_or(false) };
    if f.synced.is_none() && f.plain.is_none() && !f.instrumental {
        return None;
    }
    Some((f, o.get("duration").and_then(Json::as_f64)))
}

/// Jawaban `/api/get` (satu objek).
pub fn parse_get(body: &str) -> Option<Found> {
    found(&json::parse(body)?).map(|(f, _)| f)
}

/// Pilih hasil `/api/search` terbaik: durasi harus dekat (±8 detik) bila diketahui, utamakan lirik tersinkron, lalu yang
/// durasinya paling dekat.
pub fn pick_search(body: &str, duration_s: Option<u32>) -> Option<Found> {
    let j = json::parse(body)?;
    let mut best: Option<(Found, (bool, i64))> = None;
    for o in j.as_arr()? {
        let Some((f, d)) = found(o) else { continue };
        let diff = match (duration_s, d) {
            (Some(want), Some(have)) => {
                let x = (have - want as f64).abs();
                if x > 8.0 {
                    continue;
                }
                (x * 1000.0) as i64
            }
            _ => 0,
        };
        // kunci urut: lirik tersinkron dulu (false < true, jadi dibalik), lalu selisih durasi terkecil
        let key = (f.synced.is_none(), diff);
        if best.as_ref().is_none_or(|(_, k)| key < *k) {
            best = Some((f, key));
        }
    }
    best.map(|(f, _)| f)
}

/// Nama berkas cache untuk lagu ini (FNV-1a 64-bit dari data yang dinormalkan).
pub fn cache_key(t: &Track) -> String {
    let norm = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        t.artist.trim().to_lowercase(),
        t.title.trim().to_lowercase(),
        t.album.trim().to_lowercase(),
        t.duration_s.unwrap_or(0)
    );
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in norm.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr(title: &str, artist: &str, album: &str, d: Option<u32>) -> Track {
        Track { title: title.into(), artist: artist.into(), album: album.into(), duration_s: d }
    }

    #[test]
    fn encodes_like_rfc3986() {
        assert_eq!(encode("Rock & Roll/é ~ok"), "Rock%20%26%20Roll%2F%C3%A9%20~ok");
    }

    #[test]
    fn urls_include_only_what_is_known() {
        let t = tr("Clocks", "Coldplay", "A Rush of Blood to the Head", Some(307));
        assert_eq!(
            get_url(&t).unwrap(),
            "https://lrclib.net/api/get?track_name=Clocks&artist_name=Coldplay&album_name=A%20Rush%20of%20Blood%20to%20the%20Head&duration=307"
        );
        assert_eq!(get_url(&tr("Clocks", "Coldplay", "", Some(307))), None, "tanpa album: pakai pencarian");
        assert_eq!(search_url(&tr("Clocks", "Coldplay", "", None)).unwrap(), "https://lrclib.net/api/search?track_name=Clocks&artist_name=Coldplay");
        assert_eq!(search_url(&tr("", "Coldplay", "", None)), None);
    }

    #[test]
    fn cleans_streaming_titles() {
        assert_eq!(clean("Coldplay - Clocks (Official Video)", ""), ("Clocks".into(), "Coldplay".into()));
        assert_eq!(clean("Coldplay - Clocks [Lyrics]", "Coldplay"), ("Clocks".into(), "Coldplay".into()));
        assert_eq!(clean("Clocks", "Coldplay - Topic"), ("Clocks".into(), "Coldplay".into()));
        assert_eq!(clean("Clocks", "ColdplayVEVO"), ("Clocks".into(), "Coldplay".into()));
        assert_eq!(clean("Here Comes the Sun - Remastered 2009", "The Beatles"), ("Here Comes the Sun".into(), "The Beatles".into()));
        // keterangan yang bukan tambahan dipertahankan
        assert_eq!(clean("Stay (feat. Justin Bieber)", "The Kid LAROI"), ("Stay (feat. Justin Bieber)".into(), "The Kid LAROI".into()));
        assert_eq!(clean("Anti-Hero", "Taylor Swift"), ("Anti-Hero".into(), "Taylor Swift".into()));
    }

    #[test]
    fn reads_get_and_picks_the_best_search_result() {
        let one = r#"{"id":1,"trackName":"Clocks","duration":307.0,"instrumental":false,"plainLyrics":"Lights go out","syncedLyrics":"[00:10.00]Lights go out"}"#;
        let f = parse_get(one).unwrap();
        assert_eq!(f.text(), "[00:10.00]Lights go out");
        let many = r#"[
            {"duration":200.0,"plainLyrics":"wrong song","syncedLyrics":"[00:01.00]wrong"},
            {"duration":309.0,"plainLyrics":"plain only","syncedLyrics":null},
            {"duration":306.0,"plainLyrics":"p","syncedLyrics":"[00:02.00]synced"},
            {"duration":307.0,"plainLyrics":null,"syncedLyrics":null,"instrumental":false}
        ]"#;
        assert_eq!(pick_search(many, Some(307)).unwrap().text(), "[00:02.00]synced");
        assert_eq!(pick_search(r#"[{"duration":200.0,"plainLyrics":"x"}]"#, Some(307)), None, "durasi terlalu jauh");
        let inst = parse_get(r#"{"instrumental":true,"plainLyrics":null,"syncedLyrics":null}"#).unwrap();
        assert_eq!(inst.text(), "");
        assert_eq!(parse_get(r#"{"code":404,"name":"TrackNotFound"}"#), None);
    }

    #[test]
    fn cache_key_is_stable_and_case_insensitive() {
        let a = cache_key(&tr("Clocks", "Coldplay", "X", Some(307)));
        assert_eq!(a, cache_key(&tr(" clocks ", "COLDPLAY", "x", Some(307))));
        assert_ne!(a, cache_key(&tr("Clocks", "Coldplay", "X", Some(308))));
        assert_eq!(a.len(), 16);
    }
}
