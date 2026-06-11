// Probe: run lua::split over the real app fixtures and show the block-shape distribution.
use code_editor::lua::split;

fn main() {
    for path in std::env::args().skip(1) {
        let src = std::fs::read_to_string(&path).unwrap();
        let blocks = split(&src);
        println!("\n=== {} ({} lines, {} blocks)", path, src.lines().count(), blocks.len());
        for b in &blocks {
            let lines = b.text.lines().count().max(1);
            let first = b.text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
            let first: String = first.chars().take(60).collect();
            println!("  {:9} {:4} lines | {}", format!("{:?}", b.kind), lines, first);
        }
    }
}
