//! Jendela pemilih Wi-Fi dan Bluetooth ala macOS, dibuka dari ikon Wi-Fi menu bar (di bawah ikon) dan modul Control Center
//! (Control Center menutup, jendela ini muncul di pojok kanan atas dengan animasi Control Center). Sudutnya sebesar kartu
//! slider Suara/Layar di Control Center.
//! Wi-Fi: sakelar, jaringan dikenal (sinyal + gembok), "Jaringan Lain" yang bisa dibuka, Pengaturan Wi-Fi.
//! Bluetooth: sakelar, perangkat terpasang (lingkaran biru = tersambung), Pengaturan Bluetooth.
//! Data dibaca di thread pekerja (`wifi::spawn`, `bt::spawn`, `radio::spawn`); menu digambar ulang di tempat saat data
//! baru datang tanpa menutupnya.

use crate::dock::menu::{Card, Item, Menu, MENU_CLOSE, MENU_PICK};
use crate::prefs;
use nacre_core::netlist::{bars, DevKind, Device};
use nacre_core::symbols::Sym;
use nacre_platform::{bt, launch, radio, sys, wifi};
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::Tier;
use std::cell::RefCell;
use windows::core::w;
use windows::UI::Composition::Compositor;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_WIFI: u32 = WM_APP + 140;
const WM_BT: u32 = WM_APP + 141;
const WM_RADIO: u32 = WM_APP + 142;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Which {
    Wifi,
    Bluetooth,
}

/// Letak jendela (piksel layar).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Anchor {
    /// Di bawah ikon menu bar: tepi kiri `x`, tepi atas `top`.
    Below(i32, i32),
    /// Pojok kanan atas, menggantikan Control Center: tepi kanan `right`, tepi atas `top`.
    Corner(i32, i32),
}

/// Sudut sama dengan kartu slider Suara/Layar Control Center (0,34 × sel 82 dp), tepi dalam lebih lega.
const CARD: Card = Card { radius: 28.0, pad: 10.0, min_w: 300.0 };

const ID_TOGGLE: usize = 1;
const ID_OTHERS: usize = 2;
const ID_SETTINGS: usize = 3;
const ID_LOCATION: usize = 4;
const ID_FLYOUT: usize = 5;
const ID_CURRENT: usize = 6;
const ID_NET0: usize = 100;
const ID_DEV0: usize = 400;

struct Net {
    hwnd: HWND,
    comp: Compositor,
    gfx: Option<Gfx>,
    menu: Option<Menu>,
    dpi: f32,
    /// Jendela yang terbuka dan letaknya.
    open: Option<(Which, Anchor)>,
    /// "Jaringan Lain" sedang dibuka.
    others: bool,
    scanner: Option<wifi::Scanner>,
    scan: Option<wifi::Scan>,
    watcher: Option<bt::Watcher>,
    devices: Option<Vec<Device>>,
    radios: Option<radio::Radios>,
    radio: radio::Snapshot,
    /// Sakelar dibalik dari menu ini: kabari menu bar dan Control Center saat keadaan baru tiba.
    toggled: bool,
    /// Perangkat audio Bluetooth yang sedang disambung/diputus: (alamat, ingin tersambung, sejak).
    pending: Option<(u64, bool, std::time::Instant)>,
}

thread_local! {
    static NET: RefCell<Option<Net>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Net) -> R) -> Option<R> {
    NET.with(|c| c.borrow_mut().as_mut().map(f))
}

fn post(hwnd: isize, msg: u32) -> impl Fn() + Send + 'static {
    move || unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(0), LPARAM(0));
    }
}

fn ensure(comp: &Compositor) -> bool {
    if NET.with(|c| c.borrow().is_some()) {
        return true;
    }
    let hwnd = unsafe {
        let Ok(hi) = GetModuleHandleW(None) else { return false };
        let hi: HINSTANCE = hi.into();
        let class = w!("AbraNetMenu");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        match CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-netmenu"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None) {
            Ok(h) => h,
            Err(_) => return false,
        }
    };
    NET.with(|c| {
        *c.borrow_mut() = Some(Net {
            hwnd,
            comp: comp.clone(),
            gfx: None,
            menu: None,
            dpi: 1.0,
            open: None,
            others: false,
            scanner: None,
            scan: None,
            watcher: None,
            devices: None,
            radios: None,
            radio: radio::Snapshot::default(),
            toggled: false,
            pending: None,
        })
    });
    true
}

