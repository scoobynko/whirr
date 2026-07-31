//! Resolving a terminal size into a layout, once, before anything renders.
//!
//! This used to be eight booleans derived inline in `draw` (`show_ports`,
//! `show_power`, `full`, `grid`, `hero_tier`, `three_cards`, …). They were not
//! independent — `grid` was only correct because it happened to reuse the
//! column floor that decides whether the Power card exists at all, and
//! `three_cards` re-derived half of `full`'s condition from the same literal.
//! Those relationships lived in prose. Here they are the code: `Tier::Grid` is
//! reachable only when all four gauges are present, because it asks the gauge
//! list rather than re-deriving the threshold that produced it.

use ratatui::layout::Rect;

/// One gauge card in the band under the header, in display order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gauge {
    Cpu,
    Temp,
    Power,
    Memory,
}

/// Visual tier, in order of preference.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    /// Padded header with the braille burst fan, one row of four hero-number
    /// gauge cards. Needs width for four ~28-col heroes abreast.
    Full,
    /// The same header and the same hero cards, but the four gauges stacked
    /// 2x2 across two bands. A hero card needs 30 columns, so four abreast
    /// need 120 while two need only 60 — a terminal that is merely narrow can
    /// still show the full design by trading a second band for the width.
    /// This is what keeps sizes like 103x45 on the real visuals.
    Grid,
    /// Short header, and gauge cards carrying plain readouts instead of hero
    /// numbers — there is no room for a 5-row hero in an 8-row band. It still
    /// shares the burst fan and the bitmap wordmark with the tiers above.
    Compact,
}

impl Tier {
    /// Rows the header band claims. The compact band is 5 rather than 3
    /// because both tiers share one 5-row bitmap wordmark — the compact
    /// tier's old block-glyph face broke up in Terminal.app (see `ui/font.rs`).
    pub fn header_rows(self) -> u16 {
        match self {
            Tier::Full | Tier::Grid => 9,
            Tier::Compact => 5,
        }
    }

    /// Rows the gauge band claims. Compact is 8, not 10: those cards had two
    /// rows of slack, and the header needs them more now that it carries the
    /// wordmark.
    pub fn gauge_rows(self) -> u16 {
        match self {
            Tier::Grid => 24, // two 12-row bands
            Tier::Full => 12,
            Tier::Compact => 8,
        }
    }
}

/// The shape of everything below the gauge band.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Body {
    /// Processes on top, then three equal-width cards (localhost, claude
    /// sessions, others). Splitting the groups into their own cards is only
    /// worth the width when each can show a useful number of rows.
    ThreeCards,
    /// Processes stacked over the single grouped ports card, which carries all
    /// three groups at once.
    ProcessesOverPorts,
    /// Not even room for the ports card.
    ProcessesOnly,
}

/// A terminal size resolved into everything the renderer needs to know about it.
#[derive(Clone, Debug)]
pub struct Screen {
    pub tier: Tier,
    /// The gauge cards that fit, in display order. Never empty: CPU and Memory
    /// always survive.
    pub gauges: Vec<Gauge>,
    pub body: Body,
    /// Whether the network card gets a column beside the body.
    pub network: bool,
}

/// Column floors for the gauges that drop out as width gets tight, and the
/// width at which the three-card body earns its space.
const TEMP_COLS: u16 = 50;
const POWER_COLS: u16 = 70;
const THREE_CARDS_COLS: u16 = 120;

impl Screen {
    pub fn resolve(area: Rect) -> Self {
        // Gauges drop out in priority order as width gets tight; CPU and
        // Memory always survive.
        let mut gauges = vec![Gauge::Cpu];
        if area.width >= TEMP_COLS {
            gauges.push(Gauge::Temp);
        }
        if area.width >= POWER_COLS {
            gauges.push(Gauge::Power);
        }
        gauges.push(Gauge::Memory);

        let tier = if area.height >= 30 && area.width >= THREE_CARDS_COLS {
            Tier::Full
        } else if area.height >= 40 && gauges.len() == 4 {
            // The 40-row floor pays for the second band (header 9 + bands 24 +
            // body 6 + footer 1). Requiring all four gauges — rather than
            // re-deriving Power's column floor — is what guarantees the 2x2
            // grid never leaves a cell empty.
            Tier::Grid
        } else {
            Tier::Compact
        };

        let body = if area.width >= THREE_CARDS_COLS {
            Body::ThreeCards
        } else if area.height >= 20 {
            Body::ProcessesOverPorts
        } else {
            Body::ProcessesOnly
        };

        Screen { tier, gauges, body, network: area.height >= 16 }
    }
}

