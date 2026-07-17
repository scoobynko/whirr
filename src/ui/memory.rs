use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::sampler::PressureLevel;
use crate::units::fmt_bytes;
use super::{font, theme};

pub fn segment_widths(parts: &[u64], width: u16) -> Vec<u16> {
    let total: u64 = parts.iter().sum();
    if total == 0 {
        return vec![0; parts.len()];
    }
    let mut widths: Vec<u16> = parts
        .iter()
        .map(|&p| {
            if p == 0 { 0 } else { ((p as f64 / total as f64) * width as f64).round().max(1.0) as u16 }
        })
        .collect();
    // reconcile rounding drift against the largest segment
    let mut diff = widths.iter().sum::<u16>() as i32 - width as i32;
    while diff != 0 {
        let i = widths
            .iter()
            .enumerate()
            .max_by_key(|(_, &w)| w)
            .map(|(i, _)| i)
            .unwrap();
        if diff > 0 { widths[i] -= 1; diff -= 1; } else { widths[i] += 1; diff += 1; }
    }
    widths
}

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("Memory", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(mem) = app.medium.as_ref().and_then(|m| m.memory.as_ref()) else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };

    let state = match mem.pressure {
        PressureLevel::Normal => "NORMAL",
        PressureLevel::Warn => "WARN",
        PressureLevel::Critical => "CRITICAL",
    };
    let pcolor = theme::pressure_color(mem.pressure);

    let parts = [mem.app, mem.wired, mem.compressed, mem.free];
    let colors = [theme::ACCENT, theme::gradient(0.6), theme::AMBER, theme::BG_CELL];
    let labels = ["app", "wired", "compressed", "free"];
    let widths = segment_widths(&parts, inner.width.saturating_sub(3));

    let mut bar = Vec::new();
    for (i, (&w, &color)) in widths.iter().zip(colors.iter()).enumerate() {
        if w > 0 {
            bar.push(Span::styled("█".repeat(w as usize), Style::default().fg(color)));
            if i < widths.len() - 1 {
                bar.push(Span::raw(" "));
            }
        }
    }

    let legend = Line::from(
        labels
            .iter()
            .zip(colors.iter())
            .zip(parts.iter())
            .flat_map(|((l, &c), &p)| {
                vec![
                    Span::styled("■", Style::default().fg(c)),
                    Span::styled(format!(" {l} {} ", fmt_bytes(p)), Style::default().fg(theme::DIM)),
                ]
            })
            .collect::<Vec<_>>(),
    );

    let swap = format!("swap {} / {}", fmt_bytes(mem.swap_used), fmt_bytes(mem.swap_total));

    if font::hero_fits(inner) {
        let used = mem.app + mem.wired + mem.compressed;
        let used_gib = used as f64 / 1_073_741_824.0;
        let hero: Vec<Line> = font::big_text(&format!("{used_gib:.1}G"))
            .into_iter()
            .map(|r| Line::styled(r, Style::default().fg(pcolor)))
            .collect();
        let mut lines = hero;
        lines.push(Line::from(""));
        lines.push(Line::from(bar));
        lines.push(legend);
        lines.push(Line::from(vec![
            Span::styled("pressure ", Style::default().fg(theme::DIM)),
            Span::styled(state, Style::default().fg(pcolor).bold()),
            Span::styled(format!(" · {swap}"), Style::default().fg(theme::DIM)),
        ]));
        f.render_widget(Paragraph::new(lines), inner);
    } else {
        let lines = vec![
            Line::from(vec![
                Span::styled("pressure ", Style::default().fg(theme::DIM)),
                Span::styled(state, Style::default().fg(pcolor).bold()),
            ]),
            Line::from(bar),
            legend,
            Line::styled(swap, Style::default().fg(theme::DIM)),
        ];
        f.render_widget(Paragraph::new(lines), inner);
    }
}

#[cfg(test)]
mod tests {
    use super::segment_widths;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw(w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn hero_shows_used_gib_when_room() {
        // demo used = 4G + 2G + 1G = 7_000_000_000 B = 6.5 GiB → "6.5G"
        let full = draw(40, 12);
        assert!(full.contains("█▄▄ "), "4-row '6' glyph missing"); // '6' row 1
        assert!(full.contains("pressure NORMAL · swap"), "consolidated info line missing");
        let compact = draw(40, 10);
        assert!(!compact.contains("█▄▄ "));
        assert!(compact.contains("pressure "));
    }

    #[test]
    fn widths_sum_and_respect_minimums() {
        let w = segment_widths(&[50, 30, 15, 5], 40);
        assert_eq!(w.iter().sum::<u16>(), 40);
        assert!(w.iter().all(|&x| x >= 1));
        assert!(w[0] > w[3]);
    }

    #[test]
    fn zero_parts_get_zero() {
        let w = segment_widths(&[100, 0, 100], 20);
        assert_eq!(w[1], 0);
        assert_eq!(w.iter().sum::<u16>(), 20);
    }
}
