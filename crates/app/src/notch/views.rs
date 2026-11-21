use super::draw::{white, Anchor, Painter, WHITE};
use crate::prefs::tr;
use nacre_core::island::timer::{format_mmss, Phase, Timer};
use nacre_core::notch::glance::Item;
use nacre_core::notch::state::{Activity, Look, Tab, View};
use nacre_core::symbols::Sym;
use windows::UI::Color;
use windows::UI::Composition::{CompositionDrawingSurface, ContainerVisual, SpriteVisual};
use windows_numerics::Vector3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    PlayPause,
    Next,
    Prev,
    Seek(f32, f32),
    Tab(Tab),
    ShelfItem(usize),
    ShelfClear,
    Zone(usize),
    Settings,
    TimerToggle,
    TimerReset,
    TimerPlus,
    TimerMinus,
    CalDay(i32),
    FocusCat(u8),
    FocusSide,
    QuickOp(u8),
    QuickClose,
    QrClose,
    MeetingJoin,
    SendDone,
    TodoToggle(u32),
    TodoAdd,
    TodoClear,
    TodoField,
    NextTab,
}

pub struct Region {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub action: Action,
    pub vis: Option<ContainerVisual>,
}

fn kids(c: &ContainerVisual) -> i32 {
    c.Children().and_then(|k| k.Count()).unwrap_or(0)
}

fn group_since(c: &ContainerVisual, n0: i32) -> Option<ContainerVisual> {
    let col = c.Children().ok()?;
    let mut all = Vec::new();
    let it = col.First().ok()?;
    while it.HasCurrent().unwrap_or(false) {
        all.push(it.Current().ok()?);
        let _ = it.MoveNext();
    }
    let added = all.get(n0.max(0) as usize..)?;
    if added.is_empty() {
        return None;
    }
    let g = c.Compositor().ok()?.CreateContainerVisual().ok()?;
    let gk = g.Children().ok()?;
    for v in added {
        col.Remove(v).ok()?;
        gk.InsertAtTop(v).ok()?;
    }
    col.InsertAtTop(&g).ok()?;
    Some(g)
}

impl Region {
    pub fn hit(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(Clone, Copy)]
pub struct Frame {
    pub x0: f32,
    pub w: f32,
    pub h: f32,
}

pub struct Data<'a> {
    pub battery: Option<(u8, bool)>,
    pub timer: &'a Timer,
    pub now_ms: i64,
    pub glance: Vec<(Item, String)>,
    pub media: Option<MediaView<'a>>,
    pub lyrics: LyricsView<'a>,
    pub shelf: Vec<(&'a str, Option<&'a CompositionDrawingSurface>)>,
    pub drop_zone: Option<usize>,
    pub drop_count: usize,
    pub privacy: (bool, bool),
    pub download: Option<DownloadView<'a>>,
    pub cal: CalView<'a>,
    pub send: Option<SendView<'a>>,
    pub send_zone: bool,
    pub pill: Option<PillView>,
    pub todo: Vec<TodoRow>,
    pub todo_done: usize,
    pub todo_total: usize,
    pub todo_more: usize,
    pub todo_edit: Option<(String, bool)>,
    pub focus: FocusView,
    pub quick: Option<QuickView>,
    pub qr: Option<QrView<'a>>,
    pub meeting: Option<MeetingView>,
}

pub struct QuickView {
    pub count: usize,
    pub images: usize,
    pub busy: Option<u8>,
    pub status: String,
}

pub struct QrView<'a> {
    pub surface: Option<&'a CompositionDrawingSurface>,
    pub px: f32,
    pub text: String,
}

pub struct MeetingView {
    pub title: String,
    pub when: nacre_core::quick::When,
    pub link: bool,
    pub mic: bool,
    pub cam: bool,
}

pub struct FocusView {
    pub cat: usize,
    pub names: [String; nacre_core::focus::CATS],
    pub stage: nacre_core::focus::Stage,
    pub round: u8,
    pub today: nacre_core::focus::Today,
    pub week: [u32; 7],
    pub show_cal: bool,
}

impl FocusView {
    pub fn rgb(&self) -> [u8; 3] {
        nacre_core::focus::COLORS[self.cat.min(nacre_core::focus::CATS - 1)]
    }

    pub fn label(&self) -> String {
        if self.stage == nacre_core::focus::Stage::Break {
            tr("Istirahat").to_string()
        } else {
            self.names[self.cat.min(nacre_core::focus::CATS - 1)].clone()
        }
    }
}

pub struct PillView {
    pub kind: nacre_core::notch::pills::Kind,
    pub title: String,
    pub sub: String,
    pub swatch: Option<[u8; 3]>,
}

pub struct TodoRow {
    pub id: u32,
    pub text: String,
    pub done: bool,
    pub due: String,
    pub late: bool,
}

pub struct SendView<'a> {
    pub qr: Option<&'a CompositionDrawingSurface>,
    pub qr_px: f32,
    pub title: String,
    pub status: String,
    pub frac: f32,
    pub done: bool,
    pub error: bool,
}

pub type Date = (i32, u32, u32);

pub struct CalView<'a> {
    pub today: Date,
    pub sel: Date,
    pub events: Events<'a>,
}

pub enum Events<'a> {
    Loading,
    Unavailable,
    List(&'a [nacre_platform::agenda::Event]),
}

pub struct DownloadView<'a> {
    pub icon: Option<&'a CompositionDrawingSurface>,
    pub label: String,
    pub frac: Option<f32>,
    pub done: bool,
    pub up: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LyrState {
    Idle,
    Loading,
    Ready,
    Missing,
}

pub struct LyricsView<'a> {
    pub lines: Option<&'a nacre_core::notch::lyrics::Lyrics>,
    pub state: LyrState,
    pub pos_ms: i64,
    pub lrclib: bool,
}

pub struct MediaView<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub playing: bool,
    pub pos_ms: i64,
    pub dur_ms: i64,
    pub can_seek: bool,
    pub art: Option<&'a CompositionDrawingSurface>,
    pub tint: Option<[u8; 3]>,
    pub drag: Option<f32>,
}

#[derive(Default)]
pub struct Live {
    pub bars: Vec<SpriteVisual>,
    pub vis: nacre_core::theme::VisStyle,
    pub lyric: Option<SpriteVisual>,
    pub lyric_scroll: Option<windows::UI::Composition::ContainerVisual>,
    pub lyric_shift: f32,
}

const ORANGE: [u8; 3] = [255, 159, 10];
const GREEN: [u8; 3] = [48, 209, 88];
const INK: [u8; 3] = [22, 22, 24];
const SURFACE: Color = Color { A: 255, R: 27, G: 27, B: 31 };

fn surface(p: &Painter, c: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, r: f32) {
    p.rect(c, x, y, w, h, r, SURFACE);
}

fn next_button(p: &mut Painter, c: &ContainerVisual, f: Frame, accent: [u8; 3], regions: &mut Vec<Region>) {
    let d = p.px(26.0);
    let cx = f.x0 + f.w - p.px(19.0);
    let cy = p.px(48.0) + (f.h - p.px(48.0) - p.px(22.0)) / 2.0;
    let n0 = kids(c);
    p.rect(c, cx - d / 2.0, cy - d / 2.0, d, d, d / 2.0, SURFACE);
    p.rect(c, cx - d / 2.0, cy - d / 2.0, d, d, d / 2.0, rgb(accent, 46));
    p.sym(c, Sym::ChevronRight, cx + p.px(0.5), cy, 11.0, WHITE, 0.92);
    regions.push(Region { x: cx - d / 2.0 - p.px(4.0), y: cy - d / 2.0 - p.px(6.0), w: d + p.px(8.0), h: d + p.px(12.0), action: Action::NextTab, vis: group_since(c, n0) });
}

fn tab_accent(t: Tab, d: &Data) -> [u8; 3] {
    match t {
        Tab::Play => d.media.as_ref().and_then(|m| m.tint).unwrap_or_else(crate::prefs::accent),
        Tab::Shelf => ORANGE,
        Tab::Timer => d.focus.rgb(),
        Tab::Todo => GREEN,
    }
}