/// Buka jendela `which` di `anchor`. Membuka jendela yang sama lagi menutupnya.
pub fn show(comp: &Compositor, which: Which, anchor: Anchor, dpi: f32) {
    if !ensure(comp) {
        return;
    }
    with(|n| {
        let visible = n.menu.as_ref().map(|m| m.is_open()).unwrap_or(false);
        if visible && n.open.map(|o| o.0) == Some(which) {
            n.close();
            return;
        }
        if n.dpi != dpi {
            n.menu = None;
        }
        n.dpi = dpi;
        n.open = Some((which, anchor));
        n.others = false;
        let h = n.hwnd.0 as isize;
        if n.radios.is_none() {
            n.radios = Some(radio::spawn(post(h, WM_RADIO)));
        } else if let Some(r) = &n.radios {
            let _ = r.commands.send(radio::Cmd::Refresh);
        }
        match which {
            Which::Wifi => {
                let s = n.scanner.get_or_insert_with(|| wifi::spawn(post(h, WM_WIFI)));
                let _ = s.commands.send(wifi::Cmd::Scan);
            }
            Which::Bluetooth => {
                let w = n.watcher.get_or_insert_with(|| bt::spawn(post(h, WM_BT)));
                let _ = w.commands.send(bt::Cmd::Refresh);
            }
        }
        n.render();
    });
}

/// Khusus uji (`NACRE_TEST_NETMENU=wifi-others`): buka/tutup "Jaringan Lain" seperti diklik.
pub fn test_toggle_others() {
    with(|n| n.pick(ID_OTHERS));
}

/// Khusus uji (`NACRE_TEST_NETMENU=bt-click`, hanya dengan data palsu): klik perangkat audio pertama.
pub fn test_click_audio() {
    if std::env::var_os("NACRE_FAKE_WIFI").is_none() {
        return;
    }
    with(|n| {
        let i = n.devices.as_ref().and_then(|d| d.iter().position(|x| x.audio));
        if let Some(i) = i {
            n.pick(ID_DEV0 + i);
        }
    });
}

fn signal_sym(signal: u8) -> Sym {
    match bars(signal) {
        1 => Sym::Wifi1,
        2 => Sym::Wifi2,
        _ => Sym::Wifi,
    }
}

fn device_sym(k: DevKind) -> Sym {
    match k {
        DevKind::Headphones => Sym::Headphones,
        DevKind::Keyboard => Sym::Keyboard,
        DevKind::Mouse => Sym::Mouse,
        DevKind::Other => Sym::Bluetooth,
    }
}

impl Net {
    /// Tutup dengan animasi tutup Control Center.
    fn close(&mut self) {
        if let Some(m) = self.menu.as_mut() {
            m.hide_animated(&self.comp);
        }
        self.open = None;
    }

    fn wifi_items(&self) -> Vec<Item> {
        let on = self.radio.wifi;
        let mut v = vec![Item::switch(ID_TOGGLE, "Wi-Fi", on == Some(true))];
        if on != Some(false) {
            match &self.scan {
                None => v.push(Item::heading(prefs::tr("Mencari jaringan…"))),
                Some(s) if !s.adapter => v.push(Item::heading(prefs::tr("Tidak ada adaptor Wi-Fi"))),
                Some(s) if s.denied => {
                    // Windows 11 24H2+: tanpa izin Lokasi hanya nama jaringan tersambung yang boleh dibaca
                    if s.current.is_some() || s.connected {
                        let label = s.current.clone().unwrap_or_else(|| prefs::tr("Tersambung").to_string());
                        v.push(Item::heading(prefs::tr("Jaringan Dikenal")));
                        v.push(Item::action(ID_CURRENT, &label, true).with_lead(Sym::Wifi, true));
                    }
                    v.push(Item::heading(prefs::tr("Lokasi mati: jaringan sekitar tersembunyi")));
                    v.push(Item::action(ID_LOCATION, prefs::tr("Nyalakan Lokasi…"), true));
                    v.push(Item::action(ID_FLYOUT, prefs::tr("Jaringan Lain…"), true));
                }
                Some(s) => {
                    let row = |i: usize, n: &nacre_core::netlist::Network| {
                        Item::action(ID_NET0 + i, &n.ssid, true).with_lead(signal_sym(n.signal), n.connected).with_trail(if n.secured { vec![Sym::Lock] } else { vec![] })
                    };
                    let known: Vec<usize> = (0..s.networks.len()).filter(|i| s.networks[*i].known() || s.networks[*i].connected).collect();
                    if !known.is_empty() {
                        v.push(Item::heading(prefs::tr("Jaringan Dikenal")));
                        v.extend(known.iter().map(|i| row(*i, &s.networks[*i])));
                    }
                    v.push(Item::action(ID_OTHERS, prefs::tr("Jaringan Lain"), true).with_trail(vec![if self.others { Sym::ChevronDown } else { Sym::ChevronRight }]).keep_open());
                    if self.others {
                        let others: Vec<usize> = (0..s.networks.len()).filter(|i| !known.contains(i)).take(12).collect();
                        if others.is_empty() {
                            v.push(Item::heading(prefs::tr("Tidak ada jaringan lain")));
                        }
                        v.extend(others.iter().map(|i| row(*i, &s.networks[*i])));
                    }
                }
            }
        }
        v.push(Item::separator());
        v.push(Item::action(ID_SETTINGS, prefs::tr("Pengaturan Wi-Fi…"), true));
        v
    }