#[cfg(test)]
mod tests {
    use super::{Body, Gauge, Screen, Tier};
    use ratatui::layout::Rect;

    fn at(w: u16, h: u16) -> Screen {
        Screen::resolve(Rect::new(0, 0, w, h))
    }

    #[test]
    fn the_grid_tier_always_has_four_cards_to_place() {
        // The invariant that used to live only in a comment. Swept across the
        // whole plausible size space rather than at the two sizes that
        // motivated it — a half-empty second band is the failure this rules out.
        for w in 20u16..=250 {
            for h in [40u16, 45, 60, 100] {
                let s = at(w, h);
                if s.tier == Tier::Grid {
                    assert_eq!(s.gauges.len(), 4, "{w}x{h}: grid tier with a gap in the 2x2");
                }
            }
        }
    }

    #[test]
    fn gauges_drop_out_in_priority_order_and_never_lose_cpu_or_memory() {
        for w in 1u16..=250 {
            let g = at(w, 40).gauges;
            assert_eq!(g.first(), Some(&Gauge::Cpu), "{w}: CPU must always survive");
            assert_eq!(g.last(), Some(&Gauge::Memory), "{w}: Memory must always survive");
            // Power never appears without Temp: it has the wider floor.
            if g.contains(&Gauge::Power) {
                assert!(g.contains(&Gauge::Temp), "{w}: Power without Temp");
            }
        }
        assert_eq!(at(49, 40).gauges, vec![Gauge::Cpu, Gauge::Memory]);
        assert_eq!(at(50, 40).gauges, vec![Gauge::Cpu, Gauge::Temp, Gauge::Memory]);
        assert_eq!(
            at(70, 40).gauges,
            vec![Gauge::Cpu, Gauge::Temp, Gauge::Power, Gauge::Memory]
        );
    }

    #[test]
    fn tier_boundaries() {
        assert_eq!(at(120, 30).tier, Tier::Full);
        assert_eq!(at(119, 30).tier, Tier::Compact, "too narrow for full, too short for grid");
        assert_eq!(at(120, 29).tier, Tier::Compact);
        assert_eq!(at(103, 45).tier, Tier::Grid, "narrow but tall keeps the hero design");
        assert_eq!(at(103, 39).tier, Tier::Compact, "one row below the grid floor");
        assert_eq!(at(69, 45).tier, Tier::Compact, "one column below the grid floor");
    }

    #[test]
    fn the_three_card_body_and_the_full_tier_are_decided_independently() {
        // Both check the same column floor, but only the tier cares about
        // height — a wide, short terminal gets three cards under a compact
        // header, which is exactly what 120x24 should look like.
        let s = at(120, 24);
        assert_eq!(s.body, Body::ThreeCards);
        assert_eq!(s.tier, Tier::Compact);
    }

    #[test]
    fn the_body_sheds_ports_then_network_as_height_gets_tight() {
        assert_eq!(at(80, 24).body, Body::ProcessesOverPorts);
        assert!(at(80, 24).network);
        assert_eq!(at(80, 19).body, Body::ProcessesOnly, "ports goes first");
        assert!(at(80, 19).network, "network outlives ports");
        assert!(!at(80, 15).network, "network goes next");
    }

    #[test]
    fn every_size_resolves_without_panicking_or_producing_an_empty_gauge_band() {
        for w in 0u16..=200 {
            for h in 0u16..=60 {
                let s = at(w, h);
                assert!(!s.gauges.is_empty(), "{w}x{h}: no gauges at all");
                assert!(s.tier.header_rows() > 0 && s.tier.gauge_rows() > 0);
            }
        }
    }
}
