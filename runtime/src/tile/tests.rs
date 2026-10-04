use super::*;
use crate::{El, Headless, col, row, text, tile};
use std::sync::Arc;

/// The host: a 100-wide strip, then a 200×100 tile slot. It only records what it is told.
#[derive(Default)]
struct Host {
    got: Vec<TileInput>,
    wheels: usize,
}

#[derive(Clone)]
enum HostMsg {
    Tile(TileInput),
    Wheel,
}

impl App for Host {
    type Msg = HostMsg;
    fn view(&self) -> El<HostMsg> {
        row()
            .child(col().w(100.0).h(100.0))
            .child(
                tile(Arc::new(Scene::new()))
                    .size(200.0, 100.0)
                    .on_tile("t", HostMsg::Tile),
            )
            .on_wheel("host", |_| HostMsg::Wheel)
    }
    fn update(&mut self, msg: HostMsg) {
        match msg {
            HostMsg::Tile(i) => self.got.push(i),
            HostMsg::Wheel => self.wheels += 1,
        }
    }
}

/// The guest: a button at (40, 40)–(80, 70) in its own coordinates.
#[derive(Default)]
struct Guest {
    clicks: usize,
}

impl App for Guest {
    type Msg = ();
    fn view(&self) -> El<()> {
        col()
            .pad(40.0)
            .child(text("go").size(40.0, 30.0).id("go").on_click(()))
    }
    fn update(&mut self, _: ()) {
        self.clicks += 1;
    }
}

fn drain(host: &mut Headless<Host>) -> Vec<TileInput> {
    std::mem::take(&mut host.app_mut().got)
}

#[test]
fn what_crosses_is_send() {
    fn send<T: Send + 'static>() {}
    send::<Scene>();
    send::<TileInput>();
}

/// The whole loop by hand: the host's events, replayed into a tile, click the tile's button.
#[test]
fn a_click_in_the_slot_reaches_the_tile_app() {
    let mut host = Headless::new(Host::default(), (300.0, 100.0));
    let mut guest = Tile::new(Guest::default(), (200.0, 100.0));
    guest.frame();

    host.click_at(150.0, 50.0);
    let got = drain(&mut host);
    assert!(
        matches!(got.first(), Some(TileInput::Move(x, y)) if *x == 50.0 && *y == 50.0),
        "{got:?}"
    );
    for i in got {
        guest.input(i);
    }
    assert_eq!(guest.app().clicks, 1);
}

#[test]
fn outside_the_slot_nothing_is_forwarded() {
    let mut host = Headless::new(Host::default(), (300.0, 100.0));
    host.click_at(50.0, 50.0);
    assert!(drain(&mut host).is_empty());
}

/// A press inside holds the tile: moves past its edge are still its own, in its coordinates, and
/// the release reaches it. Only then does it hear that the pointer left.
#[test]
fn a_press_captures_until_release() {
    let mut host = Headless::new(Host::default(), (300.0, 100.0));
    host.move_to(150.0, 50.0);
    host.press();
    host.move_to(20.0, 50.0);
    let got = drain(&mut host);
    assert!(
        got.iter()
            .any(|i| matches!(i, TileInput::Move(x, _) if *x == -80.0)),
        "{got:?}"
    );
    assert!(!got.iter().any(|i| matches!(i, TileInput::Leave)), "{got:?}");
    host.release();
    let got = drain(&mut host);
    assert!(
        got.iter().any(|i| matches!(i, TileInput::Button(false))),
        "{got:?}"
    );
    host.move_to(10.0, 50.0);
    host.move_to(150.0, 50.0);
    host.move_to(10.0, 50.0);
    assert!(
        drain(&mut host)
            .iter()
            .any(|i| matches!(i, TileInput::Leave))
    );
}

/// A press elsewhere takes the keyboard back from the tile.
#[test]
fn a_press_outside_blurs_the_tile() {
    let mut host = Headless::new(Host::default(), (300.0, 100.0));
    host.click_at(150.0, 50.0);
    drain(&mut host);
    host.click_at(50.0, 50.0);
    assert!(
        drain(&mut host)
            .iter()
            .any(|i| matches!(i, TileInput::Blur))
    );
}

