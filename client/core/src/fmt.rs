//! Display formatting shared by the frontends. Pure functions — the caller
//! supplies "now" so this stays testable off-wasm.

/// Coarse relative time ("just now", "5m ago", "3d ago"). Both stamps are epoch
/// milliseconds. Future stamps (clock skew between server and client) clamp to
/// "just now" rather than rendering a negative age.
pub fn rel_time(now_ms: f64, then_ms: f64) -> String {
    let secs = ((now_ms - then_ms) / 1000.0).max(0.0);
    // Descending thresholds: first match wins, so the coarsest useful unit shows.
    const UNITS: [(f64, &str); 4] = [(86_400.0, "d"), (3_600.0, "h"), (60.0, "m"), (1.0, "s")];
    for (size, suffix) in UNITS {
        if secs >= size {
            // ponytail: no weeks/months — a lobby row past ~30d is stale anyway.
            return format!("{}{suffix} ago", (secs / size) as i64);
        }
    }
    "just now".to_string()
}

/// `n` with a unit, pluralised the boring English way ("1 clue", "12 clues").
pub fn plural(n: i64, unit: &str) -> String {
    if n == 1 {
        format!("{n} {unit}")
    } else {
        format!("{n} {unit}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_time_picks_the_coarsest_unit() {
        let now = 1_000_000_000.0;
        assert_eq!(rel_time(now, now), "just now");
        assert_eq!(rel_time(now, now - 500.0), "just now");
        assert_eq!(rel_time(now, now - 5_000.0), "5s ago");
        assert_eq!(rel_time(now, now - 90_000.0), "1m ago");
        assert_eq!(rel_time(now, now - 7_200_000.0), "2h ago");
        assert_eq!(rel_time(now, now - 3.0 * 86_400_000.0), "3d ago");
        // clock skew: server stamp ahead of the client clock
        assert_eq!(rel_time(now, now + 60_000.0), "just now");
    }

    #[test]
    fn plural_only_drops_the_s_at_one() {
        assert_eq!(plural(0, "clue"), "0 clues");
        assert_eq!(plural(1, "clue"), "1 clue");
        assert_eq!(plural(2, "player"), "2 players");
    }
}
