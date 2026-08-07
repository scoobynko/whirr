use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::units::{fmt_bytes, fmt_rate};
use super::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = app.theme.panel_block("Network", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(fast) = app.fast.as_ref() else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(app.theme.dim)), inner);
        return;
    };

    // The rate line always claims its row first; the sparkline bands only
    // get whatever's left, so the numbers a user actually reads are never
    // the part that gets sacrificed to the traces.
    let rate_area = Rect { height: inner.height.min(1), ..inner };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("▼ {}", fmt_rate(fast.net_rx_rate)), Style::default().fg(app.theme.accent)),
            Span::styled(
                format!("  ▲ {}", fmt_rate(fast.net_tx_rate)),
                Style::default().fg(app.theme.gradient(0.55)),
            ),
            Span::styled(
                format!("  ∑ ▼{} ▲{}", fmt_bytes(fast.net_rx_total), fmt_bytes(fast.net_tx_total)),
                Style::default().fg(app.theme.dim),
            ),
        ])),
        rate_area,
    );

    let bands_height = inner.height.saturating_sub(rate_area.height);
    if bands_height == 0 {
        return;
    }
    let bands_area = Rect { y: rate_area.y + rate_area.height, height: bands_height, ..inner };

    // Shared peak so the two bands' heights are directly comparable.
    let peak = app
        .net_hist
        .iter()
        .map(|(rx, tx)| rx.max(tx))
        .fold(1024.0, f64::max) // ≥1 KB/s so an idle machine doesn't render noise
        * 1.2;
    let max = peak as u64;
    let down: Vec<u64> = app.net_hist.iter().map(|(rx, _)| rx as u64).collect();

    if bands_height == 1 {
        // Only one row left: show download alone — the more informative of
        // the pair — rather than squeezing both bands into it.
        render_band(f, bands_area, &app.theme, "▼", &down, max, app.theme.accent);
        return;
    }

    let up: Vec<u64> = app.net_hist.iter().map(|(_, tx)| tx as u64).collect();
    let bands = Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(bands_area);
    render_band(f, bands[0], &app.theme, "▼", &down, max, app.theme.accent);
    render_band(f, bands[1], &app.theme, "▲", &up, max, app.theme.gradient(0.55));
}

/// One labeled sparkline band: a 2-col dim marker gutter then the sparkline.
/// The marker sits on the band's baseline (bottom row) so it labels the bars,
/// which grow up from the bottom, rather than floating above empty space.
fn render_band(
    f: &mut Frame,
    area: Rect,
    theme: &theme::Theme,
    marker: &str,
    data: &[u64],
    max: u64,
    color: Color,
) {
    let cols = Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).split(area);
    let gutter = cols[0];
    let marker_area = Rect { y: gutter.y + gutter.height.saturating_sub(1), height: 1, ..gutter };
    f.render_widget(
        Paragraph::new(marker).style(Style::default().fg(theme.dim)),
        marker_area,
    );
    super::spark::render(f, cols[1], theme, data, max, Style::default().fg(color));
}

#[cfg(test)]
mod tests {
    /// The palette the app starts with; tests assert against it by name
    /// rather than against raw RGB triples.
    const TH: crate::ui::theme::Theme = crate::ui::theme::Theme::dark();

    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

    use crate::app::App;


    fn draw() -> Vec<String> {
        draw_at_inner_height(6)
    }

