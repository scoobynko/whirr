//! Temporary font-risk gate for the braille burst fan (see
//! docs/superpowers/specs/2026-07-27-whirr-burst-fan-design.md §9).
//! Prints static braille sample rows so the real terminal font can be judged
//! before any of the fan is implemented. Deleted once the fan lands.

fn main() {
    println!("\nAll 8 dot weights (should be evenly spaced, no gaps between cells):");
    println!("⠁⠃⠇⡇⡏⡟⡿⣿⠀⠁⠃⠇⡇⡏⡟⡿⣿");

    println!("\nHairline diagonals (should read as continuous thin lines):");
    for row in [
        "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣶⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
        "⠀⠀⠀⠀⠀⠘⠦⠀⠀⠀⣭⠀⠀⠀⠴⠃⠀⠀⠀⠀⠀",
        "⠀⠀⠀⠀⠀⠀⠀⢳⡄⠀⣿⠀⢠⡞⠀⠀⠀⠀⠀⠀⠀",
        "⠀⠀⠘⠳⠀⢤⣄⣀⠹⠂⠛⠐⠏⣀⣠⡤⠀⠞⠃⠀⠀",
        "⠀⠀⠀⠀⠀⠀⠀⣉⡁⠀⠀⠀⢈⣉⠀⠀⠀⠀⠀⠀⠀",
        "⠀⠀⢠⡴⠀⠚⠋⠉⣰⠄⣤⠠⣆⠉⠙⠓⠀⢦⡄⠀⠀",
        "⠀⠀⠀⠀⠀⠀⠀⡼⠃⠀⣿⠀⠘⢧⠀⠀⠀⠀⠀⠀⠀",
        "⠀⠀⠀⠀⠀⢠⠖⠀⠀⠀⣛⠀⠀⠀⠲⡄⠀⠀⠀⠀⠀",
        "⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
    ] {
        println!("{row}");
    }

    println!("\nBraille beside the fallback stroke glyphs (─ ╲ │ ╱):");
    println!("⠳⣄  vs  ╲    ⣿  vs  │    ⠤⠤  vs  ──");

    println!("\nWidth check — these two rows must end at the same column:");
    println!("⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿|");
    println!("XXXXXXXXXXXXXXXXXXXXX|");
}