    fn bt_items(&self) -> Vec<Item> {
        let on = self.radio.bluetooth;
        let mut v = vec![Item::switch(ID_TOGGLE, "Bluetooth", on == Some(true))];
        if on != Some(false) {
            v.push(Item::heading(prefs::tr("Perangkat")));
            match &self.devices {
                None => v.push(Item::heading(prefs::tr("Mencari perangkat…"))),
                Some(d) if d.is_empty() => v.push(Item::heading(prefs::tr("Tidak ada perangkat"))),
                Some(d) => v.extend(d.iter().take(12).enumerate().map(|(i, dev)| {
                    let it = Item::action(ID_DEV0 + i, &dev.name, true).with_lead(device_sym(dev.kind), dev.connected);
                    match self.pending {
                        // sedang disambung/diputus: label redup di kanan sampai keadaannya berubah
                        Some((a, want, _)) if a == dev.addr && dev.audio => it.with_accel(prefs::tr(if want { "Menyambung…" } else { "Memutus…" })).keep_open(),
                        // headphone/speaker: klik = sambung/putus langsung, menu tetap terbuka
                        _ if dev.audio => it.keep_open(),
                        _ => it,
                    }
                })),
            }
        }
        v.push(Item::separator());
        v.push(Item::action(ID_SETTINGS, prefs::tr("Pengaturan Bluetooth…"), true));
        v
    }

    fn render(&mut self) {
        let Some((which, anchor)) = self.open else { return };
        let items = match which {
            Which::Wifi => self.wifi_items(),
            Which::Bluetooth => self.bt_items(),
        };
        if self.gfx.is_none() {
            self.gfx = Gfx::new(&self.comp).ok();
        }
        if self.menu.is_none() {
            let c = sys::conditions();
            let tier = if !c.transparency || c.battery_saver || c.remote_session { Tier::Solid } else { Tier::Full };
            self.menu = Menu::new(&self.comp, self.hwnd, self.dpi, sys::screen_size(), tier).ok().map(|mut m| {
                m.set_card(Some(CARD));
                m
            });
        }
        let (Some(menu), Some(gfx)) = (self.menu.as_mut(), self.gfx.as_ref()) else { return };
        let (x, top, right) = match anchor {
            Anchor::Below(x, top) => (x, top, false),
            Anchor::Corner(r, top) => (r, top, true),
        };
        if let Err(e) = menu.show_card(&self.comp, gfx, items, x, top, right) {
            nacre_platform::log::line("netmenu", &format!("menu gagal: {e}"));
            return;
        }
        // di atas Control Center yang sedang menutup
        menu.raise();
    }

