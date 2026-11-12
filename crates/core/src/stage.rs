//! Model Stage Manager: satu grup app di "panggung" (boleh lebih dari satu app), grup lain terbaru di strip samping.
//! Murni: pemanggil menjalankan hasilnya (parkir/tampilkan jendela).

pub const MAX_STRIP: usize = 6;

pub type Group = Vec<String>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stage {
    /// App di panggung (satu grup; urutan = urutan ditambahkan).
    pub current: Group,
    /// Grup di strip, paling baru lebih dulu. Tidak berisi app yang ada di `current`.
    pub strip: Vec<Group>,
    /// Batas jumlah grup di strip (0 = `MAX_STRIP`).
    pub max: usize,
}

/// Hasil operasi: app yang harus diparkir (keluar panggung) dan yang harus ditampilkan (masuk panggung).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub park: Vec<String>,
    pub show: Vec<String>,
}

impl Stage {
    fn cap(&self) -> usize {
        if self.max == 0 { MAX_STRIP } else { self.max }
    }

    /// Masukkan grup di depan strip; grup yang terdorong keluar batas dikembalikan (jendelanya harus ditampilkan lagi).
    fn push_front_group(&mut self, g: Group) -> Vec<String> {
        if g.is_empty() {
            return Vec::new();
        }
        self.strip.insert(0, g);
        let cap = self.cap();
        if self.strip.len() > cap {
            self.strip.split_off(cap).into_iter().flatten().collect()
        } else {
            Vec::new()
        }
    }

    fn take_from_strip(&mut self, app: &str) -> Option<Group> {
        let i = self.strip.iter().position(|g| g.iter().any(|a| a == app))?;
        Some(self.strip.remove(i))
    }

    pub fn contains(&self, app: &str) -> bool {
        self.current.iter().any(|a| a == app) || self.strip.iter().any(|g| g.iter().any(|a| a == app))
    }

    /// `app` mendapat fokus. `running`: semua app yang punya jendela (yang belum dikenal masuk strip).
    pub fn focus(&mut self, app: &str, running: &[String]) -> Change {
        let mut ch = Change::default();
        if !self.current.iter().any(|a| a == app) {
            let incoming = self.take_from_strip(app).unwrap_or_else(|| vec![app.to_string()]);
            let prev = std::mem::replace(&mut self.current, incoming.clone());
            ch.park.extend(prev.iter().cloned());
            ch.show.extend(incoming);
            let dropped = self.push_front_group(prev);
            for a in &dropped {
                ch.park.retain(|p| p != a);
            }
            ch.show.extend(dropped);
        }
        // app berjalan yang belum punya tempat: jadi grup tunggal di ujung strip
        for a in running {
            if !self.contains(a) && self.strip.len() < self.cap() {
                self.strip.push(vec![a.clone()]);
                if !ch.park.contains(a) {
                    ch.park.push(a.clone());
                }
            }
        }
        ch
    }

    /// Gabungkan `app` ke grup panggung (seret kartu strip ke panggung / Shift-klik).
    pub fn add_to_stage(&mut self, app: &str) -> Change {
        let mut ch = Change::default();
        if self.current.iter().any(|a| a == app) {
            return ch;
        }
        let grp = self.take_from_strip(app).unwrap_or_else(|| vec![app.to_string()]);
        for a in grp {
            if !self.current.contains(&a) {
                ch.show.push(a.clone());
                self.current.push(a);
            }
        }
        ch
    }

    /// Keluarkan `app` dari grup panggung ke strip (hanya bila masih ada app lain di panggung).
    pub fn remove_from_stage(&mut self, app: &str) -> Change {
        let mut ch = Change::default();
        if self.current.len() > 1 {
            if let Some(i) = self.current.iter().position(|a| a == app) {
                let a = self.current.remove(i);
                ch.park.push(a.clone());
                let dropped = self.push_front_group(vec![a]);
                ch.park.retain(|p| !dropped.contains(p));
                ch.show.extend(dropped);
            }
        }
        ch
    }

    /// Tampilkan desktop: seluruh panggung masuk strip.
    pub fn desktop(&mut self) -> Change {
        let g = std::mem::take(&mut self.current);
        let mut ch = Change { park: g.clone(), show: vec![] };
        let dropped = self.push_front_group(g);
        ch.park.retain(|p| !dropped.contains(p));
        ch.show.extend(dropped);
        ch
    }

