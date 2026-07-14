fn glyph(c: char) -> [&'static str; 3] {
    match c {
        '0' => ["█▀█", "█ █", "█▄█"],
        '1' => ["▄█ ", " █ ", "▄█▄"],
        '2' => ["▀▀█", "█▀▀", "█▄▄"],
        '3' => ["▀▀█", " ▀█", "▄▄█"],
        '4' => ["█ █", "▀▀█", "  █"],
        '5' => ["█▀▀", "▀▀█", "▄▄█"],
        '6' => ["█▀▀", "█▀█", "█▄█"],
        '7' => ["▀▀█", "  █", "  █"],
        '8' => ["█▀█", "█▀█", "█▄█"],
        '9' => ["█▀█", "▀▀█", "▄▄█"],
        '.' => ["   ", "   ", " ▄ "],
        'W' => ["█ █ █", "█ █ █", "▀▄▀▄▀"],
        '°' => ["▀▀ ", "   ", "   "],
        'C' => ["█▀▀", "█  ", "█▄▄"],
        ' ' => [" ", " ", " "],
        _ => ["?", "?", "?"],
    }
}

pub fn big_text(s: &str) -> Vec<String> {
    let mut rows = vec![String::new(); 3];
    for c in s.chars() {
        let g = glyph(c);
        for (i, row) in rows.iter_mut().enumerate() {
            row.push_str(g[i]);
            row.push(' ');
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    #[test]
    fn three_uniform_rows() {
        let rows = super::big_text("42.0 W");
        assert_eq!(rows.len(), 3);
        let w = rows[0].chars().count();
        assert!(rows.iter().all(|r| r.chars().count() == w));
    }
}
