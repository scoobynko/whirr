use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use super::{font, theme};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("Power", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(m) = app.medium.as_ref() else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };

    let hero = font::hero_fits(inner);
    let rows = Layout::vertical([
        Constraint::Length(if hero { 4 } else { 1 }), // hero
        Constraint::Length(1),                         // cpu/gpu/ane legend
        Constraint::Min(2),                            // total sparkline
        Constraint::Length(1),                         // battery footer
    ])
    .split(inner);

    match &m.power {
        Some(p) => {
            let total = p.cpu_w + p.gpu_w + p.ane_w;
            let text = format!("{total:.1} W");
            if hero {
                let coarse = format!("{total:.0} W");
                let lines: Vec<Line> = font::big_text_fit(&text, &coarse, inner.width)
                    .into_iter()
                    .map(|r| Line::styled(r, Style::default().fg(theme::ACCENT)))
                    .collect();
                f.render_widget(Paragraph::new(lines), rows[0]);
            } else {
                f.render_widget(
                    Paragraph::new(Span::styled(text, Style::default().fg(theme::ACCENT).bold())),
                    rows[0],
                );
            }
            let legend = format!("cpu {:.1} · gpu {:.1} · ane {:.1}", p.cpu_w, p.gpu_w, p.ane_w);
            f.render_widget(
                Paragraph::new(legend).style(Style::default().fg(theme::DIM)),
                rows[1],
            );
            render_spark(f, rows[2], app);
        }
        None => {
            f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), rows[0]);
        }
    }

    let battery_line = match &m.battery {
        Some(b) => {
            let state = if b.charging { "⚡" } else { "🔋" };
            let health = b.health_pct.map_or(String::new(), |h| format!(" · health {h}%"));
            format!("{state} {}% · {} cycles{health}", b.percent, b.cycles)
        }
        None => String::new(), // desktop Mac: hide line
    };
    f.render_widget(
        Paragraph::new(battery_line).style(Style::default().fg(theme::DIM)),
        rows[3],
    );
}

/// Filled block sparkline of total watts (cpu + gpu + ane).
fn render_spark(f: &mut Frame, area: Rect, app: &App) {
    let data: Vec<u64> = app
        .power_hist
        .iter()
        .map(|(c, g, a)| ((c + g + a) * 10.0).round() as u64)
        .collect();
    if data.is_empty() {
        return;
    }
    let peak = app.power_hist.iter().map(|(c, g, a)| c + g + a).fold(1.0, f64::max) * 1.2;
    let max = (peak * 10.0).round() as u64;
    super::spark::render(f, area, &data, max, Style::default().fg(theme::ACCENT));
}

#[cfg(test)]
mod tests {
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
    fn hero_when_room_compact_when_small() {
        // demo power total = 6.4 + 1.2 + 0.3 = 7.9 → "7.9 W"
        let full = draw(40, 12); // inner 38x10 → hero
        assert!(full.contains("▀▀▀█"), "4-row '7' glyph missing"); // '7' row 0
        assert!(full.contains("cpu 6.4"), "power legend (cpu/gpu/ane) missing");
        let compact = draw(40, 10); // inner 38x8 → compact
        assert!(compact.contains("7.9 W"));
        assert!(!compact.contains("▀▀▀█"));
    }

    #[test]
    fn history_renders_block_sparkline() {
        let mut t = Terminal::new(TestBackend::new(40, 12)).unwrap();
        let mut app = App::demo();
        for v in [(2.0_f64, 0.5, 0.1), (5.0, 1.0, 0.2), (8.0, 2.0, 0.3)] {
            app.power_hist.push(v);
        }
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let s: String = t.backend().buffer().content().iter().map(|c| c.symbol()).collect();
        assert!(s.contains('█') || s.contains('▇'), "power history should render filled block bars");
    }
}

