use ratatui::prelude::*;
use ratatui::widgets::Sparkline;

/// Filled block sparkline (`▁▂▃▄▅▆▇█`). `data` is oldest→newest; only the most
/// recent `area.width` samples are drawn so the newest lands at the right edge.
/// ratatui renders the first `min(width, len)` bars left-to-right and drops the
/// rest, so the raw oldest-first history must be tail-sliced here.
pub fn render(f: &mut Frame, area: Rect, data: &[u64], max: u64, style: Style) {
    let w = area.width as usize;
    let tail = if data.len() > w { &data[data.len() - w..] } else { data };
    let spark = Sparkline::default().data(tail).max(max.max(1)).style(style);
    f.render_widget(spark, area);
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
}