    /// Renders the card with a terminal sized so the block's inner area is
    /// exactly `inner_height` rows tall (the block's rounded border eats 2).
    fn draw_at_inner_height(inner_height: u16) -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(40, inner_height + 2)).unwrap();
        let mut app = App::demo();
        // download clearly larger than upload so the two bands differ.
        for i in 0..30 {
            app.net_hist.push((i as f64 * 4000.0, i as f64 * 800.0));
        }
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let buf = t.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>()
            })
            .collect()
    }

    #[test]
    fn renders_two_labeled_sparkline_bands() {
        let lines = draw();
        let joined = lines.join("\n");
        assert!(
            joined.chars().any(|c| "▁▂▃▄▅▆▇█".contains(c)),
            "no sparkline bars rendered"
        );
        // ▼/▲ each appear on the header (rate labels) plus their own band's top
        // row; the band rows are the lower occurrences. Download sits above
        // upload, in a separate row.
        let last_row = |m: char| lines.iter().rposition(|l| l.contains(m)).unwrap();
        let down_row = last_row('▼');
        let up_row = last_row('▲');
        assert_ne!(down_row, up_row, "download and upload must occupy separate rows");
        assert!(down_row < up_row, "download band should sit above upload band");
    }

    /// The rate readout ("▼ 1.1 MB/s  ▲ ...") must survive at every inner
    /// height, however few rows are left for the sparkline bands.
    #[test]
    fn rate_line_survives_at_every_inner_height() {
        for inner_height in [1, 2, 3, 6] {
            let joined = draw_at_inner_height(inner_height).join("\n");
            assert!(
                joined.contains("/s"),
                "rate readout missing at inner height {inner_height}"
            );
        }
    }

    #[test]
    fn inner_height_1_renders_no_bands() {
        let joined = draw_at_inner_height(1).join("\n");
        assert!(
            !joined.chars().any(|c| "▁▂▃▄▅▆▇█".contains(c)),
            "no room left for bands at inner height 1, but a sparkline bar rendered"
        );
    }

    #[test]
    fn inner_height_2_renders_download_band_only() {
        let lines = draw_at_inner_height(2);
        let joined = lines.join("\n");
        assert!(
            joined.chars().any(|c| "▁▂▃▄▅▆▇█".contains(c)),
            "download band should render when exactly one row remains"
        );
        // '▼' appears on the rate line plus one band row; '▲' only on the
        // rate line, since the upload band has no room to render.
        let down_rows = lines.iter().filter(|l| l.contains('▼')).count();
        let up_rows = lines.iter().filter(|l| l.contains('▲')).count();
        assert_eq!(down_rows, 2, "expected rate line + single download band row, got {down_rows}");
        assert_eq!(up_rows, 1, "upload band should not render when only one row remains, got {up_rows} rows with '▲'");
    }

    /// The two bands are drawn in different caller colours (`ACCENT` and
    /// `gradient(0.55)`); each band's ramp must stay within its own hue
    /// rather than both collapsing onto the same hardcoded teal.
    #[test]
    fn both_network_bands_ramp_within_their_own_hue() {
        let mut t = Terminal::new(TestBackend::new(10, 2)).unwrap();
        t.draw(|f| {
            let area = f.area();
            let down = Rect { height: 1, ..area };
            let up = Rect { y: area.y + 1, height: 1, ..area };
            super::render_band(f, down, &TH, "▼", &[8], 8, TH.accent);
            super::render_band(f, up, &TH, "▲", &[8], 8, TH.gradient(0.55));
        })
        .unwrap();
        let buf = t.backend().buffer().clone();
        // A single sample right-anchors to the last column (spark::render's
        // tail-slicing places the newest/only sample at the right edge). Both
        // samples are at max, so each bar is a completely full cell — those
        // carry their colour as a background fill rather than a foreground
        // block glyph, to avoid Terminal.app's seams (see `ui/spark.rs`).
        let down_bar = buf[(9, 0)].bg;
        let up_bar = buf[(9, 1)].bg;
        assert_eq!(down_bar, TH.accent, "download band's full-height bar should be its own accent colour");
        assert_eq!(up_bar, TH.gradient(0.55), "upload band's full-height bar should be its own gradient colour, not accent");
        assert_ne!(down_bar, up_bar, "the two bands must not collapse to the same colour");
    }

    #[test]
    fn inner_height_3_renders_both_bands() {
        let lines = draw_at_inner_height(3);
        let last_row = |m: char| lines.iter().rposition(|l| l.contains(m)).unwrap();
        let down_row = last_row('▼');
        let up_row = last_row('▲');
        assert_ne!(down_row, up_row, "both bands should render once two rows remain");
        assert!(down_row < up_row);
    }
}
