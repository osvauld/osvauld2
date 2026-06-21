//! shell2 — the from-scratch render runtime (winit + wgpu + vello + parley), replacing the egui
//! `sthalam` shell. This file is the entry point: the winit event loop and window lifecycle. The
//! GPU/paint plumbing lives in `render`, content in `screen`, text in `text`, colors in `theme`.

mod render;
mod screen;
mod text;
mod theme;

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use render::Render;
use screen::{LoginScreen, Redraw};

#[derive(Default)]
struct App {
    render: Option<Render>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.render.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("osvauld");
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let render = pollster::block_on(Render::new(window, Box::new(LoginScreen::new())));
        render.request_redraw(); // paint the first frame; after that we only repaint on demand
        self.render = Some(render);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(render) = self.render.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                render.set_scale(scale_factor);
                render.request_redraw();
            }
            WindowEvent::Resized(size) => {
                render.resize(size);
                render.request_redraw();
            }
            // Retained: only re-request a frame while the screen is animating. A settled screen
            // returns Idle, the loop sleeps (ControlFlow::Wait), and idle CPU drops to ~zero.
            WindowEvent::RedrawRequested => {
                if render.render() == Redraw::Animating {
                    render.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("run app");
}
