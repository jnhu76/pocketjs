//! CROSS-OS-NORMALIZED-DESKTOP-STARTUP-1 — arm B control: winit + the
//! normalized wgpu policy. No PocketJS runtime, no QuickJS, no UiSurface
//! guest. It isolates the GPU pipeline (instance → surface → adapter →
//! device → caps → configure → acquire → clear/encode → submit → present)
//! with the EXACT normalized policy the full host runs, on ONE monotonic
//! origin with the same E-series vocabulary.
//!
//! Normalized policy (identical to the full host's normalized arm):
//! backend VULKAN, power preference LowPower, memory hints MemoryUsage,
//! present mode Fifo, desired_maximum_frame_latency 1, Bgra8Unorm-preferred
//! format policy, alpha Auto, empty features, default limits, no fallback.
//!
//! Endpoint semantics: `E190_FIRST_USABLE_PRESENT_SUBMITTED` is the return
//! of the first successful `SurfaceTexture::present()` (present SUBMITTED —
//! a named CPU-side proxy, not display photon latency).
//!
//! Run with NORMTRACE=1 (+ optional NORMTRACE_RUN=<id>); silent otherwise.
#[path = "../src/norm.rs"]
mod norm;

use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

struct App {
    window: Option<Arc<Window>>,
    started: Instant,
    // GPU state after `resumed`.
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    gpu: Option<(wgpu::Device, wgpu::Queue)>,
    presents: u32,
}

fn normalized_backends() -> wgpu::Backends {
    wgpu::Backends::VULKAN
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
                        .with_title("norm-b")
                        .with_inner_size(LogicalSize::new(720u32, 480u32)),
                )
                .expect("create window"),
        );
        if let Some(scale) = norm::forced_scale() {
            let _resized = window.request_inner_size(PhysicalSize::new(
                (720u32 as f64 * scale).round() as u32,
                (480u32 as f64 * scale).round() as u32,
            ));
        }
        norm::once("E21_WINDOW_CREATE_END");

        norm::once("E30_GPU_INSTANCE_BEGIN");
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: normalized_backends(),
            ..Default::default()
        });
        norm::once("E31_GPU_INSTANCE_END");

        norm::once("E40_SURFACE_CREATE_BEGIN");
        let surface = instance
            .create_surface(window.clone())
            .expect("surface");
        norm::once("E41_SURFACE_CREATE_END");

        norm::once("E50_ADAPTER_BEGIN");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no adapter");
        norm::once("E51_ADAPTER_END");

        norm::once("E60_DEVICE_BEGIN");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("norm-b"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
        }))
        .expect("no device");
        norm::once("E61_DEVICE_END");

        norm::once("E70_SURFACE_CAPS_BEGIN");
        let caps = surface.get_capabilities(&adapter);
        norm::once("E71_SURFACE_CAPS_END");
        let format = [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Rgba8Unorm,
        ]
        .into_iter()
        .find(|format| caps.formats.contains(format))
        .expect("no portable format");
        let size: PhysicalSize<u32> = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        norm::once("E72_SURFACE_CONFIG_BEGIN");
        surface.configure(&device, &config);
        norm::once("E73_SURFACE_CONFIG_END");

        let info = adapter.get_info();
        norm::once("E130_REQUEST_REDRAW");
        window.request_redraw();
        self.window = Some(window);
        self.surface = Some(surface);
        self.config = Some(config);
        self.gpu = Some((device, queue));
        let bench_config = serde_json::json!({
            "arm": "B-winit-normalized-wgpu",
            "target_id": target_id(),
            "logical_viewport": [720, 480],
            "force_scale": norm::forced_scale(),
            "backend": "Vulkan",
            "adapter": info.name,
            "adapter_backend": format!("{:?}", info.backend),
            "device_type": format!("{:?}", info.device_type),
            "power_preference": "LowPower",
            "memory_hints": "MemoryUsage",
            "present_mode": "Fifo",
            "desired_maximum_frame_latency": 1,
            "surface_format": format!("{format:?}"),
            "alpha_mode": "Auto",
            "force_fallback_adapter": false,
            "clock": "process-local Instant; origin E00_MAIN_ENTRY",
        });
        norm::emit_config(&bench_config.to_string());
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.presents >= 3 || self.started.elapsed() > Duration::from_secs(5) {
            event_loop.exit();
            return;
        }
        if self.window.is_some() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(50),
            ));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                norm::once("E131_REDRAW_CALLBACK");
                self.do_present();
            }
            _ => {}
        }
    }
}

impl App {
    fn do_present(&mut self) {
        let (Some(surface), Some(window), Some(config), Some((device, queue))) = (
            self.surface.as_ref(),
            self.window.as_ref(),
            self.config.as_mut(),
            self.gpu.as_ref(),
        ) else {
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        if (config.width, config.height) != (size.width, size.height) {
            config.width = size.width;
            config.height = size.height;
            surface.configure(device, config);
        }
        norm::once("E140_GET_TEXTURE_BEGIN");
        let Ok(output) = surface.get_current_texture() else {
            window.request_redraw();
            return;
        };
        norm::once("E141_GET_TEXTURE_END");
        let view = output.texture.create_view(&Default::default());
        norm::once("E160_PRESENT_ENCODE_BEGIN");
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("norm-b") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("norm-b"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            drop(pass);
        }
        norm::once("E161_PRESENT_ENCODE_END");
        queue.submit([encoder.finish()]);
        norm::once("E162_PRESENT_QUEUE_SUBMIT");
        window.pre_present_notify();
        norm::once("E170_PRE_PRESENT_NOTIFY");
        norm::once("E180_PRESENT_BEGIN");
        output.present();
        norm::once("E181_PRESENT_RETURN");
        self.presents += 1;
        if self.presents == 1 {
            norm::once("E190_FIRST_USABLE_PRESENT_SUBMITTED");
        }
        if self.presents >= 3 {
            // Endpoint reached; let the loop exit on the next about_to_wait.
        } else {
            window.request_redraw();
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
        started: Instant::now(),
        surface: None,
        config: None,
        gpu: None,
        presents: 0,
    };
    event_loop.run_app(&mut app).expect("run");
}
