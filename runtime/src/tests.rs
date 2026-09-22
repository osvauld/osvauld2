use super::*;
use crate::frame::{Brush, Frame, Item, Path};
use crate::scene3d::{BuiltinMesh, Camera3d, Object3d, Scene3d};
use glam::{Quat, Vec3};
use std::cell::RefCell;
use std::rc::Rc;
use vello::kurbo::PathEl;
use vello::peniko::Fill;

fn square(size: f64) -> Arc<Path> {
    Arc::new(
        Path::new(vec![
            PathEl::MoveTo((0.0, 0.0).into()),
            PathEl::LineTo((size, 0.0).into()),
            PathEl::LineTo((size, size).into()),
            PathEl::LineTo((0.0, size).into()),
            PathEl::ClosePath,
        ])
        .unwrap(),
    )
}

/// A visual is painted at its element's *content* origin, so its own coordinates start inside the
/// padding. Pointer events are element-local, from the border box — the difference is the padding,
/// and forgetting it would shift every hit by it.
#[test]
fn a_visuals_coordinates_start_inside_the_padding() {
    let brush = Arc::new(Brush::solid(vello::peniko::Color::WHITE).unwrap());
    let frame = Arc::new(
        Frame::new(
            10.0,
            10.0,
            None,
            vec![Item::fill(square(10.0), brush, Fill::NonZero).with_id("face")],
        )
        .unwrap(),
    );
    let shapes = Shapes {
        frame,
        origin: (6.0, 4.0),
    };

    let hit = shapes.at(NodePoint::new(7.0, 5.0)).unwrap();
    assert_eq!(&*hit.id, "face");
    assert_eq!(hit.local, Point::new(1.0, 1.0));
    assert_eq!(shapes.at(NodePoint::new(3.0, 3.0)), None); // still in the padding
}

/// A drag reports the shape it *grabbed*, in that shape's coordinates, and keeps doing so after
/// the pointer has left it — which is what holding something means. The transform comes back with
/// the hit precisely so a later point can be put in the same space.
#[test]
fn a_grab_keeps_reporting_in_the_shape_it_took_hold_of() {
    // A quarter turn: the shape's +x is the frame's +y, so nothing about these two spaces is
    // interchangeable by accident.
    let turn = vello::kurbo::Affine::new([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]);
    let brush = Arc::new(Brush::solid(vello::peniko::Color::WHITE).unwrap());
    let knob = Item::fill(square(10.0), brush, Fill::NonZero).with_id("knob");
    let frame = Frame::new(
        100.0,
        100.0,
        None,
        vec![Item::group(turn, vec![knob]).unwrap()],
    )
    .unwrap();
    let shapes = Some(Shapes {
        frame: Arc::new(frame),
        origin: (0.0, 0.0),
    });

    let grab = Grabbed::take(&shapes, NodePoint::new(-5.0, 5.0)).unwrap();
    assert_eq!(&*grab.id, "knob");

    let (id, at) = grab.at(NodePoint::new(-50.0, 5.0)); // dragged well outside the shape
    assert_eq!(&*id, "knob");
    assert_eq!(at, (5.0, 50.0));

    assert!(Grabbed::take(&shapes, NodePoint::new(50.0, 5.0)).is_none()); // pressed on nothing
}

/// A tiny app that records the order handlers fired in, so a gesture can be read back as a list.
struct Recorder {
    log: Rc<RefCell<Vec<String>>>,
}

impl App for Recorder {
    type Msg = String;
    fn view(&self) -> El<String> {
        crate::col().full().child(
            crate::col()
                .id("pad")
                .w(200.0)
                .h(200.0)
                .on_click_at(|at| format!("click {},{}", at.pos.0, at.pos.1))
                .on_wheel("pad", |w| format!("wheel {},{}", w.delta.0, w.delta.1))
                .on_drag("pad", |d| format!("drag {}", d.phase.as_str())),
        )
    }
    fn update(&mut self, msg: String) {
        self.log.borrow_mut().push(msg);
    }
}

