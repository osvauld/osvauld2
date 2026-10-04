use super::*;

#[derive(Default)]
struct NativeApp {
    elapsed: Vec<f64>,
    views: RefCell<Vec<usize>>,
    capture: Option<bool>,
    completed: Vec<(u32, u32)>,
}

impl App for NativeApp {
    type Msg = (u32, u32);
    fn advance_simulation(&mut self, elapsed: f64) -> bool {
        self.elapsed.push(elapsed);
        true
    }
    fn view(&self) -> El<Self::Msg> {
        self.views.borrow_mut().push(self.elapsed.len());
        text("native").w(64.0).h(48.0)
    }
    fn update(&mut self, dimensions: Self::Msg) { self.completed.push(dimensions); }
    fn take_screenshot(&mut self) -> Option<ScreenshotRequest<Self::Msg>> {
        self.capture.take().map(|custom| ScreenshotRequest {
            viewport: custom.then_some((32.0, 24.0)), scale: None,
            complete: Box::new(|result| {
                let image = result.expect("real capture succeeds");
                (image.width, image.height)
            }),
        })
    }
}

#[test]
fn native_simulation_advances_once_before_each_normal_view() {
    let mut runner = Runner::new(NativeApp::default(), Some((64.0, 48.0)));
    runner.tick();
    runner.tick();
    assert_eq!(runner.app.elapsed, [0.0, FRAME]);
    assert_eq!(*runner.app.views.borrow(), [1, 2]);
}

#[test]
fn live_and_custom_capture_requests_skip_native_simulation_without_a_gpu() {
    let mut runner = Runner::new(NativeApp::default(), Some((64.0, 48.0)));
    runner.tick();
    for custom in [false, true] {
        runner.app.capture = Some(custom);
        runner.frame();
        assert_eq!(runner.app.elapsed.len(), 1);
        assert_eq!(runner.app.views.borrow().last(), Some(&1));
    }
    runner.tick();
    assert_eq!(runner.app.elapsed.len(), 2);
    assert_eq!(runner.app.views.borrow().last(), Some(&2));
}

#[test]
#[ignore = "needs a GPU adapter"]
fn capture_frames_skip_native_simulation_with_real_pixels() {
    let mut runner = Runner::new(NativeApp::default(), Some((64.0, 48.0)));
    runner.render = Some(pollster::block_on(Render::offscreen(64, 48)));
    runner.tick();
    for custom in [false, true] {
        runner.app.capture = Some(custom);
        runner.frame();
        assert_eq!(runner.app.elapsed.len(), 1);
        assert_eq!(runner.app.views.borrow().last(), Some(&1));
    }
    assert_eq!(runner.app.completed, [(64, 48), (32, 24)]);
    runner.tick();
    assert_eq!(runner.app.elapsed.len(), 2);
}
