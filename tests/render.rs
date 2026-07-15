use ratatui::backend::TestBackend;
use ratatui::Terminal;
use whirr::app::App;
use whirr::ui;

fn draw_at(w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    let app = App::demo();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    buf.content().iter().map(|c| c.symbol()).collect()
}

#[test]
fn renders_at_all_sizes_without_panic() {
    for (w, h) in [(200, 50), (120, 40), (80, 24), (60, 15), (20, 5)] {
        let content = draw_at(w, h);
        assert!(!content.is_empty(), "{w}x{h}");
    }
}

#[test]
fn full_size_shows_all_panels() {
    let c = draw_at(160, 45);
    for needle in ["CPU", "Temp", "Power", "Memory", "Processes", "Network", "Ports"] {
        assert!(c.contains(needle), "missing {needle}");
    }
    assert!(c.contains("(my-app)"), "port project badge missing");
}

#[test]
fn tiny_size_collapses_to_essentials() {
    let c = draw_at(48, 14);
    assert!(c.contains("Processes"));
    assert!(!c.contains("Ports"));
}