fn gesture(run: impl FnOnce(&mut Headless<Recorder>)) -> Vec<String> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut h = Headless::new(Recorder { log: log.clone() }, (400.0, 400.0));
    run(&mut h);
    let out = log.borrow().clone();
    out
}

/// A press that travels past the slop is a drag and *only* a drag: the armed click is dropped the
/// moment the gesture becomes one. Reading the release path alone suggests otherwise, which is
/// exactly the mistake this test exists to keep from being made twice.
#[test]
fn a_drag_does_not_also_fire_the_click_it_started_from() {
    let fired = gesture(|h| h.drag((50.0, 50.0), (140.0, 120.0), 4));
    assert_eq!(fired.first().map(String::as_str), Some("drag start"));
    assert_eq!(fired.last().map(String::as_str), Some("drag end"));
    assert!(
        !fired.iter().any(|m| m.starts_with("click")),
        "a drag fired a click as well: {fired:?}"
    );
}

/// The other side of the same line: a press that never travels far enough stays a click, reported
/// where it was released. A hand is never perfectly still, so this is the common case, not an edge.
#[test]
fn a_press_that_barely_moves_is_still_a_click() {
    let fired = gesture(|h| h.drag((50.0, 50.0), (53.0, 52.0), 4));
    assert_eq!(fired, vec!["click 53,52".to_string()]);
    assert!(!fired.iter().any(|m| m.starts_with("drag")));
}

#[test]
fn the_topmost_wheel_handler_consumes_the_event() {
    let fired = gesture(|h| h.wheel(50.0, 50.0, 2.0, -12.0));
    assert_eq!(fired, ["wheel 2,-12"]);
}

struct Picker {
    scene: Arc<Scene3d>,
    selected: Option<String>,
}

impl App for Picker {
    type Msg = Option<String>;
    fn view(&self) -> El<Self::Msg> {
        crate::scene3d(self.scene.clone())
            .id("scene")
            .w(200.0)
            .h(200.0)
            .on_click_at(|at| at.object.map(|hit| hit.id.to_string()))
    }
    fn update(&mut self, msg: Self::Msg) {
        self.selected = msg;
    }
}

#[test]
fn scene_click_picks_the_visible_object_through_normal_routing() {
    let camera = Camera3d {
        eye: Vec3::new(0.0, 0.0, 5.0),
        target: Vec3::ZERO,
        up: Vec3::Y,
        fov_y_radians: 0.8,
        near: 0.1,
        far: 100.0,
    };
    let object = Object3d {
        id: "cube".into(),
        mesh: BuiltinMesh::Cube,
        position: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
        color: [1.0; 4],
        surface: None,
    };
    let scene = Scene3d::new(camera, vec![object]).unwrap();
    let mut headless = Headless::new(
        Picker {
            scene,
            selected: None,
        },
        (200.0, 200.0),
    );
    headless.click_at(100.0, 100.0);
    assert_eq!(headless.app().selected.as_deref(), Some("cube"));
}

/// A canvas whose one named shape slides 30pt right per frame, under a pointer that never moves.
struct Drifting {
    x: f64,
    seen: Vec<String>,
}

#[derive(Clone)]
enum Drift {
    Tick,
    Hover(HoverPhase, Option<String>),
}

impl App for Drifting {
    type Msg = Drift;
    fn view(&self) -> El<Drift> {
        let brush = Arc::new(Brush::solid(vello::peniko::Color::WHITE).unwrap());
        let cell = Item::fill(square(50.0), brush, Fill::NonZero).with_id("cell");
        let slid = Item::group(Affine::translate((self.x, 75.0)), vec![cell]).unwrap();
        crate::frame(Arc::new(
            Frame::new(200.0, 200.0, None, vec![slid]).unwrap(),
        ))
        .id("canvas")
        .on_frame("canvas", |_| Drift::Tick)
        .on_hover("canvas", |h| {
            // The offset inside the shape, not just its name — that is the half a shape-only
            // comparison drops, and it goes stale without ever looking wrong.
            Drift::Hover(
                h.phase,
                h.shape.map(|s| format!("{}@{}", &*s.id, s.local.x)),
            )
        })
    }
    fn update(&mut self, msg: Drift) {
        match msg {
            Drift::Tick => self.x += 30.0,
            Drift::Hover(phase, shape) => self.seen.push(format!(
                "{} {}",
                phase.as_str(),
                shape.as_deref().unwrap_or("-")
            )),
        }
    }
}

