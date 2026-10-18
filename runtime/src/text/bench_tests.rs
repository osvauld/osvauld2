//! Benchmarks with real amounts of text (`ui-reuse.md` §2a): paragraphs that wrap, a chat of
//! long messages, a table of short cells. Cold is what every frame cost before the shaping
//! cache; warm is a frame where nothing changed; edit is a warm frame where one cell did.
use super::*;
use crate::{App, El, Headless, col, row, text};

const SENTENCE: &str = "The quick brown fox jumps over the lazy dog while the chat keeps scrolling and nobody reads the thread. ";

fn bench(name: &str, runs: u32, mut f: impl FnMut()) {
    f();
    let started = std::time::Instant::now();
    for _ in 0..runs {
        f();
    }
    let us = started.elapsed().as_secs_f64() * 1e6 / f64::from(runs);
    let debug = if cfg!(debug_assertions) { " debug" } else { "" };
    println!("BENCH {name} {us:.1}{debug}");
}

/// 60 paragraphs of ~400 characters, measured and painted at 760 wide, as a frame does.
#[test]
#[ignore = "benchmark: scripts/bench.py"]
fn bench_text_paragraphs() {
    let paragraphs: Vec<String> = (0..60).map(|i| format!("{i}. {}", SENTENCE.repeat(4))).collect();
    let mut e = TextEngine::new();
    let mut scene = Scene::new();
    let mut frame = |e: &mut TextEngine| {
        scene.reset();
        for p in &paragraphs {
            e.measure(p, UI_FAMILY, 15.0, Some(760.0));
            e.draw(&mut scene, p, UI_FAMILY, 15.0, Affine::IDENTITY, Color::BLACK, Some(760.0));
        }
        e.sweep();
    };
    bench("text_60_paragraphs_warm", 50, || frame(&mut e));
    bench("text_60_paragraphs_cold", 10, || {
        e.sweep();
        e.sweep();
        frame(&mut e);
    });
}

/// A page of 100 wrapping chat messages and a 50 × 8 table of short single-line cells, none
/// of it scrolled or culled — the worst case an unvirtualized screen can be.
struct Page {
    messages: Vec<String>,
    edit: usize,
}

impl App for Page {
    type Msg = ();
    fn view(&self) -> El<()> {
        let chat = col().w(560.0).gap(8.0).children(self.messages.iter().map(|m| text(m.clone())));
        let table = col().gap(2.0).children((0..50).map(|r| {
            row().gap(12.0).children((0..8).map(move |c| {
                let cell = if (r, c) == (0, 0) { format!("edited {}", self.edit) } else { format!("row {r} col {c}") };
                text(cell).no_wrap().w(90.0)
            }))
        }));
        row().full().gap(20.0).child(chat).child(table)
    }
    fn update(&mut self, _: ()) {}
}

#[test]
#[ignore = "benchmark: scripts/bench.py"]
fn bench_text_page() {
    let messages = (0..100).map(|i| format!("message {i}: {}", SENTENCE.repeat(2))).collect();
    let mut driver = Headless::new(Page { messages, edit: 0 }, (1280.0, 800.0));
    bench("text_page_warm_frame", 50, || driver.frame());
    bench("text_page_edit_frame", 50, || {
        driver.app_mut().edit += 1;
        driver.frame();
    });
}