    /// Item dipilih. Item `keep_open` (sakelar, Jaringan Lain, perangkat audio) menggambar ulang jendela; selainnya
    /// jendela ditutup dengan animasi lalu aksinya dijalankan.
    fn pick(&mut self, id: usize) {
        let Some((which, _)) = self.open else { return };
        match id {
            ID_TOGGLE => {
                let (kind, cur) = match which {
                    Which::Wifi => (radio::Kind::Wifi, &mut self.radio.wifi),
                    Which::Bluetooth => (radio::Kind::Bluetooth, &mut self.radio.bluetooth),
                };
                let want = *cur != Some(true);
                // tampilkan langsung (sakelar terasa responsif); keadaan sebenarnya menyusul lewat WM_RADIO
                *cur = Some(want);
                if let Some(r) = &self.radios {
                    let _ = r.commands.send(radio::Cmd::Set(kind, want));
                }
                self.toggled = true;
                self.render();
                return;
            }
            ID_OTHERS => {
                self.others = !self.others;
                self.render();
                return;
            }
            i if i >= ID_DEV0 => {
                let dev = self.devices.as_ref().and_then(|d| d.get(i - ID_DEV0)).cloned();
                if let Some(dev) = dev.filter(|d| d.audio && d.addr != 0) {
                    if self.pending.is_none() {
                        let want = !dev.connected;
                        if let Some(w) = &self.watcher {
                            let _ = w.commands.send(bt::Cmd::Audio(dev.addr, want));
                        }
                        self.pending = Some((dev.addr, want, std::time::Instant::now()));
                        self.render();
                    }
                    return;
                }
            }
            _ => {}
        }
        self.close();
        match id {
            ID_SETTINGS => launch::open(if which == Which::Wifi { "ms-settings:network-wifi" } else { "ms-settings:bluetooth" }),
            ID_LOCATION => launch::open("ms-settings:privacy-location"),
            ID_FLYOUT => launch::open("ms-availablenetworks:"),
            i if (ID_NET0..ID_DEV0).contains(&i) => {
                let net = self.scan.as_ref().and_then(|s| s.networks.get(i - ID_NET0)).cloned();
                if let Some(n) = net.filter(|n| !n.connected) {
                    if n.known() || !n.secured {
                        if let Some(s) = &self.scanner {
                            let _ = s.commands.send(wifi::Cmd::Connect(n));
                        }
                    } else {
                        // jaringan baru berkata sandi: biarkan Windows menanyakan sandinya
                        launch::open("ms-availablenetworks:");
                    }
                }
            }
            i if i >= ID_DEV0 => launch::open("ms-settings:bluetooth"),
            _ => {}
        }
    }
}

unsafe extern "system" fn proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        MENU_PICK => {
            let id = wp.0;
            with(|n| n.pick(id));
            LRESULT(0)
        }
        MENU_CLOSE => {
            // klik di luar jendela
            with(|n| n.close());
            LRESULT(0)
        }
        WM_WIFI => {
            with(|n| {
                if let Some(s) = &n.scanner {
                    while let Ok(r) = s.results.try_recv() {
                        n.scan = Some(r);
                    }
                }
                if n.open.map(|o| o.0) == Some(Which::Wifi) {
                    n.render();
                }
            });
            LRESULT(0)
        }
        WM_BT => {
            with(|n| {
                if let Some(w) = &n.watcher {
                    while let Ok(d) = w.lists.try_recv() {
                        n.devices = Some(d);
                    }
                }
                // sambung/putus selesai (keadaan sudah sesuai) atau menyerah setelah 12 detik
                if let Some((addr, want, since)) = n.pending {
                    let done = n.devices.as_ref().is_some_and(|d| d.iter().any(|x| x.addr == addr && x.connected == want));
                    if done || since.elapsed() > std::time::Duration::from_secs(12) {
                        n.pending = None;
                    }
                }
                if n.open.map(|o| o.0) == Some(Which::Bluetooth) {
                    n.render();
                }
            });
            LRESULT(0)
        }
        WM_RADIO => {
            let notify = with(|n| {
                let before = n.radio;
                if let Some(r) = &n.radios {
                    while let Ok(s) = r.snapshots.try_recv() {
                        n.radio = s;
                    }
                }
                // Wi-Fi baru dinyalakan: pindai ulang supaya daftar jaringan muncul
                if before.wifi != Some(true) && n.radio.wifi == Some(true) {
                    if let Some(s) = &n.scanner {
                        let _ = s.commands.send(wifi::Cmd::Scan);
                    }
                }
                if n.radio.bluetooth == Some(true) && before.bluetooth != Some(true) {
                    if let Some(w) = &n.watcher {
                        let _ = w.commands.send(bt::Cmd::Refresh);
                    }
                }
                if n.open.is_some() {
                    n.render();
                }
                std::mem::take(&mut n.toggled)
            })
            .unwrap_or(false);
            if notify {
                crate::menubar::refresh_radios();
                crate::controlcenter::refresh_radios();
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}
