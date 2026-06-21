//! Render a Lua app with the *workspace* engine build (no live shell involved):
//! `cargo run -p app_engine --example shot -- examples/standup.lua /tmp/standup.png [900x700] [x,y | x,y-x2,y2]`
//! The optional click (or press-move-release drag) runs first so interactive states show.

fn main() {
    let mut args = std::env::args().skip(1);
    let usage = "usage: shot <app.lua> <out.png> [WxH] [click_x,click_y]";
    let src = args.next().expect(usage);
    let out = args.next().expect(usage);
    let size = args.next().unwrap_or_else(|| "900x700".into());
    let (w, h) = size.split_once('x').expect("size like 900x700");
    let (w, h): (f32, f32) = (w.parse().unwrap(), h.parse().unwrap());

    let fonts = app_engine::FontBytes {
        regular: include_bytes!("../../sthalam/assets/fonts/NotoSans-Regular.ttf"),
        bold: include_bytes!("../../sthalam/assets/fonts/NotoSans-SemiBold.ttf"),
        mono: include_bytes!("../../sthalam/assets/fonts/JetBrainsMono-Regular.ttf"),
        fallback: &[include_bytes!("../../sthalam/assets/fonts/NotoSansMalayalam-Regular.ttf")],
    };
    let mut app = app_engine::EngineApp::from_file(&src);

    if let Some(c) = args.next() {
        let point = |s: &str| {
            let (x, y) = s.split_once(',').expect("point like 200,140");
            egui::pos2(x.parse().unwrap(), y.parse().unwrap())
        };
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h));
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let input = |events| egui::RawInput { screen_rect: Some(rect), events, ..Default::default() };
        app.frame(input(vec![]), 2.0); // lay out so the pointer has something to hit
        match c.split_once('-') {
            Some((from, to)) => {
                let (from, to) = (point(from), point(to));
                app.frame(input(vec![button(from, true)]), 2.0);
                app.frame(input(vec![egui::Event::PointerMoved(to)]), 2.0);
                app.frame(input(vec![button(to, false)]), 2.0);
                app.frame(input(vec![]), 2.0); // settle: the post-drag walk applies the new sizes
            }
            None => {
                let pos = point(&c);
                app.frame(input(vec![button(pos, true), button(pos, false)]), 2.0);
            }
        }
    }

    let png = app
        .screenshot(w, h, 2.0, egui::Color32::from_rgb(0x0A, 0x0B, 0x10), fonts)
        .expect("render");
    std::fs::write(&out, png).expect("write png");
    println!("wrote {out}");
}
