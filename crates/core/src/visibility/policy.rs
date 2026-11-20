#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Always,
    AutoHide,
    OnDemand,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "always" | "fixed" => Some(Mode::Always),
            "auto" | "smart" | "auto-hide" => Some(Mode::AutoHide),
            "peek" | "demand" => Some(Mode::OnDemand),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Inputs {
    pub overlapped: bool,
    pub interacting: bool,
    pub edge_armed: bool,
    pub starting: bool,
    pub fullscreen: bool,
    pub revealing: bool,
    pub desktop_empty: bool,
}

pub fn should_hide(mode: Mode, i: &Inputs) -> bool {
    if i.revealing {
        return false;
    }
    if i.fullscreen {
        return !i.interacting;
    }
    if i.starting {
        return false;
    }
    match mode {
        Mode::Always => false,
        Mode::AutoHide => !i.desktop_empty && !i.interacting && !i.edge_armed,
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
    fn autohide_stays_hidden_until_hover_or_interaction() {
        assert!(should_hide(Mode::AutoHide, &i()));
        assert!(should_hide(Mode::AutoHide, &Inputs { overlapped: true, ..i() }));
        assert!(!should_hide(Mode::AutoHide, &Inputs { interacting: true, ..i() }));
        assert!(!should_hide(Mode::AutoHide, &Inputs { edge_armed: true, ..i() }));
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
    fn minimize_reveal_shows_surface_in_every_mode() {
        for mode in [Mode::Always, Mode::AutoHide, Mode::OnDemand] {
            assert!(!should_hide(mode, &Inputs { revealing: true, overlapped: true, fullscreen: true, ..i() }));
        }
    }

    #[test]
    fn legacy_mode_names_map_to_one_policy() {
        assert_eq!(Mode::parse("fixed"), Some(Mode::Always));
        assert_eq!(Mode::parse("smart"), Some(Mode::AutoHide));
        assert_eq!(Mode::parse("peek"), Some(Mode::OnDemand));
        assert_eq!(Mode::parse("???"), None);
    }
}
