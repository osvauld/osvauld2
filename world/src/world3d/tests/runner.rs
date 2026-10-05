use super::super::*;
use super::specs;
use glam::{Quat, Vec3};
use runtime::{App, El, Headless, ScreenshotRequest};
use runtime::scene3d::{Camera3d, Object3d, Scene3d, SceneInspection, mesh::MeshData};
use std::cell::RefCell;

#[derive(Clone)]
enum Msg { Reset, Captured }

struct MarbleApp {
    world: World3d,
    authored: Arc<Scene3d>,
    rendered: RefCell<Option<(u64, SceneInspection)>>,
    active: bool,
    capture: Option<bool>,
}

impl MarbleApp {
    fn new() -> Self {
        let mut world = World3d::default();
        let bodies = specs();
        let objects = bodies.iter().map(|body| Object3d {
            id: body.id.as_str().into(), mesh: MeshData::cube(), position: body.position.into(),
            rotation: Quat::IDENTITY,
            scale: match body.shape { Shape3d::Box(size) => size.into(), Shape3d::Sphere(r) => Vec3::splat(r * 2.0) },
            color: [0.3, 0.6, 0.9, 1.0], surface: None,
        }).collect();
        world.reconcile(bodies).unwrap();
        let authored = Scene3d::new(Camera3d {
            eye: Vec3::new(8.0, 5.0, 8.0), target: Vec3::Y, up: Vec3::Y,
            fov_y_radians: 1.0, near: 0.1, far: 100.0,
        }, objects).unwrap();
        Self { world, authored, rendered: RefCell::new(None), active: true, capture: None }
    }
}

impl App for MarbleApp {
    type Msg = Msg;
    fn advance_simulation(&mut self, elapsed: f64) -> bool {
        let running = self.active && self.world.needs_ticks();
        self.world.advance(elapsed, running).unwrap();
        self.active && self.world.needs_ticks()
    }
    fn view(&self) -> El<Msg> {
        let scene = self.world.resolved_scene(&self.authored).unwrap();
        *self.rendered.borrow_mut() = Some((self.world.tick(), scene.inspect()));
        runtime::scene3d(scene).full().id("viewport").on_click(Msg::Reset)
    }
    fn update(&mut self, msg: Msg) {
        if let Msg::Reset = msg { self.world.reset("marble").unwrap(); }
    }
    fn take_screenshot(&mut self) -> Option<ScreenshotRequest<Msg>> {
        self.capture.take().map(|custom| ScreenshotRequest {
            viewport: custom.then_some((320.0, 240.0)), scale: None,
            complete: Box::new(|_| Msg::Captured),
        })
    }
}

#[test]
fn runner_drives_real_physics_before_rendering_and_reset_uses_normal_pointer_input() {
    let mut h = Headless::new(MarbleApp::new(), (640.0, 480.0));
    h.frame();
    assert_eq!(h.app().world.tick(), 0);
    for _ in 0..30 { h.frame(); }
    assert_eq!(h.app().world.tick(), 60);
    let body = h.app().world.body("marble").unwrap();
    let rendered = h.app().rendered.borrow();
    let (tick, scene) = rendered.as_ref().unwrap();
    assert_eq!(*tick, 60);
    assert_eq!(scene.objects[1].position, body.position);
    assert!(body.position[1] < 3.0);
    drop(rendered);
    for _ in 0..600 { h.frame(); }
    assert!(h.app().world.body("marble").unwrap().sleeping);
    let settled_tick = h.app().world.tick();
    let viewport = h.rects().into_iter().find(|r| r.id == "viewport").unwrap();
    h.click_at(viewport.x + viewport.w / 2.0, viewport.y + viewport.h / 2.0);
    assert_eq!(h.app().world.body("marble").unwrap().position, [0.0, 3.0, 0.0]);
    assert_eq!(h.app().world.tick(), settled_tick);
    h.frame(); // Resume establishes a fresh clock baseline.
    h.frame();
    assert!(h.app().world.body("marble").unwrap().position[1] < 3.0);
}

#[test]
fn runner_capture_requests_leave_real_physics_and_authored_scene_unchanged() {
    let mut h = Headless::new(MarbleApp::new(), (640.0, 480.0));
    for _ in 0..31 { h.frame(); }
    let before = h.app().world.inspect();
    let authored = h.app().authored.inspect();
    for custom in [false, true] {
        h.app_mut().capture = Some(custom);
        h.frame();
        assert_eq!(h.app().world.inspect(), before);
        assert_eq!(h.app().authored.inspect(), authored);
        assert_eq!(h.app().rendered.borrow().as_ref().unwrap().0, before.tick);
    }
}

#[test]
fn runner_pause_and_resume_do_not_catch_up_hidden_time() {
    let mut h = Headless::new(MarbleApp::new(), (640.0, 480.0));
    for _ in 0..31 { h.frame(); }
    let before = h.app().world.inspect();
    h.app_mut().active = false;
    h.advance(10.0);
    h.frame();
    assert_eq!(h.app().world.inspect(), before);
    h.advance(100.0);
    h.app_mut().active = true;
    h.frame();
    assert_eq!(h.app().world.inspect(), before);
    h.frame();
    assert_eq!(h.app().world.tick(), before.tick + 2);
    assert!(h.app().world.body("marble").unwrap().position[1] < before.entities[0].resolved.position[1]);
}