/// Voronoi's §3, as something that runs. The hover regions were always rebuilt by every paint, but
/// only a pointer event ever diffed them — so a shape could slide under a parked pointer and the
/// app would never hear about it. One pointer event here, and everything after it is the geometry
/// moving instead.
#[test]
fn a_shape_drifting_under_a_still_pointer_still_fires_hover() {
    let app = Drifting {
        x: 0.0,
        seen: Vec::new(),
    };
    let mut h = Headless::new(app, (400.0, 400.0));
    h.move_to(100.0, 100.0);
    for _ in 0..5 {
        h.frame();
    }
    // Two moves while on the same cell: it slid 30pt, so the offset the app was told changed even
    // though the name did not.
    assert_eq!(
        h.app().seen,
        ["enter -", "move cell@40", "move cell@10", "move -"]
    );
}

/// Records the monotonic time of every frame and every drag event, in the order they arrived.
struct Stamped {
    seen: Vec<(&'static str, f64)>,
}

#[derive(Clone)]
enum Stamp {
    Frame(f64),
    Drag(&'static str, f64),
}

impl App for Stamped {
    type Msg = Stamp;
    fn view(&self) -> El<Stamp> {
        crate::col().full().child(
            crate::col()
                .id("pad")
                .w(200.0)
                .h(200.0)
                .on_frame("pad", |f| Stamp::Frame(f.elapsed))
                .on_drag("pad", |d| Stamp::Drag(d.phase.as_str(), d.t)),
        )
    }
    fn update(&mut self, msg: Stamp) {
        match msg {
            Stamp::Frame(t) => self.seen.push(("frame", t)),
            Stamp::Drag(phase, t) => self.seen.push((phase, t)),
        }
    }
}

/// A drag is stamped when the pointer event arrives, not when the frame it lands in is drawn.
/// Frame time would hand every event processed in one frame the same number, and a fling computed
/// from two such samples divides by zero rather than looking wrong.
#[test]
fn a_drag_is_stamped_when_the_event_arrives_not_when_the_frame_draws() {
    let mut h = Headless::new(Stamped { seen: Vec::new() }, (400.0, 400.0));
    h.drag((50.0, 50.0), (150.0, 130.0), 4);
    let seen = &h.app().seen;

    assert!(
        seen.windows(2).all(|w| w[0].1 <= w[1].1),
        "time went backwards: {seen:?}"
    );

    // Each drag event is stamped *after* the frame before it. If the stamp were the frame's, the
    // two would be equal — so this is the assertion that tells the two designs apart.
    let (mut last_frame, mut drags) = (f64::NEG_INFINITY, 0);
    for (what, t) in seen {
        if *what == "frame" {
            last_frame = *t;
        } else {
            assert!(*t > last_frame, "{what} at {t} is not after {last_frame}");
            drags += 1;
        }
    }
    assert!(drags >= 3, "expected start, moves and an end: {seen:?}");

    // Offscreen the clock is virtual, so the spacing is exact and the same on every machine —
    // which is what makes a fling measurable here at all. `"start"` and the move it fires with it
    // share a stamp on purpose: one event, one time.
    let mut times: Vec<f64> = seen
        .iter()
        .filter(|(w, _)| *w != "frame")
        .map(|(_, t)| *t)
        .collect();
    times.dedup();
    let step = 1.0 / 60.0 + 0.008;
    for pair in times.windows(2) {
        let gap = pair[1] - pair[0];
        assert!(
            (gap - step).abs() < 1e-9,
            "events {gap}s apart, expected {step}s: {seen:?}"
        );
    }
}