/// The wheel over a tile is the tile's: an ancestor's handler does not also see it.
#[test]
fn the_wheel_over_a_slot_is_the_tiles() {
    let mut host = Headless::new(Host::default(), (300.0, 100.0));
    host.wheel(150.0, 50.0, 0.0, -30.0);
    assert_eq!(host.app().wheels, 0);
    assert!(
        drain(&mut host)
            .iter()
            .any(|i| matches!(i, TileInput::Wheel(_, y) if *y == -30.0))
    );
    host.wheel(50.0, 50.0, 0.0, -30.0);
    assert_eq!(host.app().wheels, 1);
}

/// A tile asks for a frame only when something changed.
#[test]
fn a_tile_wants_a_frame_only_after_a_change() {
    let mut guest = Tile::new(Guest::default(), (200.0, 100.0));
    assert!(guest.wants_frame());
    guest.frame();
    assert!(!guest.wants_frame());
    guest.resize((200.0, 100.0));
    assert!(!guest.wants_frame());
    guest.resize((300.0, 100.0));
    assert!(guest.wants_frame());
    guest.frame();
    guest.update(());
    assert!(guest.wants_frame());
}

/// The point of all this, in pixels: a tile built and painted on another thread shows in the
/// host at its slot, clipped to it. The guest's viewport is wider than the slot on purpose.
///
/// Needs a GPU adapter, like the other pixel test: `cargo test -p runtime -- --ignored`.
#[test]
#[ignore = "needs a GPU adapter"]
fn a_tile_painted_on_another_thread_shows_in_its_slot() {
    use vello::peniko::Color;
    const GREEN: (u8, u8, u8) = (0x2a, 0xbf, 0x6d);

    struct Green;
    impl App for Green {
        type Msg = ();
        fn view(&self) -> El<()> {
            col()
        }
        fn update(&mut self, _: ()) {}
        fn clear(&self) -> Color {
            Color::from_rgb8(GREEN.0, GREEN.1, GREEN.2)
        }
    }
    struct Shows(Arc<Scene>);
    impl App for Shows {
        type Msg = ();
        fn view(&self) -> El<()> {
            row()
                .child(col().w(100.0).h(100.0))
                .child(tile(self.0.clone()).size(150.0, 100.0))
        }
        fn update(&mut self, _: ()) {}
        fn clear(&self) -> Color {
            Color::BLACK
        }
    }

    let guest = std::thread::spawn(|| Tile::new(Green, (300.0, 100.0)).frame())
        .join()
        .unwrap();
    let scene = Tile::new(Shows(Arc::new(guest)), (300.0, 100.0)).frame();
    let mut render = pollster::block_on(crate::Render::offscreen(300, 100));
    let shot = render
        .capture_scene(Color::BLACK, &scene, (300.0, 100.0), 1.0, None)
        .expect("capture");
    let mut png = png::Decoder::new(std::io::Cursor::new(&shot.png))
        .read_info()
        .unwrap();
    let mut buf = vec![0; png.output_buffer_size().unwrap()];
    let info = png.next_frame(&mut buf).unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * info.width + x) * 4) as usize;
        (buf[i], buf[i + 1], buf[i + 2])
    };
    assert_eq!(at(50, 50), (0, 0, 0), "host, left of the slot");
    assert_eq!(at(175, 50), GREEN, "inside the slot");
    assert_eq!(at(275, 50), (0, 0, 0), "past the slot: clipped");
}

/// A tile that holds the keyboard and then leaves the screen lets go of it, or every key would
/// go to something no longer there.
#[test]
fn a_vanished_tile_lets_go_of_the_keyboard() {
    struct Maybe(bool);
    impl App for Maybe {
        type Msg = bool;
        fn view(&self) -> El<bool> {
            let slot = tile(Arc::new(Scene::new()))
                .size(100.0, 100.0)
                .on_tile("t", |_| true);
            if self.0 { row().child(slot) } else { row() }
        }
        fn update(&mut self, _: bool) {}
    }
    let mut host = Tile::new(Maybe(true), (100.0, 100.0));
    host.frame();
    host.input(TileInput::Move(50.0, 50.0));
    host.input(TileInput::Button(true));
    host.input(TileInput::Button(false));
    assert!(host.runner.keyboard_tile().is_some());
    host.app_mut().0 = false;
    host.frame();
    assert_eq!(host.runner.keyboard_tile(), None);
}
