use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::units::{fmt_bytes, fmt_rate};
use super::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("Network", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(fast) = app.fast.as_ref() else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(2)]).split(inner);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("▼ {}", fmt_rate(fast.net_rx_rate)), Style::default().fg(theme::ACCENT)),
            Span::styled(
                format!("  ▲ {}", fmt_rate(fast.net_tx_rate)),
                Style::default().fg(theme::gradient(0.55)),
            ),
            Span::styled(
                format!("  ∑ ▼{} ▲{}", fmt_bytes(fast.net_rx_total), fmt_bytes(fast.net_tx_total)),
                Style::default().fg(theme::DIM),
            ),
        ])),
        rows[0],
    );

    // Shared peak so the two bands' heights are directly comparable.
    let peak = app
        .net_hist
        .iter()
        .map(|(rx, tx)| rx.max(tx))
        .fold(1024.0, f64::max) // ≥1 KB/s so an idle machine doesn't render noise
        * 1.2;
    let max = peak as u64;
    let down: Vec<u64> = app.net_hist.iter().map(|(rx, _)| rx as u64).collect();
    let up: Vec<u64> = app.net_hist.iter().map(|(_, tx)| tx as u64).collect();

    let bands = Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(rows[1]);
    render_band(f, bands[0], "▼", &down, max, theme::ACCENT);
    render_band(f, bands[1], "▲", &up, max, theme::gradient(0.55));
}

/// One labeled sparkline band: a 2-col dim marker gutter then the sparkline.
/// The marker sits on the band's baseline (bottom row) so it labels the bars,
/// which grow up from the bottom, rather than floating above empty space.
fn render_band(f: &mut Frame, area: Rect, marker: &str, data: &[u64], max: u64, color: Color) {
    let cols = Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).split(area);
    let gutter = cols[0];
    let marker_area = Rect { y: gutter.y + gutter.height.saturating_sub(1), height: 1, ..gutter };
    f.render_widget(
        Paragraph::new(marker).style(Style::default().fg(theme::DIM)),
        marker_area,
    );
    super::spark::render(f, cols[1], data, max, Style::default().fg(color));
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw() -> Vec<String> {
        let mut t = Terminal::new(TestBackend::new(40, 8)).unwrap();
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
}
