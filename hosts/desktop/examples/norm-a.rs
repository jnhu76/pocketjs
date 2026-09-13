//! CROSS-OS-NORMALIZED-DESKTOP-STARTUP-1 — arm A control: winit only.
//! No PocketJS runtime, no QuickJS, no wgpu. It isolates the window +
//! event-loop stage of the startup path on ONE monotonic origin with the
//! same E-series vocabulary as the full host, so Linux and Windows arm-A
//! numbers are directly comparable to arm-C stage completions.
//!
//! Endpoint semantics (documented in the report): `E199_ARM_ENDPOINT` is the
//! completion of the FIRST `RedrawRequested` handler — window shown and the
//! compositor having issued the first redraw. There is NO present; a
//! bufferless Wayland surface has no meaningful submit boundary.
//!
//! Run with NORMTRACE=1 (+ optional NORMTRACE_RUN=<id>); silent otherwise.
#[path = "../src/norm.rs"]
mod norm;

use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

struct App {
    window: Option<Arc<Window>>,
    redraws: u32,
    started: Instant,
}

impl ApplicationHandler<()> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        norm::once("E11_EVENT_LOOP_READY");
        norm::once("E20_WINDOW_CREATE_BEGIN");
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("norm-a")
                        .with_inner_size(LogicalSize::new(720u32, 480u32)),
                )
                .expect("create window"),
        );
        if let Some(scale) = norm::forced_scale() {
            let _resized = window.request_inner_size(winit::dpi::PhysicalSize::new(
                (720u32 as f64 * scale).round() as u32,
                (480u32 as f64 * scale).round() as u32,
            ));
        }
        norm::once("E21_WINDOW_CREATE_END");
        norm::once("E130_REQUEST_REDRAW");
        window.request_redraw();
        self.window = Some(window);
        let config = serde_json::json!({
            "arm": "A-winit-only",
            "target_id": target_id(),
            "logical_viewport": [720, 480],
            "force_scale": norm::forced_scale(),
            "gpu": "none (winit only)",
            "clock": "process-local Instant; origin E00_MAIN_ENTRY",
        });
        norm::emit_config(&config.to_string());
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Safety exit: never run longer than 3 s even if the compositor
        // never issues a redraw.
        if self.started.elapsed() > Duration::from_secs(3) {
            event_loop.exit();
            return;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(50)));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                norm::once("E131_REDRAW_CALLBACK");
                self.redraws += 1;
                if self.redraws == 1 {
                    norm::once("E199_ARM_ENDPOINT");
                }
                if self.redraws >= 5 {
                    event_loop.exit();
                } else if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn target_id() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows-app",
        "linux" => "linux-app",
        "macos" => "macos-app",
        _ => "unknown-app",
    }
}

fn main() {
    norm::init();
    norm::once("E00_MAIN_ENTRY");
    norm::once("E10_EVENT_LOOP_BEGIN");
    let event_loop = EventLoop::builder().build().expect("event loop");
    let mut app = App {
        window: None,
        redraws: 0,
        started: Instant::now(),
    };
    event_loop.run_app(&mut app).expect("run");
}
