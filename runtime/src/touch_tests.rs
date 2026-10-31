use super::*;
use std::cell::RefCell;

struct TouchApp(Rc<RefCell<Vec<TouchInput>>>);
impl App for TouchApp {
    type Msg = TouchInput;
    fn view(&self) -> El<TouchInput> {
        col().full().child(col().w(100.0).h(100.0).on_touch("pad", |e| e))
            .child(col().absolute().left(120.0).top(0.0).w(100.0).h(100.0)
                .on_click(TouchInput { id: 99, phase: TouchPhase::Ended, pos: (0.0, 0.0) }))
    }
    fn update(&mut self, msg: TouchInput) { self.0.borrow_mut().push(msg); }
}

#[test]
fn ordinary_buttons_work_while_a_different_finger_holds_a_touch_surface() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut r = Runner::new(TouchApp(log.clone()), Some((300.0, 200.0)));
    r.frame();
    r.on_touch(TouchPhase::Started, 1, PhysicalPosition::new(20.0, 20.0));
    r.on_touch(TouchPhase::Started, 2, PhysicalPosition::new(150.0, 20.0));
    r.on_touch(TouchPhase::Ended, 2, PhysicalPosition::new(150.0, 20.0));
    assert_eq!(log.borrow().len(), 2, "ordinary button was not clickable with another finger held");
    assert_eq!(log.borrow()[1].id, 99);
    r.on_touch(TouchPhase::Moved, 1, PhysicalPosition::new(40.0, 20.0));
    assert_eq!(log.borrow().last().unwrap().phase, TouchPhase::Moved);
    assert_eq!(r.fingers.len(), 1, "ordinary release cancelled an unrelated finger");
}

#[test]
fn fingers_capture_independently_and_cancel_on_suspend() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut r = Runner::new(TouchApp(log.clone()), Some((300.0, 200.0)));
    r.frame();
    let at = PhysicalPosition::new(40.0, 50.0);
    r.on_touch(TouchPhase::Started, u64::MAX, at);
    r.on_touch(TouchPhase::Started, 2, at);
    r.on_touch(TouchPhase::Moved, u64::MAX, PhysicalPosition::new(200.0, 150.0));
    r.on_touch(TouchPhase::Ended, 2, at);
    r.suspend();
    let got = log.borrow();
    assert_eq!(got.len(), 5);
    assert_eq!(got[2].pos, (200.0, 150.0));
    assert_eq!(got[3].id, 2);
    assert_eq!(got[4].id, u64::MAX);
    assert_eq!(got[4].phase, TouchPhase::Cancelled);
    assert_eq!(got[4].pos, (200.0, 150.0));
}

#[test]
fn touch_driver_validates_phase_coordinates_and_reports_reachability() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut r = Runner::new(TouchApp(log.clone()), Some((300.0, 200.0)));
    assert!(r.run_driver(DriverOp::Touch { id: 1, phase: "oops".into(), at: (20.0, 20.0) }).is_err());
    assert!(r.run_driver(DriverOp::Touch { id: 1, phase: "start".into(), at: (f32::NAN, 0.0) }).is_err());
    let rects = r.run_driver(DriverOp::Rects).unwrap().rects.unwrap();
    assert!(rects.iter().any(|r| r.id == "pad" && r.hits.contains(&"touch")));
    r.run_driver(DriverOp::Touch { id: 1, phase: "start".into(), at: (20.0, 20.0) }).unwrap();
    r.run_driver(DriverOp::Touch { id: 1, phase: "end".into(), at: (220.0, 20.0) }).unwrap();
    assert_eq!(log.borrow().len(), 2);
    assert_eq!(log.borrow()[1].pos, (220.0, 20.0));
}

#[test]
fn excess_and_duplicate_fingers_do_not_become_pointer_clicks() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut r = Runner::new(TouchApp(log.clone()), Some((300.0, 200.0)));
    r.frame();
    for id in 0..20 { r.on_touch(TouchPhase::Started, id, PhysicalPosition::new(20.0, 20.0)); }
    r.on_touch(TouchPhase::Started, 0, PhysicalPosition::new(20.0, 20.0));
    assert_eq!(log.borrow().len(), 16);
    assert!(r.touch_id.is_none());
    r.clear_touch();
    assert_eq!(log.borrow().len(), 32);
}

#[test]
fn later_touch_surface_wins_over_a_tile_and_parent() {
    struct Layered(Rc<RefCell<Vec<TouchInput>>>);
    impl App for Layered {
        type Msg = TouchInput;
        fn view(&self) -> El<TouchInput> {
            col().full().on_touch("parent", |e| TouchInput { id: 10, ..e })
                .child(crate::tile(TileFrame::default()).size(100.0, 100.0)
                    .on_tile("tile", |_| panic!("covered tile took touch")))
                .child(col().absolute().left(0.0).top(0.0).w(100.0).h(100.0)
                    .on_touch("overlay", |e| e))
        }
        fn update(&mut self, msg: TouchInput) { self.0.borrow_mut().push(msg); }
    }
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut r = Runner::new(Layered(log.clone()), Some((300.0, 200.0)));
    r.frame();
    r.on_touch(TouchPhase::Started, 1, PhysicalPosition::new(20.0, 20.0));
    assert_eq!(log.borrow()[0].id, 1);
}

#[test]
fn native_rect_inspection_is_allowed_but_clock_drivers_remain_offscreen_only() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut r = Runner::new(TouchApp(log.clone()), Some((300.0, 200.0)));
    r.frame();
    r.offscreen = None;
    r.virtual_clock = false;
    r.start = Instant::now() - std::time::Duration::from_secs(5);
    let clock = r.clock;
    let rects = r.run_driver(DriverOp::Rects).expect("native touch validation needs geometry");
    assert!(rects.clock >= 5.0, "native sensing reported a stale virtual clock");
    assert!(rects.rects.unwrap().iter().any(|r| r.id == "pad"));
    assert_eq!(r.clock, clock);
    assert!(r.run_driver(DriverOp::Frame(1)).is_err());
    assert!(log.borrow().is_empty());
}

#[test]
fn ordinary_buttons_occlude_touch_surfaces_parents_and_tiles() {
    struct Covered(u8, Rc<RefCell<Vec<TouchInput>>>);
    impl App for Covered {
        type Msg = TouchInput;
        fn view(&self) -> El<Self::Msg> {
            let button = col().w(50.0).h(50.0).on_click(TouchInput {
                id: 99, phase: TouchPhase::Ended, pos: (0.0, 0.0),
            });
            if self.0 == 1 {
                col().full().on_touch("parent", |e| e).child(button)
            } else {
                let base = if self.0 == 0 { col().w(100.0).h(100.0).on_touch("base", |e| e) }
                    else { crate::tile(TileFrame::default()).size(100.0, 100.0)
                        .on_tile("base", |_| panic!("covered tile received input")) };
                col().full().child(base).child(button.absolute().left(0.0).top(0.0))
            }
        }
        fn update(&mut self, e: Self::Msg) { self.1.borrow_mut().push(e); }
    }
    for mode in 0..3 {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut r = Runner::new(Covered(mode, log.clone()), Some((200.0, 200.0)));
        r.frame();
        r.on_touch(TouchPhase::Started, 1, PhysicalPosition::new(20.0, 20.0));
        r.on_touch(TouchPhase::Ended, 1, PhysicalPosition::new(20.0, 20.0));
        assert_eq!(log.borrow().iter().map(|e| e.id).collect::<Vec<_>>(), vec![99], "mode {mode}");
    }
}
