//! Server HTTP lokal sementara untuk "Kirim ke HP": HP memindai QR lalu mengunduh berkas lewat browser.
//! - Hanya mendengarkan di alamat IPv4 privat milik laptop (Wi-Fi/LAN), port acak, path rahasia `/<token>/`.
//! - Hanya melayani berkas yang ditaruh pengguna; selain itu 404. Tanpa internet, tanpa server luar.
//! - Berhenti saat `Server` di-drop (Selesai, batas waktu, atau ABRA ditutup).

use nacre_core::notch::send::{ascii_name, is_private, page, pct, token, url};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Batas agar perangkat lain di LAN tidak bisa menghabiskan memori/thread ABRA (rilis memakai panic="abort").
const MAX_HEAD: u64 = 8 * 1024;
const MAX_CONN: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// Byte unik terkirim (bagian terjauh tiap berkas) dan total semua berkas.
    pub sent: u64,
    pub total: u64,
    /// Berkas yang sudah terkirim utuh.
    pub files_done: usize,
    /// Ada transfer yang sedang berjalan.
    pub active: bool,
}

pub struct Server {
    pub url: String,
    pub progress: Receiver<Progress>,
    stop: Arc<AtomicBool>,
    addr: SocketAddr,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // bangunkan accept() yang sedang menunggu
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(300));
    }
}

/// Alamat IPv4 laptop di jaringan lokal: rute ke luar dipilih lewat `connect` UDP (tidak ada paket yang dikirim).
fn lan_ip() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    match s.local_addr().ok()? {
        SocketAddr::V4(a) if is_private(a.ip().octets()) => Some(*a.ip()),
        _ => None,
    }
}

fn random10() -> [u8; 10] {
    let mut b = [0u8; 10];
    unsafe {
        let _ = windows::Win32::Security::Cryptography::BCryptGenRandom(None, &mut b, windows::Win32::Security::Cryptography::BCRYPT_USE_SYSTEM_PREFERRED_RNG);
    }
    b
}

struct Shared {
    files: Vec<(String, String, u64)>, // (path, nama, ukuran)
    reached: Mutex<Vec<u64>>,
    active: Mutex<usize>,
    tx: Mutex<Sender<Progress>>,
    last: Mutex<Instant>,
    notify: Box<dyn Fn() + Send + Sync>,
}

impl Shared {
    fn report(&self, force: bool) {
        let mut last = self.last.lock().unwrap();
        if !force && last.elapsed() < Duration::from_millis(250) {
            return;
        }
        *last = Instant::now();
        let reached = self.reached.lock().unwrap();
        let total: u64 = self.files.iter().map(|f| f.2).sum();
        let p = Progress {
            sent: reached.iter().sum(),
            total,
            files_done: reached.iter().zip(&self.files).filter(|(r, f)| **r >= f.2).count(),
            active: *self.active.lock().unwrap() > 0,
        };
        let _ = self.tx.lock().unwrap().send(p);
        (self.notify)();
    }
}

/// Satu koneksi aktif; slot dilepas saat thread selesai (termasuk saat panik).
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn take(live: &Arc<AtomicUsize>) -> Slot {
        live.fetch_add(1, Ordering::Relaxed);
        Slot(live.clone())
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Pembaca dengan batas waktu total (bukan per `read`), agar pengirim yang menetes 1 byte/detik tetap dihentikan.
struct Deadline(TcpStream, Instant);

impl Read for Deadline {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        let left = self.1.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        self.0.set_read_timeout(Some(left))?;
        self.0.read(b)
    }
}

fn respond(s: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) {
    let head = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
    let _ = s.write_all(head.as_bytes());
    let _ = s.write_all(body);
}

