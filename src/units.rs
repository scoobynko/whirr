pub fn fmt_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if b == 0 {
        return "0 B".into();
    }
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Gibibytes to one decimal with a bare `G` suffix, e.g. `6.5G` — the compact
/// form the Memory card's hero number and its folded pressure/swap line use
/// where `fmt_bytes`' longer `6.5 GB` would not fit. Same 1024 base as
/// `fmt_bytes`, so the two never disagree about a value.
pub fn fmt_gib(b: u64) -> String {
    format!("{:.1}G", b as f64 / 1024.0_f64.powi(3))
}

pub fn fmt_rate(bps: f64) -> String {
    if bps < 1.0 {
        return "0 B/s".into();
    }
    format!("{}/s", fmt_bytes(bps as u64))
}

pub fn fmt_duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86400, (secs % 86400) / 3600, (secs % 3600) / 60);
    match (d, h, m) {
        (0, 0, 0) => format!("{secs}s"),
        (0, 0, m) => format!("{m}m"),
        (0, h, m) => format!("{h}h {m}m"),
        (d, h, _) => format!("{d}d {h}h"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_scale() {
        assert_eq!(fmt_bytes(0), "0 B");
        assert_eq!(fmt_bytes(1536), "1.5 KB");
        assert_eq!(fmt_bytes(16 * 1024 * 1024 * 1024), "16.0 GB");
    }

    #[test]
    fn gib_matches_fmt_bytes_at_the_same_scale() {
        assert_eq!(fmt_gib(0), "0.0G");
        assert_eq!(fmt_gib(7 * 1024 * 1024 * 1024), "7.0G");
        // Same base as fmt_bytes, so a card showing both never contradicts
        // itself: 7_000_000_000 B is 6.5 GiB either way.
        assert_eq!(fmt_gib(7_000_000_000), "6.5G");
        assert_eq!(fmt_bytes(7_000_000_000), "6.5 GB");
    }

    #[test]
    fn rate_scale() {
        assert_eq!(fmt_rate(0.0), "0 B/s");
        assert_eq!(fmt_rate(1_300_000.0), "1.2 MB/s");
    }

    #[test]
    fn duration_tiers() {
        assert_eq!(fmt_duration(42), "42s");
        assert_eq!(fmt_duration(12 * 60), "12m");
        assert_eq!(fmt_duration(4 * 3600 + 12 * 60), "4h 12m");
        assert_eq!(fmt_duration(3 * 86400 + 4 * 3600), "3d 4h");
    }
}
