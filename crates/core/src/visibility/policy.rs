//! Kebijakan tampil/sembunyi untuk permukaan di tepi layar (dock, kelak island).
//! Satu fungsi murni menggantikan tiga mode terpisah di referensi.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Selalu tampil.
    Always,
    /// Sembunyi saat window lain menimpa area permukaan (maksimum, fullscreen, atau bersinggungan).
    AutoHide,
    /// Sembunyi kecuali sedang dipakai atau tepi layar disentuh cukup lama.
    OnDemand,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "always" | "fixed" => Some(Mode::Always),
            // nama lama referensi dipetakan ke satu mode
            "auto" | "smart" | "auto-hide" => Some(Mode::AutoHide),
            "peek" | "demand" => Some(Mode::OnDemand),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Inputs {
    /// Window lain menimpa area permukaan.
    pub overlapped: bool,
    /// Kursor di permukaan, menu terbuka, pratinjau terbuka, atau masih dalam masa tenggang sesudahnya.
    pub interacting: bool,
    /// Kursor bertahan di tepi layar melewati jeda dwell.
    pub edge_armed: bool,
    /// Animasi awal: jangan menyembunyikan dulu.
    pub starting: bool,
    /// Foreground fullscreen: permukaan menyingkir sepenuhnya, bahkan bila tepi disentuh.
    pub fullscreen: bool,
}

pub fn should_hide(mode: Mode, i: &Inputs) -> bool {
    if i.fullscreen {
        return !i.interacting;
    }
    if i.starting {
        return false;
    }
    match mode {
        Mode::Always => false,
        Mode::AutoHide => i.overlapped && !i.interacting && !i.edge_armed,
        Mode::OnDemand => !i.interacting && !i.edge_armed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i() -> Inputs {
        Inputs::default()
    }

    #[test]
    fn always_never_hides_except_fullscreen() {
        assert!(!should_hide(Mode::Always, &Inputs { overlapped: true, ..i() }));
        assert!(should_hide(Mode::Always, &Inputs { fullscreen: true, ..i() }));
    }

    #[test]
    fn autohide_hides_only_when_overlapped_and_idle() {
        assert!(!should_hide(Mode::AutoHide, &i()));
        assert!(should_hide(Mode::AutoHide, &Inputs { overlapped: true, ..i() }));
        assert!(!should_hide(Mode::AutoHide, &Inputs { overlapped: true, interacting: true, ..i() }));
        assert!(!should_hide(Mode::AutoHide, &Inputs { overlapped: true, edge_armed: true, ..i() }));
    }

    #[test]
    fn ondemand_hides_unless_used_or_edge_touched() {
        assert!(should_hide(Mode::OnDemand, &i()));
        assert!(!should_hide(Mode::OnDemand, &Inputs { interacting: true, ..i() }));
        assert!(!should_hide(Mode::OnDemand, &Inputs { edge_armed: true, ..i() }));
    }

    #[test]
    fn startup_animation_blocks_hiding() {
        assert!(!should_hide(Mode::OnDemand, &Inputs { starting: true, ..i() }));
    }

    #[test]
    fn fullscreen_hides_even_when_edge_is_armed() {
        assert!(should_hide(Mode::AutoHide, &Inputs { fullscreen: true, edge_armed: true, ..i() }));
        assert!(!should_hide(Mode::AutoHide, &Inputs { fullscreen: true, interacting: true, ..i() }));
    }

    #[test]
    fn legacy_mode_names_map_to_one_policy() {
        assert_eq!(Mode::parse("fixed"), Some(Mode::Always));
        assert_eq!(Mode::parse("smart"), Some(Mode::AutoHide));
        assert_eq!(Mode::parse("peek"), Some(Mode::OnDemand));
        assert_eq!(Mode::parse("???"), None);
    }
}