fn serve(mut s: TcpStream, tok: &str, sh: &Shared, en: bool) {
    let _ = s.set_write_timeout(Some(Duration::from_secs(30)));
    let Ok(clone) = s.try_clone() else { return };
    // baris permintaan + header dibatasi MAX_HEAD byte dan 10 detik total
    let mut r = BufReader::new(Deadline(clone, Instant::now() + Duration::from_secs(10)).take(MAX_HEAD));
    let mut line = String::new();
    if r.read_line(&mut line).is_err() {
        return;
    }
    let mut done = false;
    loop {
        let mut h = Vec::new();
        match r.read_until(b'\n', &mut h) {
            Ok(0) | Err(_) => break,
            Ok(_) if h == b"\r\n" || h == b"\n" => {
                done = true;
                break;
            }
            _ => {}
        }
    }
    if !done {
        respond(&mut s, "431 Request Header Fields Too Large", "text/plain", b"");
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if method != "GET" {
        respond(&mut s, "405 Method Not Allowed", "text/plain", b"");
        return;
    }
    let base = format!("/{tok}/");
    if path == format!("/{tok}") || path == base {
        let list: Vec<(String, u64)> = sh.files.iter().map(|f| (f.1.clone(), f.2)).collect();
        respond(&mut s, "200 OK", "text/html; charset=utf-8", page(tok, &list, en).as_bytes());
        return;
    }
    let Some(i) = path.strip_prefix(&format!("{base}f/")).and_then(|n| n.parse::<usize>().ok()).filter(|i| *i < sh.files.len()) else {
        respond(&mut s, "404 Not Found", "text/plain", b"");
        return;
    };
    let (fpath, name, size) = &sh.files[i];
    let Ok(mut f) = std::fs::File::open(fpath) else {
        respond(&mut s, "404 Not Found", "text/plain", b"");
        return;
    };
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {size}\r\nContent-Disposition: attachment; filename=\"{}\"; filename*=UTF-8''{}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        ascii_name(name),
        pct(name)
    );
    if s.write_all(head.as_bytes()).is_err() {
        return;
    }
    *sh.active.lock().unwrap() += 1;
    sh.report(true);
    let mut buf = vec![0u8; 64 * 1024];
    let mut sent = 0u64;
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if s.write_all(&buf[..n]).is_err() {
            break;
        }
        sent += n as u64;
        {
            let mut reached = sh.reached.lock().unwrap();
            reached[i] = reached[i].max(sent.min(*size));
        }
        sh.report(false);
    }
    *sh.active.lock().unwrap() -= 1;
    sh.report(true);
}

