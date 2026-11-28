pub const QUIET_MS: u64 = 60;
pub const MAX_WAIT_MS: u64 = 300;
pub const SWITCH_MS: u64 = 20;

pub fn delay(age_ms: u64, switched: bool) -> u64 {
    if switched {
        return SWITCH_MS;
    }
    QUIET_MS.min(MAX_WAIT_MS.saturating_sub(age_ms)).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_apps_is_handled_fast() {
        assert_eq!(delay(0, true), SWITCH_MS);
        assert_eq!(delay(250, true), SWITCH_MS);
    }

    #[test]
    fn a_stream_of_events_cannot_postpone_forever() {
        assert_eq!(delay(0, false), QUIET_MS);
        assert_eq!(delay(250, false), 50);
        assert_eq!(delay(299, false), 1);
        assert_eq!(delay(5000, false), 1);
        let mut age = 0u64;
        let mut fire_at = delay(age, false);
        while age < 1000 {
            age += 40;
            fire_at = fire_at.min(age + delay(age, false));
        }
        assert!(fire_at <= MAX_WAIT_MS, "event tiap 40 ms tetap memicu pengecekan paling lambat {MAX_WAIT_MS} ms");
    }
}