fn localized(id: &'static str, en: &'static str, es: &'static str) -> &'static str {
    match crate::prefs::get().lang {
        nacre_core::i18n::Lang::Id => id,
        nacre_core::i18n::Lang::En => en,
        nacre_core::i18n::Lang::Es => es,
    }
}

fn tab_title(t: Tab) -> (&'static str, &'static str) {
    match (t, crate::prefs::get().lang) {
        (Tab::Play, nacre_core::i18n::Lang::Id) => ("Sedang", "Diputar"),
        (Tab::Play, nacre_core::i18n::Lang::En) => ("Now", "Playing"),
        (Tab::Play, nacre_core::i18n::Lang::Es) => ("En", "reproducción"),
        (Tab::Shelf, nacre_core::i18n::Lang::Id) => ("Rak", "Berkas"),
        (Tab::Shelf, nacre_core::i18n::Lang::En) => ("File", "Shelf"),
        (Tab::Shelf, nacre_core::i18n::Lang::Es) => ("Archivos", "guardados"),
        (Tab::Timer, nacre_core::i18n::Lang::Id) => ("Sesi", "Fokus"),
        (Tab::Timer, nacre_core::i18n::Lang::En) => ("Focus", "Session"),
        (Tab::Timer, nacre_core::i18n::Lang::Es) => ("Sesión", "de enfoque"),
        (Tab::Todo, nacre_core::i18n::Lang::Id) => ("Daftar", "Tugas"),
        (Tab::Todo, nacre_core::i18n::Lang::En) => ("Task", "List"),
        (Tab::Todo, nacre_core::i18n::Lang::Es) => ("Lista", "de tareas"),
    }
}

fn rgb(c: [u8; 3], a: u8) -> Color {
    Color { A: a, R: c[0], G: c[1], B: c[2] }
}

fn clock_parts() -> (String, String) {
    let n = nacre_platform::time::local_now();
    let s = crate::prefs::get();
    let lang = s.lang;
    let wd = nacre_core::island::calendar::weekday(n.year, n.month, n.day) as usize;
    let days = nacre_core::i18n::weekday_labels(lang, false, nacre_core::island::calendar::weekday_labels(nacre_core::i18n::Lang::Id, false));
    let mon: String = nacre_core::i18n::month_name(lang, n.month, nacre_core::island::calendar::month_name(n.month))
        .chars().take(3).collect();
    let date = format!("{} {} {}", days[wd % 7], n.day, mon);
    let time = if s.clock_24h {
        let sep = if lang == nacre_core::i18n::Lang::Id { '.' } else { ':' };
        format!("{:02}{sep}{:02}", n.hour, n.minute)
    } else {
        format!("{}:{:02} {}", if n.hour % 12 == 0 { 12 } else { n.hour % 12 }, n.minute, nacre_core::i18n::day_period(lang, n.hour))
    };
    (date, time)
}

fn page_dots(p: &mut Painter, c: &ContainerVisual, f: Frame, cur: Tab, accent: [u8; 3], regions: &mut Vec<Region>) {
    let cy = f.h - p.px(9.0);
    let (dot, long, gap) = (p.px(5.0), p.px(16.0), p.px(5.0));
    let total: f32 = Tab::ALL.iter().map(|t| if *t == cur { long } else { dot }).sum::<f32>() + gap * (Tab::ALL.len() as f32 - 1.0);
    let mut x = f.x0 + f.w / 2.0 - total / 2.0;
    for t in Tab::ALL {
        let w = if t == cur { long } else { dot };
        let n0 = kids(c);
        p.rect(c, x, cy - dot / 2.0, w, dot, dot / 2.0, if t == cur { rgb(accent, 255) } else { white(70) });
        regions.push(Region { x: x - gap / 2.0, y: cy - p.px(8.0), w: w + gap, h: p.px(16.0), action: Action::Tab(t), vis: group_since(c, n0) });
        x += w + gap;
    }
}

fn card(p: &Painter, c: &ContainerVisual, x: f32, y: f32, w: f32, h: f32) {
    surface(p, c, x, y, w, h, p.px(18.0));
}

pub fn draw(p: &mut Painter, c: &ContainerVisual, look: Look, f: Frame, d: &Data, regions: &mut Vec<Region>, live: &mut Live) {
    match look.view {
        View::Rest => {}
        View::Compact(Activity::Timer) | View::Compact(Activity::TimerEnding) => {
            let urgent = look.view == View::Compact(Activity::TimerEnding);
            compact_timer(p, c, f, d, urgent)
        }
        View::Compact(Activity::Charging) => compact_charging(p, c, f, d),
        View::Compact(Activity::Glance | Activity::GlancePair) => compact_glance(p, c, f, d),
        View::Compact(Activity::Music) => compact_music(p, c, f, d, live, f.h),
        View::Compact(Activity::Lyrics) => compact_lyrics(p, c, f, d, live),
        View::Compact(Activity::Shelf) => compact_shelf(p, c, f, d),
        View::Compact(Activity::Download) => compact_download(p, c, f, d),
        View::Compact(Activity::Player) => compact_player(p, c, f, d, regions, live),
        View::Compact(Activity::Pill) => compact_pill(p, c, f, d),
        View::Compact(Activity::Meeting) => compact_meeting(p, c, f, d, regions),
        View::Open(tab) => {
            header(p, c, f, tab, d, regions);
            let inner = Frame { w: f.w - p.px(30.0), ..f };
            match tab {
                Tab::Play => play(p, c, inner, d, regions, live),
                Tab::Shelf if d.qr.is_some() => qr_card(p, c, inner, d, regions),
                Tab::Shelf if d.quick.is_some() => quick_card(p, c, inner, d, regions),
                Tab::Shelf if d.send.is_some() => send_card(p, c, inner, d, regions),
                Tab::Shelf if d.shelf.is_empty() => shelf_empty(p, c, inner),
                Tab::Shelf => shelf_tab(p, c, inner, d, regions),
                Tab::Timer => timer(p, c, inner, d, regions),
                Tab::Todo => todo_tab(p, c, inner, d, regions),
            }
            next_button(p, c, f, tab_accent(tab, d), regions);
        }
        View::Drop => drop_zones(p, c, f, d, regions),
    }
    if matches!(look.view, View::Rest | View::Compact(_)) {
        privacy_dots(p, c, f, d.privacy);
    }
}

fn privacy_dots(p: &mut Painter, c: &ContainerVisual, f: Frame, (mic, cam): (bool, bool)) {
    let (r, cy) = (p.px(3.0), f.h / 2.0);
    let mut x = f.x0 + f.w - p.px(7.0);
    for (on, rgb) in [(mic, ORANGE), (cam, GREEN)] {
        if on {
            p.rect(c, x - r, cy - r, 2.0 * r, 2.0 * r, r, Color { A: 255, R: rgb[0], G: rgb[1], B: rgb[2] });
            x -= p.px(9.0);
        }
    }
}

pub(super) fn count_text(n: usize) -> String {
    match crate::prefs::get().lang {
        nacre_core::i18n::Lang::Id => format!("{n} berkas"),
        nacre_core::i18n::Lang::En => format!("{n} {}", if n == 1 { "file" } else { "files" }),
        nacre_core::i18n::Lang::Es => format!("{n} {}", if n == 1 { "archivo" } else { "archivos" }),
    }
}

fn compact_shelf(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data) {
    let cy = f.h / 2.0;
    p.sym(c, Sym::Tray, f.x0 + p.px(22.0), cy, 15.0, WHITE, 0.9);
    let th = p.px(12.5) * 1.3;
    p.text(c, &count_text(d.shelf.len()), f.x0 + f.w - p.px(16.0), cy - th / 2.0, 12.5, 600, WHITE, 0.95, Anchor::Right, f.w / 2.0);
}

fn drop_zones(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>) {
    let (pad, gap) = (p.px(14.0), p.px(10.0));
    let all = [(0usize, Sym::Tray, "Simpan ke Rak"), (1, Sym::ArrowUp, "Kirim"), (2, Sym::Folder, "Salin ke\u{2026}"), (3, Sym::Grid, "Buka dengan\u{2026}"), (4, Sym::Control, "Aksi cepat")];
    let zones: Vec<_> = all.into_iter().filter(|z| z.0 != 1 || d.send_zone).collect();
    let n = zones.len() as f32;
    let zw = (f.w - 2.0 * pad - (n - 1.0) * gap) / n;
    let (top, zh) = (pad, f.h - 2.0 * pad);
    let accent = crate::prefs::accent();
    for (k, (i, s, label)) in zones.iter().enumerate() {
        let i = *i;
        let x = f.x0 + pad + k as f32 * (zw + gap);
        let on = d.drop_zone == Some(i);
        let fill = if on { Color { A: 230, R: accent[0], G: accent[1], B: accent[2] } } else { white(22) };
        p.rect(c, x, top, zw, zh, p.px(18.0), fill);
        p.sym(c, *s, x + zw / 2.0, top + zh / 2.0 - p.px(12.0), 22.0, WHITE, if on { 1.0 } else { 0.85 });
        let th = p.px(12.5) * 1.3;
        p.text(c, tr(label), x + zw / 2.0, top + zh / 2.0 + p.px(10.0) - th / 2.0 + p.px(6.0), 12.5, 600, WHITE, if on { 1.0 } else { 0.85 }, Anchor::Center, zw - p.px(12.0));
        regions.push(Region { x, y: top, w: zw, h: zh, action: Action::Zone(i), vis: None });
    }
    if d.drop_count > 0 {
        let th = p.px(10.5) * 1.3;
        p.text(c, &count_text(d.drop_count), f.x0 + f.w / 2.0, f.h - pad / 2.0 - th / 2.0 - p.px(1.0), 10.5, 500, WHITE, 0.55, Anchor::Center, f.w);
    }
}

fn pill_button(p: &mut Painter, c: &ContainerVisual, label: &str, right: f32, top: f32, fill: Color, action: Action, regions: &mut Vec<Region>) -> f32 {
    let (bw, bh) = (p.measure(label, 12.0, 600) + p.px(26.0), p.px(26.0));
    let bx = right - bw;
    p.rect(c, bx, top, bw, bh, bh / 2.0, fill);
    let th = p.px(12.0) * 1.3;
    p.text(c, label, bx + bw / 2.0, top + bh / 2.0 - th / 2.0, 12.0, 600, WHITE, 0.95, Anchor::Center, bw);
    regions.push(Region { x: bx, y: top, w: bw, h: bh, action, vis: None });
    bx
}

fn quick_card(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>) {
    use nacre_core::quick::Op;
    let Some(q) = &d.quick else { return };
    let pad = p.px(14.0);
    let (top, h) = (p.px(48.0), f.h - p.px(48.0) - pad);
    let (x0, w) = (f.x0 + pad, f.w - 2.0 * pad);
    card(p, c, x0, top, w, h);
    let cyan = [100u8, 210, 255];
    let title = format!("{} \u{00B7} {}", tr("Aksi cepat"), count_text(q.count));
    p.text(c, &title, x0 + p.px(16.0), top + p.px(12.0), 14.0, 700, WHITE, 1.0, Anchor::Left, w * 0.6);
    pill_button(p, c, tr("Tutup"), x0 + w - p.px(12.0), top + p.px(9.0), white(40), Action::QuickClose, regions);
    let (gx, gy) = (x0 + p.px(12.0), top + p.px(44.0));
    let gap = p.px(8.0);
    let bw = (w - p.px(24.0) - gap) / 2.0;
    let bh = ((h - p.px(44.0) - p.px(30.0) - gap) / 2.0).max(p.px(26.0));
    for (i, op) in Op::ALL.iter().enumerate() {
        let (r, col) = (i / 2, i % 2);
        let (bx, by) = (gx + col as f32 * (bw + gap), gy + r as f32 * (bh + gap));
        let enabled = q.busy.is_none() && (!op.needs_image() || q.images > 0);
        let active = q.busy == Some(i as u8);
        let n0 = kids(c);
        let fill = if active { Color { A: 230, R: cyan[0], G: cyan[1], B: cyan[2] } } else { white(if enabled { 26 } else { 10 }) };
        p.rect(c, bx, by, bw, bh, p.px(12.0), fill);
        let th = p.px(12.5) * 1.3;
        p.text(c, &p.fit(tr(op.title()), 12.5, 600, bw - p.px(20.0)), bx + p.px(14.0), by + (bh - th) / 2.0, 12.5, 600, WHITE, if enabled || active { 1.0 } else { 0.35 }, Anchor::Left, bw - p.px(16.0));
        if enabled {
            regions.push(Region { x: bx, y: by, w: bw, h: bh, action: Action::QuickOp(i as u8), vis: group_since(c, n0) });
        }
    }
    let status = if q.status.is_empty() { tr("Hasil disimpan di folder yang sama; berkas asli tidak diubah.").to_string() } else { q.status.clone() };
    p.text(c, &p.fit(&status, 11.0, 500, w - p.px(28.0)), x0 + p.px(14.0), top + h - p.px(22.0), 11.0, 500, WHITE, 0.6, Anchor::Left, w - p.px(24.0));
}

fn qr_card(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>) {
    let Some(q) = &d.qr else { return };
    let pad = p.px(14.0);
    let (top, h) = (p.px(48.0), f.h - p.px(48.0) - pad);
    let (x0, w) = (f.x0 + pad, f.w - 2.0 * pad);
    card(p, c, x0, top, w, h);
    let s = h - p.px(16.0);
    let (qx, qy) = (x0 + p.px(8.0), top + p.px(8.0));
    p.rect(c, qx, qy, s, s, p.px(14.0), Color { A: 255, R: 255, G: 255, B: 255 });
    if let Some(surf) = q.surface {
        let inset = ((s - q.px) / 2.0).round();
        p.place(c, surf, qx + inset, qy + inset, q.px, q.px, 1.0);
    }
    let bx = pill_button(p, c, tr("Tutup"), x0 + w - p.px(12.0), top + p.px(10.0), white(40), Action::QrClose, regions);
    let tx = qx + s + p.px(18.0);
    let tw = x0 + w - p.px(16.0) - tx;
    p.text(c, &p.fit(tr("QR dari papan klip"), 14.0, 700, bx - p.px(10.0) - tx), tx, top + p.px(12.0), 14.0, 700, WHITE, 1.0, Anchor::Left, bx - tx);
    p.text(c, &p.fit(&q.text, 12.0, 500, tw), tx, top + p.px(38.0), 12.0, 500, WHITE, 0.75, Anchor::Left, tw + p.px(4.0));
    p.text(c, tr("Pindai dengan kamera HP."), tx, top + p.px(60.0), 11.5, 500, WHITE, 0.5, Anchor::Left, tw);
}

fn compact_meeting(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>) {
    use nacre_core::quick::When;
    let Some(m) = &d.meeting else { return };
    let cy = f.h / 2.0;
    let live = matches!(m.when, When::Live(_));
    let rgb = if live { GREEN } else { ORANGE };
    let dot = p.px(8.0);
    p.rect(c, f.x0 + p.px(16.0), cy - dot / 2.0, dot, dot, dot / 2.0, Color { A: 255, R: rgb[0], G: rgb[1], B: rgb[2] });
    let th = p.px(12.5) * 1.3;
    let mut right = f.x0 + f.w - p.px(12.0);
    if m.link {
        let label = tr("Gabung");
        let (bw, bh) = (p.measure(label, 11.5, 700) + p.px(18.0), p.px(20.0));
        let bx = right - bw;
        p.rect(c, bx, cy - bh / 2.0, bw, bh, bh / 2.0, Color { A: 255, R: GREEN[0], G: GREEN[1], B: GREEN[2] });
        let bt = p.px(11.5) * 1.3;
        p.text(c, label, bx + bw / 2.0, cy - bt / 2.0, 11.5, 700, [4, 52, 44], 1.0, Anchor::Center, bw);
        regions.push(Region { x: bx - p.px(4.0), y: 0.0, w: bw + p.px(8.0), h: f.h, action: Action::MeetingJoin, vis: None });
        right = bx - p.px(10.0);
    }
    if live {
        for (on, col) in [(m.cam, GREEN), (m.mic, ORANGE)] {
            if on {
                let r = p.px(3.5);
                p.rect(c, right - 2.0 * r, cy - r, 2.0 * r, 2.0 * r, r, Color { A: 255, R: col[0], G: col[1], B: col[2] });
                right -= 2.0 * r + p.px(6.0);
            }
        }
    }
    let when = match m.when {
        When::Soon(0) => tr("sekarang").to_string(),
        When::Soon(n) => format!("{n} {}", tr("mnt lagi")),
        When::Live(n) => format!("{n} {}", tr("mnt")),
    };
    p.text(c, &when, right, cy - th / 2.0, 12.0, 600, WHITE, 0.75, Anchor::Right, f.w * 0.3);
    let tw = p.measure(&when, 12.0, 600);
    let left = f.x0 + p.px(16.0) + dot + p.px(8.0);
    let max = (right - tw - p.px(10.0) - left).max(p.px(30.0));
    p.text(c, &p.fit(&m.title, 12.5, 600, max), left, cy - th / 2.0, 12.5, 600, WHITE, 0.95, Anchor::Left, max + p.px(4.0));
}

fn send_card(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>) {
    let Some(sv) = &d.send else { return };
    let pad = p.px(14.0);
    let (top, h) = (p.px(48.0), f.h - p.px(48.0) - pad);
    let (x0, w) = (f.x0 + pad, f.w - 2.0 * pad);
    card(p, c, x0, top, w, h);
    let s = h - p.px(16.0);
    let (qx, qy) = (x0 + p.px(8.0), top + p.px(8.0));
    if sv.error {
        p.rect(c, qx, qy, s, s, p.px(14.0), white(20));
        p.sym(c, Sym::Wifi, qx + s / 2.0, qy + s / 2.0, 30.0, WHITE, 0.45);
    } else {
        p.rect(c, qx, qy, s, s, p.px(14.0), Color { A: 255, R: 255, G: 255, B: 255 });
        if let Some(q) = sv.qr {
            let inset = ((s - sv.qr_px) / 2.0).round();
            p.place(c, q, qx + inset, qy + inset, sv.qr_px, sv.qr_px, 1.0);
        }
    }
    let label = tr(if sv.done || sv.error { "Tutup" } else { "Batal" });
    let (bw, bh) = (p.measure(label, 12.0, 600) + p.px(26.0), p.px(26.0));
    let (bx, by) = (x0 + w - p.px(12.0) - bw, top + p.px(10.0));
    let tx = qx + s + p.px(18.0);
    let tw = x0 + w - p.px(16.0) - tx;
    let head = bx - p.px(10.0) - tx;
    if sv.error {
        p.text(c, &p.fit(tr("Tidak tersambung ke jaringan lokal"), 14.0, 700, head), tx, top + p.px(16.0), 14.0, 700, WHITE, 1.0, Anchor::Left, head + p.px(4.0));
        p.text(c, tr("Sambungkan laptop ke Wi-Fi yang sama dengan HP."), tx, top + p.px(40.0), 12.0, 500, WHITE, 0.6, Anchor::Left, tw);
    } else {
        p.text(c, &p.fit(&sv.title, 14.0, 700, head), tx, top + p.px(12.0), 14.0, 700, WHITE, 1.0, Anchor::Left, head + p.px(4.0));
        p.text(c, &p.fit(tr("Pindai QR dengan kamera HP (Wi-Fi yang sama)"), 11.5, 500, tw), tx, top + p.px(33.0), 11.5, 500, WHITE, 0.6, Anchor::Left, tw + p.px(4.0));
        let green = Color { A: 255, R: GREEN[0], G: GREEN[1], B: GREEN[2] };
        p.text(c, &sv.status, tx, top + p.px(55.0), 12.0, 600, if sv.done { GREEN } else { WHITE }, 0.95, Anchor::Left, tw);
        let (by, bh) = (top + p.px(76.0), p.px(4.0));
        p.rect(c, tx, by, tw, bh, bh / 2.0, white(30));
        if sv.frac > 0.0 {
            p.rect(c, tx, by, (tw * sv.frac.clamp(0.0, 1.0)).max(bh), bh, bh / 2.0, green);
        }
        p.text(c, tr("Izinkan ABRA di Windows Firewall bila diminta."), tx, top + h - p.px(22.0), 10.0, 500, WHITE, 0.4, Anchor::Left, tw);
    }
    p.rect(c, bx, by, bw, bh, bh / 2.0, white(if sv.done || sv.error { 46 } else { 34 }));
    let th = p.px(12.0) * 1.3;
    p.text(c, label, bx + bw / 2.0, by + bh / 2.0 - th / 2.0, 12.0, 600, WHITE, 0.95, Anchor::Center, bw);
    regions.push(Region { x: bx, y: by, w: bw, h: bh, action: Action::SendDone, vis: None });
}

fn shelf_tab(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>) {
    let pad = p.px(14.0);
    let (top, h) = (p.px(48.0), f.h - p.px(48.0) - pad);
    let (x0, w) = (f.x0 + pad, f.w - 2.0 * pad);
    card(p, c, x0, top, w, h);
    p.text(c, &count_text(d.shelf.len()), x0 + p.px(14.0), top + p.px(9.0), 11.5, 600, WHITE, 0.5, Anchor::Left, w / 2.0);
    let clear = tr("Kosongkan");
    let cw = p.measure(clear, 11.5, 600) + p.px(4.0);
    let accent = crate::prefs::accent();
    p.text(c, clear, x0 + w - p.px(14.0), top + p.px(9.0), 11.5, 600, accent, 0.95, Anchor::Right, cw + 4.0);
    regions.push(Region { x: x0 + w - p.px(14.0) - cw - p.px(6.0), y: top + p.px(2.0), w: cw + p.px(12.0), h: p.px(24.0), action: Action::ShelfClear, vis: None });
    let (tw, th, gap) = (p.px(96.0), p.px(78.0), p.px(8.0));
    let ty = top + p.px(28.0);
    let max = ((w - p.px(20.0) + gap) / (tw + gap)).floor().max(1.0) as usize;
    let extra = d.shelf.len().saturating_sub(max);
    for (i, (name, icon)) in d.shelf.iter().take(max).enumerate() {
        let x = x0 + p.px(10.0) + i as f32 * (tw + gap);
        if extra > 0 && i == max - 1 {
            p.rect(c, x, ty, tw, th, p.px(14.0), white(14));
            let t = p.px(13.0) * 1.3;
            p.text(c, &format!("+{}", extra + 1), x + tw / 2.0, ty + th / 2.0 - t / 2.0, 13.0, 600, WHITE, 0.7, Anchor::Center, tw);
            break;
        }
        let n0 = kids(c);
        let isz = p.px(40.0);
        match icon {
            Some(s) => {
                p.image(c, s, x + (tw - isz) / 2.0, ty + p.px(6.0), isz, isz, p.px(6.0));
            }
            None => p.sym(c, Sym::Folder, x + tw / 2.0, ty + p.px(26.0), 26.0, WHITE, 0.7),
        }
        let shown = p.fit(name, 11.0, 500, tw - p.px(8.0));
        p.text(c, &shown, x + tw / 2.0, ty + p.px(52.0), 11.0, 500, WHITE, 0.9, Anchor::Center, tw - p.px(6.0));
        regions.push(Region { x, y: ty, w: tw, h: th, action: Action::ShelfItem(i), vis: group_since(c, n0) });
    }
}

fn compact_timer(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, urgent: bool) {
    let cy = f.h / 2.0;
    let rgb = if urgent { ORANGE } else { WHITE };
    let dot = p.px(8.0);
    let cat = d.focus.rgb();
    p.rect(c, f.x0 + p.px(16.0), cy - dot / 2.0, dot, dot, dot / 2.0, Color { A: 255, R: cat[0], G: cat[1], B: cat[2] });
    let th = p.px(13.0) * 1.3;
    let name = p.fit(&d.focus.label(), 12.5, 600, f.w * 0.5);
    p.text(c, &name, f.x0 + p.px(16.0) + dot + p.px(8.0), cy - th / 2.0, 12.5, 600, WHITE, 0.9, Anchor::Left, f.w * 0.55);
    let t = format_mmss(d.timer.remaining_secs(d.now_ms));
    p.text(c, &t, f.x0 + f.w - p.px(16.0), cy - th / 2.0, 13.0, 600, rgb, 1.0, Anchor::Right, f.w);
}

fn accent_or(t: Option<[u8; 3]>) -> [u8; 3] {
    t.unwrap_or_else(crate::prefs::accent)
}

fn current_line(d: &Data) -> String {
    match d.lyrics.lines {
        Some(l) => l.current(d.lyrics.pos_ms).unwrap_or("\u{266A}").to_string(),
        None => String::new(),
    }
}

fn compact_lyrics(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, live: &mut Live) {
    compact_music(p, c, f, d, live, f.h);
    let left = f.x0 + p.px(12.0 + 20.0 + 10.0);
    let right = f.x0 + f.w - p.px(14.0 + 27.0 + 10.0);
    let max = (right - left).max(p.px(20.0));
    let line = current_line(d);
    let shown = p.fit(&line, 13.0, 600, max);
    let th = p.px(13.0) * 1.3;
    let (_, v) = p.text_v(c, &shown, (left + right) / 2.0, f.h / 2.0 - th / 2.0, 13.0, 600, WHITE, 1.0, Anchor::Center, max + p.px(2.0));
    live.lyric = v;
}

fn compact_player(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>, live: &mut Live) {
    let Some(m) = &d.media else { return };
    let a = p.px(52.0);
    let (ax, ay) = (f.x0 + p.px(22.0), p.px(10.0));
    match m.art {
        Some(s) => {
            p.image(c, s, ax, ay, a, a, p.px(12.0));
        }
        None => {
            p.rect(c, ax, ay, a, a, p.px(12.0), white(36));
            p.sym(c, Sym::Note, ax + a / 2.0, ay + a / 2.0, 20.0, WHITE, 0.7);
        }
    }
    let (cx0, cy, step, hit) = (ax + a / 2.0, ay + a + p.px(17.0), p.px(24.0), p.px(24.0));
    let buttons = [(Action::Prev, Sym::Backward, 12.0), (Action::PlayPause, if m.playing { Sym::Pause } else { Sym::Play }, 14.0), (Action::Next, Sym::Forward, 12.0)];
    for (i, (act, sym, size)) in buttons.into_iter().enumerate() {
        let cx = cx0 + (i as f32 - 1.0) * step;
        let n0 = kids(c);
        p.sym(c, sym, cx, cy, size, WHITE, 0.95);
        regions.push(Region { x: cx - hit / 2.0, y: cy - hit / 2.0, w: hit, h: hit, action: act, vis: group_since(c, n0) });
    }
    let lx = ax + a + p.px(28.0);
    let lw = f.x0 + f.w - p.px(22.0) - lx;
    let synced = d.lyrics.lines.is_some_and(|l| l.synced);
    if synced {
        lyrics_flow(p, c, lx, p.px(6.0), lw, f.h - p.px(12.0), d, live, Flow { size: 16.0, pad: 0.0, anchor: 0.36, card: false });
    } else {
        let th = p.px(16.0) * 1.3;
        let cy = f.h / 2.0;
        p.text(c, &p.fit(m.title, 16.0, 700, lw), lx, cy - th, 16.0, 700, WHITE, 1.0, Anchor::Left, lw + p.px(4.0));
        p.text(c, &p.fit(m.artist, 13.0, 500, lw), lx, cy + p.px(2.0), 13.0, 500, WHITE, 0.6, Anchor::Left, lw + p.px(4.0));
    }
}

fn compact_music(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, live: &mut Live, row_h: f32) {
    let Some(m) = &d.media else { return };
    let cy = row_h / 2.0;
    let a = p.px(20.0);
    match m.art {
        Some(s) => {
            p.image(c, s, f.x0 + p.px(12.0), cy - a / 2.0, a, a, p.px(6.0));
        }
        None => {
            p.rect(c, f.x0 + p.px(12.0), cy - a / 2.0, a, a, p.px(6.0), white(40));
            p.sym(c, Sym::Note, f.x0 + p.px(12.0) + a / 2.0, cy, 12.0, WHITE, 0.8);
        }
    }
    let rgb = accent_or(m.tint);
    let prefs = crate::prefs::get();
    let style = prefs.visualizer;
    let square = prefs.preset == nacre_core::theme::Preset::Pixel;
    live.vis = style;
    let n = style.count();
    let (bw, gap, bh) = match style {
        nacre_core::theme::VisStyle::Bars => (p.px(3.0), p.px(3.0), p.px(14.0)),
        nacre_core::theme::VisStyle::Spectrum => (p.px(1.6), p.px(0.8), p.px(14.0)),
        nacre_core::theme::VisStyle::Wave => (p.px(1.0), p.px(0.5), p.px(15.0)),
        nacre_core::theme::VisStyle::Dots => (p.px(3.6), p.px(1.2), p.px(3.6)),
    };
    let right = f.x0 + f.w - p.px(14.0);
    let x0 = right - n as f32 * bw - (n as f32 - 1.0) * gap;
    let radius = if square { 0.0 } else { bw / 2.0 };
    for i in 0..n {
        if let Some(v) = p.rect(c, x0 + i as f32 * (bw + gap), cy - bh / 2.0, bw, bh, radius, Color { A: 240, R: rgb[0], G: rgb[1], B: rgb[2] }) {
            let _ = v.SetCenterPoint(Vector3 { X: bw / 2.0, Y: bh / 2.0, Z: 0.0 });
            let start = if style == nacre_core::theme::VisStyle::Dots { Vector3 { X: 0.7, Y: 0.7, Z: 1.0 } } else { Vector3 { X: 1.0, Y: 0.3, Z: 1.0 } };
            let _ = v.SetScale(start);
            live.bars.push(v);
        }
    }
}

fn mmss(ms: i64) -> String {
    let s = (ms.max(0) + 500) / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

#[allow(clippy::too_many_arguments)]
fn music_card(p: &mut Painter, c: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, m: &MediaView, regions: &mut Vec<Region>) {
    let tint = m.tint;
    let bg = match tint {
        Some(t) => Color { A: 48, R: t[0], G: t[1], B: t[2] },
        None => white(18),
    };
    surface(p, c, x, y, w, h, p.px(18.0));
    p.rect(c, x, y, w, h, p.px(18.0), bg);
    let pad = p.px(14.0);
    let art = p.px(64.0);
    let (ax, ay) = (x + pad, y + pad);
    let glow = accent_or(tint);
    p.rect(c, ax - p.px(4.0), ay - p.px(2.0), art + p.px(8.0), art + p.px(8.0), p.px(18.0), Color { A: 60, R: glow[0], G: glow[1], B: glow[2] });
    match m.art {
        Some(s) => {
            p.image(c, s, ax, ay, art, art, p.px(14.0));
        }
        None => {
            p.rect(c, ax, ay, art, art, p.px(14.0), white(40));
            p.sym(c, Sym::Note, ax + art / 2.0, ay + art / 2.0, 24.0, WHITE, 0.8);
        }
    }
    let tx = ax + art + p.px(12.0);
    let tw = x + w - pad - tx;
    let title = p.fit(m.title, 13.5, 600, tw);
    p.text(c, &title, tx, ay + p.px(10.0), 13.5, 600, WHITE, 1.0, Anchor::Left, tw + 2.0);
    let artist = p.fit(m.artist, 12.0, 400, tw);
    p.text(c, &artist, tx, ay + p.px(30.0), 12.0, 400, WHITE, 0.62, Anchor::Left, tw + 2.0);

    let (sx, sw) = (x + pad, w - 2.0 * pad);
    let sy = ay + art + p.px(14.0);
    let th = p.px(5.0);
    let frac = m.drag.unwrap_or(if m.dur_ms > 0 { m.pos_ms as f32 / m.dur_ms as f32 } else { 0.0 }).clamp(0.0, 1.0);
    p.rect(c, sx, sy - th / 2.0, sw, th, th / 2.0, white(52));
    p.rect(c, sx, sy - th / 2.0, (sw * frac).max(th), th, th / 2.0, white(236));
    let kx = sx + sw * frac;
    if m.drag.is_some() {
        let (kw, kh) = (p.px(28.0), p.px(18.0));
        p.rect(c, kx - kw / 2.0, sy - kh / 2.0, kw, kh, kh / 2.0, white(125));
        p.rect(c, kx - kw / 2.0 + p.px(1.5), sy - kh / 2.0 + p.px(1.5), kw - p.px(3.0), kh - p.px(3.0), (kh - p.px(3.0)) / 2.0, white(40));
    } else {
        let (kw, kh) = (p.px(18.0), p.px(12.0));
        p.rect(c, kx - kw / 2.0, sy - kh / 2.0, kw, kh, kh / 2.0, white(250));
    }
    if m.can_seek && m.dur_ms > 0 {
        regions.push(Region { x: sx - p.px(6.0), y: sy - p.px(12.0), w: sw + p.px(12.0), h: p.px(24.0), action: Action::Seek(sx, sw), vis: None });
    }
    let pos = if let Some(fr) = m.drag { (m.dur_ms as f32 * fr) as i64 } else { m.pos_ms };
    let ty = sy + p.px(8.0);
    p.text(c, &mmss(pos), sx, ty, 10.5, 500, WHITE, 0.55, Anchor::Left, sw / 2.0);
    p.text(c, &format!("-{}", mmss(m.dur_ms - pos)), sx + sw, ty, 10.5, 500, WHITE, 0.55, Anchor::Right, sw / 2.0);

    let cy = y + h - p.px(20.0);
    let cx = x + w / 2.0;
    let gap = p.px(48.0);
    let items = [(Sym::Backward, cx - gap, 17.0, Action::Prev), (if m.playing { Sym::Pause } else { Sym::Play }, cx, 22.0, Action::PlayPause), (Sym::Forward, cx + gap, 17.0, Action::Next)];
    for (s, bx, size, a) in items {
        let n0 = kids(c);
        p.sym(c, s, bx, cy, size, WHITE, 0.95);
        regions.push(Region { x: bx - p.px(20.0), y: cy - p.px(16.0), w: p.px(40.0), h: p.px(32.0), action: a, vis: group_since(c, n0) });
    }
}

fn glance_icon_w(p: &Painter, item: Item) -> f32 {
    match item {
        Item::Battery => (24.0 * p.px(1.0)).round(),
        Item::Clock | Item::Date | Item::Agenda | Item::Wifi => p.px(16.0),
        Item::Greeting | Item::System => 0.0,
    }
}

fn glance_icon(p: &mut Painter, c: &ContainerVisual, item: Item, x: f32, cy: f32, d: &Data) -> f32 {
    match item {
        Item::Battery => {
            let (pct, ch) = d.battery.unwrap_or((0, false));
            p.battery(c, x, cy, pct, ch)
        }
        Item::Clock | Item::Date | Item::Agenda | Item::Wifi => {
            let sym = match item {
                Item::Clock => Sym::Clock,
                Item::Wifi => Sym::Wifi,
                _ => Sym::Calendar,
            };
            p.sym(c, sym, x + p.px(8.0), cy, 15.0, WHITE, 0.9);
            p.px(16.0)
        }
        Item::Greeting | Item::System => 0.0,
    }
}

fn compact_glance(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data) {
    let cy = f.h / 2.0;
    let th = p.px(12.5) * 1.3;
    let ty = cy - th / 2.0;
    let mut left = f.x0 + p.px(14.0);
    let right = f.x0 + f.w - p.px(16.0);
    if crate::prefs::get().capsule_decor == nacre_core::notch::glance::Decor::Dot && !d.glance.is_empty() {
        let r = p.px(6.0);
        let a = crate::prefs::accent();
        p.rect(c, left, cy - r / 2.0, r, r, r / 2.0, Color { A: 255, R: a[0], G: a[1], B: a[2] });
        left += p.px(12.0);
    }
    match d.glance.as_slice() {
        [] => {}
        [(Item::Greeting, text)] => {
            let max = right - left;
            let shown = p.fit(text, 12.5, 600, max);
            p.text(c, &shown, (left + right) / 2.0, ty, 12.5, 600, WHITE, 0.95, Anchor::Center, max + p.px(2.0));
        }
        [(item, text)] => {
            let iw = glance_icon(p, c, *item, left, cy, d);
            let max = right - (left + iw + p.px(10.0));
            let shown = p.fit(text, 12.5, 600, max);
            p.text(c, &shown, right, ty, 12.5, 600, WHITE, 0.95, Anchor::Right, max + p.px(2.0));
        }
        [(a, ta), (b, tb), ..] => {
            let mid = f.x0 + f.w / 2.0;
            let iw = glance_icon(p, c, *a, left, cy, d);
            let lx = if iw > 0.0 { left + iw + p.px(7.0) } else { left };
            let lmax = (mid - p.px(5.0) - lx).max(0.0);
            let ls = p.fit(ta, 12.5, 600, lmax);
            p.text(c, &ls, lx, ty, 12.5, 600, WHITE, 0.95, Anchor::Left, lmax + p.px(2.0));
            let bw = glance_icon_w(p, *b);
            let rmax = (right - mid - p.px(5.0) - if bw > 0.0 { bw + p.px(7.0) } else { 0.0 }).max(0.0);
            let rs = p.fit(tb, 12.5, 600, rmax);
            let tw = p.measure(&rs, 12.5, 600);
            p.text(c, &rs, right, ty, 12.5, 600, WHITE, 0.95, Anchor::Right, rmax + p.px(2.0));
            if bw > 0.0 {
                glance_icon(p, c, *b, right - tw - p.px(7.0) - bw, cy, d);
            }
        }
    }
}

fn compact_download(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data) {
    let Some(dl) = &d.download else { return };
    let cy = f.h / 2.0;
    let a = p.px(22.0);
    let ix = f.x0 + p.px(14.0);
    match dl.icon {
        Some(s) => {
            p.image(c, s, ix, cy - a / 2.0, a, a, p.px(6.0));
        }
        None => {
            p.rect(c, ix, cy - a / 2.0, a, a, a / 2.0, white(36));
            p.sym(c, if dl.up { Sym::ArrowUp } else { Sym::ArrowDown }, ix + a / 2.0, cy, 11.0, WHITE, 0.9);
        }
    }
    let (r, w) = (11.0, 2.6);
    let gx = f.x0 + f.w - p.px(16.0) - p.px(r + w / 2.0);
    let gy = p.px(15.0);
    p.arc(c, gx, gy, r, w, 0.0, 1.0, WHITE, 56);
    match (dl.frac, dl.done) {
        (_, true) => {
            p.arc(c, gx, gy, r, w, 0.0, 1.0, GREEN, 255);
        }
        (Some(fr), false) => {
            if fr > 0.002 {
                p.arc(c, gx, gy, r, w, 0.0, fr.clamp(0.0, 1.0), GREEN, 255);
            }
        }
        (None, false) => {
            if let Some((v, n)) = p.arc(c, gx, gy, r, w, 0.0, 0.24, GREEN, 255) {
                spin(p, &v, n, d.now_ms);
            }
        }
    }
    p.sym(c, if dl.done { Sym::Check } else if dl.up { Sym::ArrowUp } else { Sym::ArrowDown }, gx, gy, 12.0, WHITE, 0.95);
    let th = p.px(9.5) * 1.3;
    p.text(c, &dl.label, gx, f.h - p.px(2.0) - th, 9.5, 600, WHITE, 0.95, Anchor::Center, p.px(40.0));
}

fn spin(p: &Painter, v: &SpriteVisual, n: f32, now_ms: i64) {
    use windows::Foundation::TimeSpan;
    use windows::UI::Composition::{AnimationIterationBehavior, CompositionEasingFunction};
    use windows_core::{Interface, HSTRING};
    const PERIOD: i64 = 1100;
    let go = || -> windows_core::Result<()> {
        let phase = (now_ms.rem_euclid(PERIOD)) as f32 / PERIOD as f32 * 360.0;
        v.SetCenterPoint(Vector3 { X: n / 2.0, Y: n / 2.0, Z: 0.0 })?;
        let lin: CompositionEasingFunction = p.comp.CreateLinearEasingFunction()?.cast()?;
        let a = p.comp.CreateScalarKeyFrameAnimation()?;
        a.InsertKeyFrame(0.0, phase)?;
        a.InsertKeyFrameWithEasingFunction(1.0, phase + 360.0, &lin)?;
        a.SetDuration(TimeSpan { Duration: PERIOD * 10_000 })?;
        a.SetIterationBehavior(AnimationIterationBehavior::Forever)?;
        v.StartAnimation(&HSTRING::from("RotationAngleInDegrees"), &a)
    };
    let _ = go();
}

fn compact_pill(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data) {
    use nacre_core::notch::pills::Kind;
    let Some(pv) = &d.pill else { return };
    let cy = f.h / 2.0;
    let accent = crate::prefs::accent();
    let (sym, rgb) = match pv.kind {
        Kind::Reminder => (Sym::Clock, ORANGE),
        Kind::Notice | Kind::Info => (Sym::Info, pv.swatch.unwrap_or(accent)),
        Kind::Update => (Sym::ArrowDown, GREEN),
        Kind::Device => {
            let t = pv.title.to_lowercase();
            let s = if pv.sub.contains(':') || t.contains("disk") || t.contains("usb") || t.contains("drive") {
                Sym::Folder
            } else if t.contains("mouse") {
                Sym::Mouse
            } else if t.contains("keyboard") || t.contains("keyb") {
                Sym::Keyboard
            } else if ["buds", "airpods", "headphone", "headset", "earphone", "wh-", "wf-", "jbl", "tws"].iter().any(|k| t.contains(k)) {
                Sym::Headphones
            } else {
                Sym::Bluetooth
            };
            (s, accent)
        }
    };
    let ix = f.x0 + p.px(22.0);
    let r = p.px(11.0);
    p.rect(c, ix - r, cy - r, 2.0 * r, 2.0 * r, r, Color { A: 255, R: rgb[0], G: rgb[1], B: rgb[2] });
    p.sym(c, sym, ix, cy, 12.0, WHITE, 1.0);
    let th = p.px(12.5) * 1.3;
    let sub_w = if pv.sub.is_empty() { 0.0 } else { (p.measure(&pv.sub, 11.5, 500) + p.px(4.0)).min(f.w * 0.42) };
    let tx = ix + r + p.px(10.0);
    let tw = (f.x0 + f.w - p.px(16.0) - sub_w - p.px(10.0) - tx).max(p.px(40.0));
    let title = p.fit(&pv.title, 12.5, 600, tw);
    p.text(c, &title, tx, cy - th / 2.0, 12.5, 600, WHITE, 0.98, Anchor::Left, tw + p.px(4.0));
    if !pv.sub.is_empty() {
        let sh = p.px(11.5) * 1.3;
        p.text(c, &p.fit(&pv.sub, 11.5, 500, sub_w), f.x0 + f.w - p.px(16.0), cy - sh / 2.0, 11.5, 500, WHITE, 0.62, Anchor::Right, sub_w + p.px(4.0));
    }
}

fn todo_tab(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>) {
    use nacre_core::notch::state::TODO_ROW;
    let lang = crate::prefs::get().lang;
    let pad = p.px(20.0);
    let top = p.px(50.0);
    let lw = p.px(150.0);
    let lx = f.x0 + pad;
    if d.todo_total > 0 {
        let hero = format!("{}", d.todo_done);
        let hw = p.measure(&hero, 38.0, 800);
        p.text(c, &hero, lx, top - p.px(4.0), 38.0, 800, WHITE, 1.0, Anchor::Left, hw + p.px(6.0));
        p.text(c, &format!("/{}", d.todo_total), lx + hw + p.px(2.0), top + p.px(7.0), 22.0, 700, GREEN, 0.95, Anchor::Left, p.px(80.0));
        let sub = localized("tugas selesai", "tasks done", "tareas completadas");
        p.text(c, sub, lx + p.px(2.0), top + p.px(42.0), 12.5, 500, WHITE, 0.55, Anchor::Left, lw);
    } else {
        p.sym(c, Sym::Check, lx + p.px(14.0), top + p.px(14.0), 20.0, GREEN, 0.9);
        let msg = localized("Belum ada tugas", "No tasks yet", "Aún no hay tareas");
        p.text(c, msg, lx, top + p.px(34.0), 13.0, 600, WHITE, 0.85, Anchor::Left, lw);
        let hint = localized("Atau tekan Ctrl+Alt+N", "Or press Ctrl+Alt+N", "O pulsa Ctrl+Alt+N");
        p.text(c, hint, lx, top + p.px(52.0), 11.5, 500, WHITE, 0.5, Anchor::Left, lw);
    }
    let mut ly = top + p.px(66.0);
    {
        let (bh, editing) = (p.px(30.0), d.todo_edit.is_some());
        let n0 = kids(c);
        surface(p, c, lx, ly, lw, bh, bh / 2.0);
        p.rect(c, lx, ly, lw, bh, bh / 2.0, rgb(GREEN, if editing { 110 } else { 52 }));
        let r = p.px(9.0);
        let ck = lx + p.px(6.0) + r;
        let cy = ly + bh / 2.0;
        p.rect(c, ck - r, cy - r, 2.0 * r, 2.0 * r, r, rgb(GREEN, 255));
        p.sym(c, Sym::Plus, ck, cy, 10.0, WHITE, 1.0);
        let label = localized("Tambah tugas", "Add a task", "Añadir una tarea");
        let th = p.px(13.0) * 1.3;
        p.text(c, label, ck + r + p.px(9.0), cy - th / 2.0, 13.0, 700, if editing { WHITE } else { GREEN }, 1.0, Anchor::Left, lw - p.px(40.0));
        regions.push(Region { x: lx, y: ly, w: lw, h: bh, action: Action::TodoField, vis: group_since(c, n0) });
        ly += bh + p.px(8.0);
    }
    if d.todo_done > 0 && d.todo_edit.is_none() {
        let clear = tr("Hapus yang selesai");
        let cw = p.measure(clear, 11.5, 600) + p.px(20.0);
        let ch = p.px(24.0);
        let n0 = kids(c);
        surface(p, c, lx, ly, cw, ch, ch / 2.0);
        p.rect(c, lx, ly, cw, ch, ch / 2.0, white(20));
        let th = p.px(11.5) * 1.3;
        p.text(c, clear, lx + cw / 2.0, ly + ch / 2.0 - th / 2.0, 11.5, 600, WHITE, 0.85, Anchor::Center, cw);
        regions.push(Region { x: lx, y: ly, w: cw, h: ch, action: Action::TodoClear, vis: group_since(c, n0) });
        ly += ch + p.px(8.0);
    }
    if d.todo_more > 0 {
        let more = format!("+{} {}", d.todo_more, localized("lainnya", "more", "más"));
        p.text(c, &more, lx + p.px(2.0), ly, 11.5, 600, WHITE, 0.5, Anchor::Left, lw);
    }
    let rx = lx + lw + p.px(14.0);
    let rw = f.x0 + f.w - pad - rx;
    let step = p.px(TODO_ROW);
    let row_h = step - p.px(6.0);
    let mut y = top;
    for t in &d.todo {
        let n0 = kids(c);
        let cy = y + row_h / 2.0;
        surface(p, c, rx, y, rw, row_h, row_h / 2.0);
        p.rect(c, rx, y, rw, row_h, row_h / 2.0, if t.done { rgb(GREEN, 44) } else { white(14) });
        let r = p.px(10.0);
        let ck = rx + p.px(9.0) + r;
        if t.done {
            p.rect(c, ck - r, cy - r, 2.0 * r, 2.0 * r, r, rgb(GREEN, 255));
            p.sym(c, Sym::Check, ck, cy, 11.0, WHITE, 1.0);
        } else {
            p.rect(c, ck - r, cy - r, 2.0 * r, 2.0 * r, r, white(110));
            let i = p.px(2.0);
            p.rect(c, ck - r + i, cy - r + i, 2.0 * (r - i), 2.0 * (r - i), r - i, SURFACE);
        }
        let due_w = if t.due.is_empty() { 0.0 } else { p.measure(&t.due, 12.0, 700) + p.px(6.0) };
        let tx = ck + r + p.px(11.0);
        let tw = rx + rw - p.px(14.0) - due_w - p.px(6.0) - tx;
        let lh = p.px(14.0) * 1.3;
        let label = p.fit(&t.text, 14.0, 600, tw);
        p.text(c, &label, tx, cy - lh / 2.0, 14.0, 600, WHITE, if t.done { 0.5 } else { 0.97 }, Anchor::Left, tw + p.px(4.0));
        if t.done {
            let sw = p.measure(&label, 14.0, 600).min(tw);
            p.rect(c, tx, cy - p.px(0.7), sw, p.px(1.4), p.px(0.7), white(150));
        }
        if !t.due.is_empty() {
            let dh = p.px(12.0) * 1.3;
            p.text(c, &t.due, rx + rw - p.px(14.0), cy - dh / 2.0, 12.0, 700, if t.late { ORANGE } else { WHITE }, if t.done { 0.4 } else { 0.85 }, Anchor::Right, due_w + p.px(4.0));
        }
        regions.push(Region { x: rx, y, w: rw, h: row_h, action: Action::TodoToggle(t.id), vis: group_since(c, n0) });
        y += step;
    }
    let n0 = kids(c);
    let cy = y + row_h / 2.0;
    let lh = p.px(14.0) * 1.3;
    match &d.todo_edit {
        None => {
            if d.todo.is_empty() {
                let mh = p.px(13.0) * 1.3;
                let msg = localized("Daftarmu kosong. Santai dulu.", "Your list is empty. Enjoy the quiet.", "Tu lista está vacía. Disfruta la calma.");
                p.text(c, msg, rx + rw / 2.0, top + p.px(52.0) - mh / 2.0, 13.0, 500, WHITE, 0.5, Anchor::Center, rw);
            }
            return;
        }
        Some((text, caret)) => {
            surface(p, c, rx, y, rw, row_h, row_h / 2.0);
            p.rect(c, rx, y, rw, row_h, row_h / 2.0, rgb(GREEN, 120));
            p.rect(c, rx + p.px(1.5), y + p.px(1.5), rw - p.px(3.0), row_h - p.px(3.0), row_h / 2.0 - p.px(1.5), SURFACE);
            let now = crate::tasks::now();
            let (_, due) = nacre_core::todo::parse(text, now);
            let chip = due.map(|m| nacre_core::todo::due_label_lang(m, now, lang));
            let chip_w = chip.as_ref().map_or(0.0, |t| p.measure(t, 11.5, 700) + p.px(30.0));
            let tx = rx + p.px(16.0);
            let avail = rw - p.px(30.0) - chip_w;
            let caret_h = row_h - p.px(14.0);
            if text.is_empty() {
                let ph = localized("Ketik tugas \u{00B7} mis. 15:30 rapat", "Type a task \u{00B7} e.g. 15:30 meeting", "Escribe una tarea \u{00B7} p. ej., reunión a las 15:30");
                p.text(c, ph, tx, cy - lh / 2.0, 14.0, 500, WHITE, 0.4, Anchor::Left, avail);
                if *caret {
                    p.rect(c, tx, cy - caret_h / 2.0, p.px(1.8), caret_h, p.px(0.9), rgb(GREEN, 255));
                }
            } else {
                let mut shown: String = text.clone();
                while p.measure(&shown, 14.0, 500) > avail - p.px(8.0) && shown.chars().count() > 1 {
                    shown = shown.chars().skip(1).collect();
                }
                p.text(c, &shown, tx, cy - lh / 2.0, 14.0, 500, WHITE, 0.97, Anchor::Left, avail + p.px(4.0));
                if *caret {
                    let cx = tx + p.measure(&shown, 14.0, 500) + p.px(1.5);
                    p.rect(c, cx, cy - caret_h / 2.0, p.px(1.8), caret_h, p.px(0.9), rgb(GREEN, 255));
                }
            }
            if let Some(t) = chip {
                let cx = rx + rw - p.px(6.0) - chip_w;
                let chh = row_h - p.px(12.0);
                p.rect(c, cx, cy - chh / 2.0, chip_w, chh, chh / 2.0, rgb(ORANGE, 60));
                p.sym(c, Sym::Clock, cx + p.px(12.0), cy, 10.0, ORANGE, 1.0);
                let ch = p.px(11.5) * 1.3;
                p.text(c, &t, cx + p.px(22.0), cy - ch / 2.0, 11.5, 700, ORANGE, 1.0, Anchor::Left, chip_w);
            }
        }
    }
    regions.push(Region { x: rx, y, w: rw, h: row_h, action: Action::TodoField, vis: group_since(c, n0) });
}

fn compact_charging(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data) {
    let cy = f.h / 2.0;
    let th = p.px(13.0) * 1.3;
    p.text(c, tr("Mengisi daya"), f.x0 + p.px(16.0), cy - th / 2.0, 12.5, 500, WHITE, 0.85, Anchor::Left, f.w / 2.0);
    let pct = d.battery.map_or(String::new(), |(b, _)| format!("\u{26A1} {b}%"));
    p.text(c, &pct, f.x0 + f.w - p.px(16.0), cy - th / 2.0, 13.0, 600, GREEN, 1.0, Anchor::Right, f.w / 2.0);
}

fn header(p: &mut Painter, c: &ContainerVisual, f: Frame, cur: Tab, d: &Data, regions: &mut Vec<Region>) {
    let cy = p.px(25.0);
    let accent = tab_accent(cur, d);
    let (a, b) = tab_title(cur);
    let x = f.x0 + p.px(20.0);
    let th = p.px(15.0) * 1.3;
    let aw = p.measure(a, 15.0, 700);
    p.text(c, a, x, cy - th / 2.0, 15.0, 700, WHITE, 1.0, Anchor::Left, aw + p.px(4.0));
    p.text(c, b, x + aw + p.px(5.0), cy - th / 2.0, 15.0, 700, accent, 1.0, Anchor::Left, p.px(160.0));
    let (date, time) = clock_parts();
    let (dw, tw) = (p.measure(&date, 12.5, 500), p.measure(&time, 13.0, 700));
    let gap = p.px(8.0);
    let cx = f.x0 + f.w / 2.0 + p.px(20.0) - (dw + gap + tw) / 2.0;
    let sh = p.px(12.5) * 1.3;
    p.text(c, &date, cx, cy - sh / 2.0, 12.5, 500, WHITE, 0.62, Anchor::Left, dw + p.px(4.0));
    let bh = p.px(13.0) * 1.3;
    p.text(c, &time, cx + dw + gap, cy - bh / 2.0, 13.0, 700, WHITE, 0.95, Anchor::Left, tw + p.px(4.0));
    let gx = f.x0 + f.w - p.px(28.0);
    let n0 = kids(c);
    p.sym(c, Sym::Gear, gx, cy, 15.0, WHITE, 0.62);
    regions.push(Region { x: gx - p.px(16.0), y: cy - p.px(15.0), w: p.px(32.0), h: p.px(30.0), action: Action::Settings, vis: group_since(c, n0) });
    if let Some((pct, charging)) = d.battery {
        let label = if charging { format!("\u{26A1}{pct}%") } else { format!("{pct}%") };
        let lh = p.px(12.0) * 1.3;
        p.text(c, &label, gx - p.px(18.0), cy - lh / 2.0, 12.0, 600, if charging { GREEN } else { WHITE }, 0.7, Anchor::Right, p.px(80.0));
    }
    page_dots(p, c, f, cur, accent, regions);
}

fn play(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>, live: &mut Live) {
    let (pad, gap) = (p.px(14.0), p.px(10.0));
    let (top, bottom) = (p.px(48.0), f.h - pad);
    let h = bottom - top;
    let mw = p.px(230.0);
    let lw = f.w - 2.0 * pad - mw - gap;
    let (mx, lx) = (f.x0 + pad, f.x0 + pad + mw + gap);
    match &d.media {
        Some(m) => music_card(p, c, mx, top, mw, h, m, regions),
        None => {
            card(p, c, mx, top, mw, h);
            p.sym(c, Sym::Note, mx + mw / 2.0, top + h / 2.0 - p.px(14.0), 26.0, WHITE, 0.35);
            p.text(c, tr("Tidak diputar"), mx + mw / 2.0, top + h / 2.0 + p.px(8.0), 12.5, 500, WHITE, 0.6, Anchor::Center, mw - p.px(20.0));
        }
    }
    lyrics_flow(p, c, lx, top, lw, h, d, live, Flow { size: 17.0, pad: 16.0, anchor: 0.30, card: false });
}

#[derive(Clone, Copy)]
struct Flow {
    size: f32,
    pad: f32,
    anchor: f32,
    card: bool,
}

#[allow(clippy::too_many_arguments)]
fn lyrics_flow(p: &mut Painter, c: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, d: &Data, live: &mut Live, fl: Flow) {
    if fl.card {
        card(p, c, x, y, w, h);
    }
    let ly = &d.lyrics;
    let note = match ly.lines {
        Some(l) if !l.synced => tr("Lirik tanpa waktu"),
        Some(_) if ly.lrclib => "LRCLIB",
        _ => "",
    };
    let center_msg = |p: &mut Painter, s: &str| {
        p.text(c, s, x + w / 2.0, y + h / 2.0 - p.px(8.0), 13.0, 600, WHITE, 0.45, Anchor::Center, w - p.px(20.0));
    };
    let Some(l) = ly.lines else {
        match (ly.state, d.media.is_some()) {
            (LyrState::Loading, _) => center_msg(p, tr("Mencari lirik\u{2026}")),
            (_, true) => center_msg(p, tr("Lirik tidak tersedia")),
            _ => center_msg(p, tr("Lirik tampil di sini saat lagu diputar.")),
        }
        return;
    };
    let go = || -> windows_core::Result<(ContainerVisual, ContainerVisual)> {
        let frame = p.comp.CreateContainerVisual()?;
        frame.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
        frame.SetSize(windows_numerics::Vector2 { X: w, Y: h })?;
        frame.SetClip(&p.comp.CreateInsetClip()?)?;
        c.Children()?.InsertAtTop(&frame)?;
        let scroll = p.comp.CreateContainerVisual()?;
        scroll.SetSize(windows_numerics::Vector2 { X: w, Y: h })?;
        frame.Children()?.InsertAtTop(&scroll)?;
        Ok((frame, scroll))
    };
    let Ok((frame, sc)) = go() else { return };
    let (pad, size) = (p.px(fl.pad), fl.size);
    let line_h = p.px(size) * 1.28;
    let gap = p.px(size * 0.53);
    let maxw = w - 2.0 * pad;
    let blocks = |p: &Painter, text: &str| -> Vec<String> {
        let t = if text.is_empty() { "\u{266A}" } else { text };
        nacre_core::notch::typeset::wrap(t, maxw, 3, |s| p.measure(s, size, 700))
    };
    if l.synced {
        let active = l.index_at(ly.pos_ms);
        let center = active.unwrap_or(0);
        let ya = h * fl.anchor;
        let mut drawn: Vec<(usize, f32, Vec<String>)> = Vec::new();
        if center > 0 {
            let prev = blocks(p, &l.lines[center - 1].text);
            let hp = prev.len() as f32 * line_h;
            live.lyric_shift = hp + gap;
            drawn.push((center - 1, ya - gap - hp, prev));
        }
        let mut yy = ya;
        for i in center..l.lines.len() {
            if yy > h {
                break;
            }
            let b = blocks(p, &l.lines[i].text);
            let hb = b.len() as f32 * line_h;
            drawn.push((i, yy, b));
            yy += hb + gap;
        }
        for (i, top, lines) in drawn {
            let on = active == Some(i);
            for (k, t) in lines.iter().enumerate() {
                let ty = top + k as f32 * line_h;
                if on {
                    p.text(&sc, t, pad, ty, size, 700, WHITE, 1.0, Anchor::Left, maxw + p.px(4.0));
                } else {
                    p.text_soft(&sc, t, pad, ty, size, 700, WHITE, 0.32, (2.2 * p.dpi).round());
                }
            }
        }
        live.lyric_scroll = Some(sc);
    } else {
        let mut yy = pad;
        for line in l.lines.iter().filter(|x| !x.text.is_empty()) {
            if yy > h {
                break;
            }
            for t in nacre_core::notch::typeset::wrap(&line.text, maxw, 3, |s| p.measure(s, 15.0, 600)) {
                p.text(&sc, &t, pad, yy, 15.0, 600, WHITE, 0.8, Anchor::Left, maxw + p.px(4.0));
                yy += p.px(15.0) * 1.28;
            }
            yy += gap * 0.6;
        }
    }
    if fl.card {
        p.text(&frame, note, w - p.px(12.0), h - p.px(18.0), 9.5, 500, WHITE, 0.35, Anchor::Right, w / 2.0);
    }
}

fn shelf_empty(p: &mut Painter, c: &ContainerVisual, f: Frame) {
    let pad = p.px(14.0);
    let (top, h) = (p.px(48.0), f.h - p.px(48.0) - pad);
    card(p, c, f.x0 + pad, top, f.w - 2.0 * pad, h);
    let cx = f.x0 + f.w / 2.0;
    p.sym(c, Sym::Plus, cx, top + h / 2.0 - p.px(16.0), 18.0, WHITE, 0.4);
    p.text(c, tr("Rak kosong"), cx, top + h / 2.0 + p.px(2.0), 13.0, 600, WHITE, 0.8, Anchor::Center, f.w);
    p.text(c, tr("Seret berkas ke Visor untuk menaruhnya di sini."), cx, top + h / 2.0 + p.px(22.0), 12.0, 400, WHITE, 0.5, Anchor::Center, f.w);
}

fn round_button(p: &mut Painter, c: &ContainerVisual, cx: f32, cy: f32, d: f32, fill: Color) {
    if fill.A < 255 {
        p.rect(c, cx - d / 2.0, cy - d / 2.0, d, d, d / 2.0, SURFACE);
    }
    p.rect(c, cx - d / 2.0, cy - d / 2.0, d, d, d / 2.0, fill);
}

fn timer(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>) {
    let (pad, gap) = (p.px(14.0), p.px(10.0));
    let (top, h) = (p.px(48.0), f.h - p.px(48.0) - pad);
    let cw = (f.w - 2.0 * pad - gap) / 2.0;
    timer_card(p, c, f.x0 + pad, top, cw, h, d, regions);
    let rx = f.x0 + pad + cw + gap;
    if d.focus.show_cal {
        calendar_card(p, c, rx, top, cw, h, d, regions);
    } else {
        focus_stats(p, c, rx, top, cw, h, d);
    }
    side_chip(p, c, rx + cw, top, d, regions);
}

fn side_chip(p: &mut Painter, c: &ContainerVisual, right: f32, top: f32, d: &Data, regions: &mut Vec<Region>) {
    let label = if d.focus.show_cal { tr("Fokus") } else { tr("Kalender") };
    let text = format!("{label} \u{203A}");
    let w = p.measure(&text, 10.5, 600) + p.px(18.0);
    let (x, y, h) = (right - w - p.px(10.0), top + p.px(10.0), p.px(20.0));
    let n0 = kids(c);
    p.rect(c, x, y, w, h, h / 2.0, white(26));
    let th = p.px(10.5) * 1.3;
    p.text(c, &text, x + w / 2.0, y + (h - th) / 2.0, 10.5, 600, WHITE, 0.85, Anchor::Center, w);
    regions.push(Region { x: x - p.px(4.0), y: y - p.px(4.0), w: w + p.px(8.0), h: h + p.px(8.0), action: Action::FocusSide, vis: group_since(c, n0) });
}

#[allow(clippy::too_many_arguments)]
fn focus_stats(p: &mut Painter, c: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, d: &Data) {
    card(p, c, x, y, w, h);
    let lang = crate::prefs::get().lang;
    let f = &d.focus;
    let pad = p.px(16.0);
    p.text(c, tr("Hari ini"), x + pad, y + p.px(12.0), 11.5, 500, WHITE, 0.55, Anchor::Left, w * 0.5);
    p.text(c, &nacre_core::focus::duration_label_lang(f.today.minutes, lang), x + pad, y + p.px(28.0), 24.0, 700, WHITE, 1.0, Anchor::Left, w - 2.0 * pad);
    let sub = match f.today.top {
        Some(t) => format!("{} {} \u{00B7} {}", f.today.sessions, tr("sesi"), f.names[t]),
        None => tr("Belum ada sesi").to_string(),
    };
    p.text(c, &p.fit(&sub, 11.0, 500, w - 2.0 * pad), x + pad, y + p.px(60.0), 11.0, 500, WHITE, 0.6, Anchor::Left, w - 2.0 * pad);
    let max = f.week.iter().copied().max().unwrap_or(0).max(1) as f32;
    let (bx, bw_all) = (x + pad, w - 2.0 * pad);
    let gap = p.px(5.0);
    let bw = (bw_all - 6.0 * gap) / 7.0;
    let bottom = y + h - p.px(14.0);
    let tall = (bottom - (y + p.px(82.0))).max(p.px(8.0));
    let rgb = f.rgb();
    for (i, m) in f.week.iter().enumerate() {
        let bh = (tall * (*m as f32 / max)).max(p.px(3.0));
        let col = if i == 6 { Color { A: 255, R: rgb[0], G: rgb[1], B: rgb[2] } } else { white(46) };
        p.rect(c, bx + i as f32 * (bw + gap), bottom - bh, bw, bh, p.px(3.0), col);
    }
}

#[allow(clippy::too_many_arguments)]
fn timer_card(p: &mut Painter, c: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, d: &Data, regions: &mut Vec<Region>) {
    card(p, c, x, y, w, h);
    let t = d.timer;
    let secs = t.remaining_secs(d.now_ms);
    let f = &d.focus;
    let cat_rgb = f.rgb();
    let running = t.phase == Phase::Running;
    let pad = p.px(12.0);
    let (mut chx, chy, chh) = (x + pad, y + p.px(10.0), p.px(20.0));
    let chip_max = (w - 2.0 * pad - 3.0 * p.px(5.0)) / 4.0;
    for (i, name) in f.names.iter().enumerate() {
        let on = i == f.cat;
        let shown = p.fit(name, 10.5, 600, chip_max - p.px(12.0));
        let cw = (p.measure(&shown, 10.5, 600) + p.px(14.0)).min(chip_max);
        let n0 = kids(c);
        let rgb = nacre_core::focus::COLORS[i];
        let fill = if on { Color { A: 255, R: rgb[0], G: rgb[1], B: rgb[2] } } else { white(if running { 10 } else { 22 }) };
        p.rect(c, chx, chy, cw, chh, chh / 2.0, fill);
        let th = p.px(10.5) * 1.3;
        p.text(c, &shown, chx + cw / 2.0, chy + (chh - th) / 2.0, 10.5, 600, WHITE, if on { 1.0 } else if running { 0.35 } else { 0.8 }, Anchor::Center, cw);
        if !running && !on {
            regions.push(Region { x: chx, y: chy - p.px(4.0), w: cw, h: chh + p.px(8.0), action: Action::FocusCat(i as u8), vis: group_since(c, n0) });
        }
        chx += cw + p.px(5.0);
    }
    let rgb = if running && secs <= 60 { ORANGE } else { WHITE };
    let ring_r = 17.0;
    let rcx = x + pad + p.px(ring_r + 3.0);
    let rcy = y + p.px(58.0);
    let frac = if t.duration > 0 { 1.0 - (t.remaining_ms(d.now_ms) as f32 / (t.duration as f32 * 1000.0)) } else { 0.0 };
    let frac = (frac.clamp(0.0, 1.0) * 120.0).round() / 120.0;
    p.arc(c, rcx, rcy, ring_r, 4.0, 0.0, 1.0, WHITE, 40);
    if frac > 0.0 {
        p.arc(c, rcx, rcy, ring_r, 4.0, 0.0, frac, cat_rgb, 255);
    }
    let tx = rcx + p.px(ring_r + 12.0);
    p.text(c, &format_mmss(secs), tx, rcy - p.px(22.0), 26.0, 600, rgb, 1.0, Anchor::Left, x + w - tx);
    let status = match (t.phase, f.stage) {
        (Phase::Paused, _) => tr("Dijeda").to_string(),
        (_, nacre_core::focus::Stage::Break) => tr("Istirahat").to_string(),
        _ => format!("{} {} {} {}", tr("Fokus"), f.round, tr("dari"), nacre_core::focus::ROUNDS),
    };
    p.text(c, &status, tx, rcy + p.px(8.0), 11.0, 500, WHITE, 0.55, Anchor::Left, x + w - tx);
    let (bd, bgap) = (p.px(34.0), p.px(14.0));
    let total = 4.0 * bd + 3.0 * bgap;
    let cy = y + h - p.px(12.0) - bd / 2.0;
    let mut cx = x + (w - total) / 2.0 + bd / 2.0;
    let idle = matches!(t.phase, Phase::Idle | Phase::Finished);
    let buttons: [(Action, bool); 4] = [(Action::TimerMinus, idle), (Action::TimerToggle, true), (Action::TimerReset, true), (Action::TimerPlus, idle)];
    for (a, enabled) in buttons {
        let primary = a == Action::TimerToggle;
        let fill = if primary { Color { A: 255, R: cat_rgb[0], G: cat_rgb[1], B: cat_rgb[2] } } else { white(if enabled { 30 } else { 14 }) };
        let n0 = kids(c);
        round_button(p, c, cx, cy, bd, fill);
        let op = if enabled { 1.0 } else { 0.35 };
        match a {
            Action::TimerToggle => p.sym(c, if t.phase == Phase::Running { Sym::Pause } else { Sym::Play }, cx, cy, 16.0, WHITE, 1.0),
            Action::TimerReset => p.sym(c, Sym::Restart, cx, cy, 15.0, WHITE, 0.9),
            Action::TimerMinus | Action::TimerPlus => {
                let th = p.px(12.0) * 1.3;
                let s = if a == Action::TimerMinus { "\u{2212}5" } else { "+5" };
                p.text(c, s, cx, cy - th / 2.0, 12.0, 600, WHITE, op, Anchor::Center, bd);
            }
            _ => {}
        }
        if enabled {
            regions.push(Region { x: cx - bd / 2.0, y: cy - bd / 2.0, w: bd, h: bd, action: a, vis: group_since(c, n0) });
        }
        cx += bd + bgap;
    }
}

#[allow(clippy::too_many_arguments)]
fn calendar_card(p: &mut Painter, c: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, d: &Data, regions: &mut Vec<Region>) {
    use nacre_core::island::calendar::{add_days, month_short, weekday, weekday_short};
    card(p, c, x, y, w, h);
    let lang = crate::prefs::get().lang;
    let cal = &d.cal;
    let accent = crate::prefs::accent();
    let (sy, sm, sd) = cal.sel;
    p.text(c, month_short(sm, lang), x + p.px(16.0), y + p.px(12.0), 18.0, 700, WHITE, 1.0, Anchor::Left, p.px(60.0));
    p.text(c, &sy.to_string(), x + p.px(16.0), y + p.px(36.0), 11.0, 500, WHITE, 0.45, Anchor::Left, p.px(60.0));
    let (left, right) = (x + p.px(70.0), x + w - p.px(10.0));
    let cell = (right - left) / 5.0;
    let (wy, ny, dia) = (y + p.px(10.0), y + p.px(34.0), p.px(28.0));
    for i in -2i32..=2 {
        let (yy, mm, dd) = add_days(sy, sm, sd, i);
        let cx = left + cell * (i + 2) as f32 + cell / 2.0;
        let selected = i == 0;
        let today = (yy, mm, dd) == cal.today;
        let wd = weekday_short(weekday(yy, mm, dd), lang);
        let n0 = kids(c);
        p.text(c, wd, cx, wy, 9.0, 600, if selected { accent } else { WHITE }, if selected { 1.0 } else { 0.4 }, Anchor::Center, cell);
        if selected {
            p.rect(c, cx - dia / 2.0, ny, dia, dia, dia / 2.0, Color { A: 255, R: accent[0], G: accent[1], B: accent[2] });
        }
        let th = p.px(14.0) * 1.3;
        let (rgb, op) = if selected { (WHITE, 1.0) } else if today { (accent, 1.0) } else { (WHITE, 0.8) };
        p.text(c, &format!("{dd:02}"), cx, ny + dia / 2.0 - th / 2.0, 14.0, if selected || today { 700 } else { 500 }, rgb, op, Anchor::Center, cell);
        if !selected {
            regions.push(Region { x: cx - cell / 2.0, y: wy, w: cell, h: ny + dia - wy, action: Action::CalDay(i), vis: group_since(c, n0) });
        }
    }
    let is_today = cal.sel == cal.today;
    let line = match &cal.events {
        Events::Loading => "\u{2026}".to_string(),
        Events::Unavailable => String::new(),
        Events::List([]) => tr(if is_today { "Tidak ada acara hari ini" } else { "Tidak ada acara" }).to_string(),
        Events::List(list) => {
            let e = &list[0];
            let when = match e.minute {
                Some(m) if crate::prefs::get().clock_24h => format!("{:02}.{:02}  ", m / 60, m % 60),
                Some(m) => format!("{}.{:02} {}  ", (m / 60 + 11) % 12 + 1, m % 60, if m < 720 { "AM" } else { "PM" }),
                None => String::new(),
            };
            let more = if list.len() > 1 { format!("  +{}", list.len() - 1) } else { String::new() };
            format!("{when}{}{more}", e.title)
        }
    };
    if !line.is_empty() {
        let ly = y + h - p.px(26.0);
        p.sym(c, Sym::Calendar, x + p.px(22.0), ly + p.px(7.5), 12.0, WHITE, 0.55);
        let maxw = w - p.px(44.0);
        let shown = p.fit(&line, 11.5, 500, maxw);
        p.text(c, &shown, x + p.px(34.0), ly, 11.5, 500, WHITE, 0.75, Anchor::Left, maxw + p.px(4.0));
    }
}