/// Mulai sesi untuk `paths`. Gagal bila laptop tidak di jaringan lokal (teks alasannya untuk log).
pub fn start(paths: &[String], en: bool, notify: impl Fn() + Send + Sync + 'static) -> Result<Server, String> {
    let files: Vec<(String, String, u64)> = paths
        .iter()
        .filter_map(|p| {
            let md = std::fs::metadata(p).ok().filter(|m| m.is_file())?;
            let name = std::path::Path::new(p).file_name()?.to_string_lossy().to_string();
            Some((p.clone(), name, md.len()))
        })
        .collect();
    if files.is_empty() {
        return Err("tidak ada berkas (folder belum didukung)".into());
    }
    // khusus uji otomatis: hanya 127.0.0.1 (tanpa dialog Windows Firewall, tak terjangkau perangkat lain)
    let ip = if std::env::var_os("NACRE_SEND_LOOPBACK").is_some() { Ipv4Addr::LOCALHOST } else { lan_ip().ok_or("tidak tersambung ke jaringan lokal")? };
    let listener = TcpListener::bind((ip, 0)).map_err(|e| e.to_string())?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let tok = token(&random10());
    let link = url(ip.octets(), addr.port(), &tok);
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let n = files.len();
    let sh = Arc::new(Shared { files, reached: Mutex::new(vec![0; n]), active: Mutex::new(0), tx: Mutex::new(tx), last: Mutex::new(Instant::now()), notify: Box::new(notify) });
    let stop2 = stop.clone();
    let live = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            if stop2.load(Ordering::Relaxed) {
                break;
            }
            let Ok(c) = conn else { continue };
            // hanya perangkat di jaringan lokal
            let ok = matches!(c.peer_addr(), Ok(SocketAddr::V4(a)) if is_private(a.ip().octets()) || a.ip().is_loopback());
            // lebih dari MAX_CONN koneksi: langsung ditutup
            if !ok || live.load(Ordering::Relaxed) >= MAX_CONN {
                continue;
            }
            let slot = Slot::take(&live);
            let (sh, tok) = (sh.clone(), tok.clone());
            let _ = std::thread::Builder::new().spawn(move || {
                let _slot = slot;
                serve(c, &tok, &sh, en)
            });
        }
    });
    crate::log::line("kirim", &format!("sesi dimulai: {n} berkas, port {}", addr.port()));
    Ok(Server { url: link, progress: rx, stop, addr })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(tag: &str) -> Server {
        std::env::set_var("NACRE_SEND_LOOPBACK", "1");
        let p = std::env::temp_dir().join(format!("nacre-send-{tag}-{}.txt", std::process::id()));
        std::fs::write(&p, b"halo").unwrap();
        start(&[p.to_string_lossy().to_string()], true, || {}).unwrap()
    }

    fn ask(addr: SocketAddr, req: &[u8]) -> String {
        let mut c = TcpStream::connect(addr).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let _ = c.write_all(req);
        let mut out = String::new();
        let _ = c.read_to_string(&mut out);
        out
    }

    #[test]
    fn header_kebesaran_ditolak() {
        let srv = server("head");
        let mut req = b"GET /x HTTP/1.1\r\nX-Pad: ".to_vec();
        req.resize(req.len() + 20 * 1024, b'a');
        req.extend(b"\r\n\r\n");
        assert!(ask(srv.addr, &req).starts_with("HTTP/1.1 431"));
        // permintaan normal tetap dilayani
        let tok = srv.url.rsplit('/').find(|s| !s.is_empty()).unwrap().to_string();
        assert!(ask(srv.addr, format!("GET /{tok}/ HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn header_non_utf8_tetap_dilayani() {
        let srv = server("utf8");
        assert!(ask(srv.addr, b"GET /x HTTP/1.1\r\nX-A: \xff\xfe\r\n\r\n").starts_with("HTTP/1.1 404"));
    }

    #[test]
    fn batas_waktu_total_bukan_per_baca() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut c = TcpStream::connect(l.local_addr().unwrap()).unwrap();
        let (s, _) = l.accept().unwrap();
        // menetes 1 byte tiap 50 ms tanpa baris baru; tiap read selesai jauh sebelum batas 300 ms
        std::thread::spawn(move || while c.write_all(b"a").is_ok() { std::thread::sleep(Duration::from_millis(50)) });
        let t = Instant::now();
        // take(100): kalau Deadline rusak, test gagal setelah ~5 detik, bukan menggantung
        let mut r = BufReader::new(Deadline(s, t + Duration::from_millis(300)).take(100));
        let e = r.read_line(&mut String::new()).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::TimedOut);
        assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    }

    #[test]
    fn koneksi_dibatasi() {
        let srv = server("conn");
        let idle: Vec<TcpStream> = (0..MAX_CONN).map(|_| TcpStream::connect(srv.addr).unwrap()).collect();
        let mut extra = TcpStream::connect(srv.addr).unwrap();
        extra.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let r = extra.read(&mut [0u8; 1]);
        let closed = matches!(r, Ok(0)) || matches!(&r, Err(e) if matches!(e.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted));
        assert!(closed, "koneksi ke-{} harus langsung ditutup: {r:?}", MAX_CONN + 1);
        // slot kembali setelah koneksi lama ditutup
        drop(idle);
        let t = Instant::now();
        while !ask(srv.addr, b"POST / HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 405") {
            assert!(t.elapsed() < Duration::from_secs(3), "slot tidak kembali");
        }
    }
}