    /// Buang app yang sudah tidak berjalan.
    pub fn prune(&mut self, running: &[String]) {
        self.current.retain(|a| running.contains(a));
        for g in &mut self.strip {
            g.retain(|a| running.contains(a));
        }
        self.strip.retain(|g| !g.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn switching_moves_previous_group_to_strip_front_and_parks_it() {
        let mut st = Stage::default();
        let running = s(&["a", "b", "c"]);
        st.focus("a", &running);
        let ch = st.focus("b", &running);
        assert_eq!(st.current, s(&["b"]));
        assert_eq!(st.strip[0], s(&["a"]));
        assert!(ch.park.contains(&"a".to_string()));
        assert!(ch.show.contains(&"b".to_string()));
    }

    #[test]
    fn focusing_a_strip_app_removes_it_from_strip() {
        let mut st = Stage::default();
        let running = s(&["a", "b"]);
        st.focus("a", &running);
        st.focus("b", &running);
        st.focus("a", &running);
        assert_eq!(st.current, s(&["a"]));
        assert_eq!(st.strip, vec![s(&["b"])]);
    }

    #[test]
    fn strip_is_capped_and_pruned() {
        let mut st = Stage::default();
        let running: Vec<String> = (0..10).map(|i| format!("app{i}")).collect();
        for r in &running {
            st.focus(r, &running);
        }
        assert!(st.strip.len() <= MAX_STRIP);
        st.prune(&s(&["app9"]));
        assert!(st.strip.is_empty());
        assert_eq!(st.current, s(&["app9"]));
    }

    #[test]
    fn same_app_focus_is_idempotent() {
        let mut st = Stage::default();
        let running = s(&["a", "b"]);
        st.focus("a", &running);
        let before = st.clone();
        let ch = st.focus("a", &running);
        assert_eq!(st, before);
        assert!(ch.show.is_empty());
    }

    #[test]
    fn add_and_remove_group_members() {
        let mut st = Stage::default();
        let running = s(&["a", "b", "c"]);
        st.focus("a", &running);
        let ch = st.add_to_stage("b");
        assert_eq!(st.current, s(&["a", "b"]));
        assert_eq!(ch.show, s(&["b"]));
        assert!(!st.strip.iter().any(|g| g.contains(&"b".to_string())));
        // pindah ke app lain memarkir seluruh grup dan grup kembali utuh saat dipilih
        let ch = st.focus("c", &running);
        assert_eq!(ch.park, s(&["a", "b"]));
        assert_eq!(st.strip[0], s(&["a", "b"]));
        let ch = st.focus("b", &running);
        assert_eq!(st.current, s(&["a", "b"]));
        assert_eq!(ch.show, s(&["a", "b"]));
        let ch = st.remove_from_stage("b");
        assert_eq!(st.current, s(&["a"]));
        assert_eq!(ch.park, s(&["b"]));
        assert_eq!(st.strip[0], s(&["b"]));
    }

    #[test]
    fn desktop_parks_everything() {
        let mut st = Stage::default();
        let running = s(&["a", "b"]);
        st.focus("a", &running);
        st.add_to_stage("b");
        let ch = st.desktop();
        assert!(st.current.is_empty());
        assert_eq!(ch.park, s(&["a", "b"]));
        assert_eq!(st.strip[0], s(&["a", "b"]));
    }

    #[test]
    fn cannot_remove_last_app_from_stage() {
        let mut st = Stage::default();
        st.focus("a", &s(&["a"]));
        let ch = st.remove_from_stage("a");
        assert!(ch.park.is_empty());
        assert_eq!(st.current, s(&["a"]));
    }

    #[test]
    fn custom_cap_drops_oldest_group_back_to_screen() {
        let mut st = Stage { max: 2, ..Default::default() };
        let running: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        st.focus("a", &running);
        st.focus("b", &running);
        let ch = st.focus("c", &running);
        assert!(st.strip.len() <= 2);
        // grup yang terdorong keluar tidak boleh tetap diparkir
        for g in st.strip.iter() {
            for a in g {
                assert!(!ch.show.contains(a));
            }
        }
    }
}
