use ratatui::prelude::*;
use ratatui::widgets::Sparkline;

/// Filled block sparkline (`▁▂▃▄▅▆▇█`). `data` is oldest→newest; only the most
/// recent `area.width` samples are drawn so the newest lands at the right edge.
/// ratatui renders the first `min(width, len)` bars left-to-right and drops the
/// rest, so the raw oldest-first history must be tail-sliced here.
///
/// When there's less history than the area is wide, the short slice is drawn
/// into a right-anchored sub-rect instead of ratatui's default left-aligned
/// placement — the newest sample must stay at the right edge, and the empty
/// left space must stay genuinely blank rather than fake zero-value bars.
pub fn render(f: &mut Frame, area: Rect, data: &[u64], max: u64, style: Style) {
    let max = max.max(1);
    let w = area.width as usize;
    let tail = if data.len() > w { &data[data.len() - w..] } else { data };
    // Clamp so a pathological value can't overflow ratatui's internal
    // `value * height * 8 / max` computation.
    let clamped: Vec<u64> = tail.iter().map(|&v| v.min(max)).collect();
    let target = if clamped.len() < w {
        Rect { x: area.x + (w - clamped.len()) as u16, width: clamped.len() as u16, ..area }
    } else {
        area
    };
    let spark = Sparkline::default().data(&clamped).max(max).style(style);
    f.render_widget(spark, target);
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::prelude::*;
    use ratatui::Terminal;

    fn draw(data: &[u64], max: u64, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| super::render(f, f.area(), data, max, Style::default()))
            .unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn renders_expected_block_levels() {
        // values 0..=8 scaled against max 8 fill one row as " ▁▂▃▄▅▆▇█"
        let s = draw(&[0, 1, 2, 3, 4, 5, 6, 7, 8], 8, 9, 1);
        assert_eq!(s, " ▁▂▃▄▅▆▇█");
    }

    #[test]
    fn shows_only_the_most_recent_width_samples() {
        // 100 ascending samples into a width-10 chart show the last 10
        // (samples 91..=100), newest at the right edge.
        let data: Vec<u64> = (1..=100).collect();
        let s = draw(&data, 100, 10, 1);
        assert_eq!(s.chars().last().unwrap(), '█', "newest (100) should be full at right");
        // sample 91/100 → 7/8 height → '▇'; the first 10 samples (1..=10) would
        // be near-empty, so a filled left cell proves we tail-sliced.
        assert!("▅▆▇█".contains(s.chars().next().unwrap()), "left cell should be the recent tail, not the oldest samples");
    }

    #[test]
    fn short_data_right_anchors_instead_of_left_aligning() {
        // 3 samples into a 10-wide chart: bars must sit flush against the
        // RIGHT edge (newest at the far right), and the left 7 cells must
        // stay genuinely blank — not zero-value bars.
        let s = draw(&[2, 5, 8], 10, 10, 1);
        assert_eq!(s, "       ▁▄▆", "bars must hug the right edge with blank space on the left");
    }

    #[test]
    fn data_exactly_equal_to_width_fills_the_whole_area() {
        let s = draw(&[1, 2, 3, 4, 5], 5, 5, 1);
        assert_eq!(s, "▁▃▄▆█", "no blank padding when data fills the area exactly");
    }

    #[test]
    fn values_at_or_above_max_are_clamped_not_overflowed() {
        // A pathological value far above max must not panic or wrap; it
        // should render as a full bar, same as a value exactly at max.
        let s = draw(&[u64::MAX, 5], 10, 2, 1);
        assert_eq!(s.chars().next().unwrap(), '█', "value above max should clamp to full bar");
    }
}
