//! Server HTTP lokal sementara untuk "Kirim ke HP": HP memindai QR lalu mengunduh berkas lewat browser.
//! - Hanya mendengarkan di alamat IPv4 privat milik laptop (Wi-Fi/LAN), port acak, path rahasia `/<token>/`.
//! - Hanya melayani berkas yang ditaruh pengguna; selain itu 404. Tanpa internet, tanpa server luar.
//! - Berhenti saat `Server` di-drop (Selesai, batas waktu, atau ABRA ditutup).

use nacre_core::notch::send::{ascii_name, is_private, page, pct, token, url};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

fn respond(s: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) {
    let head = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n", body.len());
    let _ = s.write_all(head.as_bytes());
    let _ = s.write_all(body);
}

fn serve(mut s: TcpStream, tok: &str, sh: &Shared, en: bool) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(30)));
    let Ok(clone) = s.try_clone() else { return };
    let mut r = BufReader::new(clone);
    let mut line = String::new();
    if r.read_line(&mut line).is_err() {
        return;
    }
    // lewati header
    loop {
        let mut h = String::new();
        match r.read_line(&mut h) {
            Ok(0) | Err(_) => break,
            Ok(_) if h == "\r\n" || h == "\n" => break,
            _ => {}
        }
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
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            if stop2.load(Ordering::Relaxed) {
                break;
            }
            let Ok(c) = conn else { continue };
            // hanya perangkat di jaringan lokal
            let ok = matches!(c.peer_addr(), Ok(SocketAddr::V4(a)) if is_private(a.ip().octets()) || a.ip().is_loopback());
            if !ok {
                continue;
            }
            let (sh, tok) = (sh.clone(), tok.clone());
            std::thread::spawn(move || serve(c, &tok, &sh, en));
        }
    });
    crate::log::line("kirim", &format!("sesi dimulai: {n} berkas, port {}", addr.port()));
    Ok(Server { url: link, progress: rx, stop, addr })
}
