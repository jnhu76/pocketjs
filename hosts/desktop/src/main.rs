//! Native platform adapter: window, input, clipboard and GPU presentation.
//! Guests, layout, composition and GPU recording belong to the runtime worker.
//! Text capabilities run in separately budgeted io.offload workers. No platform
//! text system participates in layout, shaping or drawing.
use anyhow::{Context as _, Result, anyhow};
use pocket_mod::Guest;
use pocket_ui_surface::{UiSurface, offload::OffloadWorker};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::mpsc::{Receiver, SyncSender, sync_channel},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize, PhysicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{CursorIcon, Window, WindowId},
};
mod gpu;
mod memprobe;
mod net;
include!("plan.rs");
include!("supervisor.rs");
include!("buttons.rs");
#[cfg(feature = "bench-harness")]
include!("a2.rs");
include!("a3.rs");

/// A7: monotonic milliseconds since process start, for startup phase
/// attribution (BENCHMARK §5 monotonic clock).
fn proc_ms() -> u128 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis()
}

/// Measurement stderr channels (A7EVENT startup phases, A6EVENT DPI/raster
/// transitions): silent unless the run opted into measurement plumbing with
/// `--announce-ready` — the flag that also arms the READY/IMGREADY markers,
/// so every run that consumes these lines already passes it. Product
/// operation prints none of them.
static MEASUREMENT_TRACING: AtomicBool = AtomicBool::new(false);

/// A7: one stderr line per startup phase (attribution evidence).
fn phase(name: &str) {
    if MEASUREMENT_TRACING.load(Ordering::Relaxed) {
        eprintln!("A7EVENT,phase,{name},{}ms", proc_ms());
    }
}

fn text_worker(pak: Vec<u8>) -> OffloadWorker {
    OffloadWorker::spawn(move || {
        let mut engine = pocket_text::Engine::new();
        engine.load_pak(&pak);
        move |record: &str| engine.reply(record)
    })
}

/// A6: the raster density the runtime actually renders with. The plan
/// density is authoritative until a scale transition is driven (by the
/// OS on a real monitor DPI move, or by the harness), after which the
/// window scale governs so the raster tracks physical client pixels —
/// present stays 1:1 and nothing is bitmap-stretched.
fn effective_density(plan_density: u32, scale: Option<f64>) -> u32 {
    match scale {
        None => plan_density,
        Some(s) => (s.round() as u32).clamp(1, 4),
    }
}

/// A6: logical viewport from a physical client-size report — the exact
/// conversion the winit Resized path has always applied, factored out so
/// the scale-transition tests can pin logical stability.
fn logical_from_physical(w: u32, h: u32, scale: f64) -> (u32, u32) {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    (
        (w as f64 / scale).round().clamp(240.0, 4096.0) as u32,
        (h as f64 / scale).round().clamp(180.0, 4096.0) as u32,
    )
}

enum Input {
    Service(Value),
    Pointer(Value),
    Resize(u32, u32),
    /// A6: a monitor-DPI/scale transition (real or harness-driven). The
    /// logical viewport is the invariant; the raster density follows.
    Scale(f64),
    Button(u32, bool),
    Reset,
    Quit,
}
// Reservation covers queued GPU work as well as the output channel.
struct OutputPermit(Arc<AtomicBool>);
impl OutputPermit {
    fn acquire(available: &Arc<AtomicBool>) -> Option<Self> {
        available
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(available.clone()))
    }
}
impl Drop for OutputPermit {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
struct Output {
    _permit: OutputPermit,
    tick: u64,
    target: Option<Arc<gpu::Target>>,
    intents: Vec<Value>,
}
#[derive(Debug)]
enum Wake {
    Output,
    Exit(Option<String>),
}

struct Runtime {
    args: Args,
    surface: UiSurface,
    guest: Guest,
    supervisor: AppSupervisor,
    offload: OffloadWorker,
    viewport: (u32, u32),
    /// A6: driven window scale; None until the first transition.
    scale: Option<f64>,
    ticks: u64,
    buttons: u32,
    #[cfg(feature = "bench-harness")]
    script: Vec<ScriptEvent>,
    #[cfg(feature = "bench-harness")]
    script_buttons: u32,
    #[cfg(feature = "bench-harness")]
    script_mouse: bool,
    click_edge: bool,
    mouse_down: bool,
    wire: Option<net::SvcWire>,
    #[cfg(feature = "bench-harness")]
    a2: A2Harness,
    a3: A3Harness,
    /// A7: successes already reported as `imgready` intents.
    seen_successes: u64,
}
impl Runtime {
    fn boot(args: Args) -> Result<Self> {
        if args.native_text {
            return Err(anyhow!(
                "text.layout.native is unavailable; use the portable text offload capability"
            ));
        }
        let pak = std::fs::read(resolve_asset(args.pak.clone(), &args.app, "pak")?)?;
        let source = std::fs::read_to_string(resolve_asset(args.js.clone(), &args.app, "js")?)?;
        memprobe::stage("boot_assets_read");
        let surface = UiSurface::new_with_density(
            (args.viewport.0 as f32, args.viewport.1 as f32),
            args.density,
        );
        memprobe::stage("boot_ui_surface");
        surface.set_identity(HOST_ID, HOST_ABI);
        surface.set_tick_rate(60);
        surface.set_svc_allowlist(args.companions.clone());
        surface.feed_pak(&pak);
        let supervisor = AppSupervisor::new(args.system.as_ref(), &surface)?;
        memprobe::stage("boot_supervisor");
        let guest = Guest::new()?;
        memprobe::stage("boot_quickjs");
        surface.mount(&guest)?;
        let offload = text_worker(pak);
        offload.mount(&guest)?;
        guest.eval(&args.app, &source)?;
        memprobe::stage("boot_guest_eval");
        if !guest.has_frame() {
            return Err(anyhow!("bundle installed no frame handler"));
        }
        surface.svc_push(
            json!({"t":"hello","w":args.viewport.0,"h":args.viewport.1,"epoch":epoch_ms()})
                .to_string(),
        );
        if let Some(file) = &args.file
            && let Ok(text) = std::fs::read_to_string(file)
        {
            surface.svc_push(json!({"t":"load","text":text}).to_string());
        }
        let wire = args
            .svc_connect
            .clone()
            .map(|addr| net::SvcWire::spawn(addr, args.app.clone()));
        #[cfg(feature = "bench-harness")]
        let a2_harness = args.a2_harness;
        let a3_harness_active = args.a3_harness;
        let a3_files = args.a3_files.clone();
        let mut runtime = Self {
            viewport: args.viewport,
            scale: None,
            #[cfg(feature = "bench-harness")]
            script: args.script.clone(),
            args,
            surface,
            guest,
            supervisor,
            offload,
            ticks: 0,
            buttons: 0,
            #[cfg(feature = "bench-harness")]
            script_buttons: 0,
            #[cfg(feature = "bench-harness")]
            script_mouse: false,
            click_edge: false,
            mouse_down: false,
            wire,
            #[cfg(feature = "bench-harness")]
            a2: A2Harness::new(a2_harness),
            a3: A3Harness::new(a3_harness_active, a3_files),
            seen_successes: 0,
        };
        runtime.a3.boot(&runtime.surface);
        Ok(runtime)
    }
    fn svc(&self, event: Value) {
        self.surface.svc_push(event.to_string());
    }
    /// C3: every source that can change guest-visible state without an
    /// input arriving must be quiet before the worker parks on the input
    /// channel (a blocking `recv` — the only wait that carries a wakeup).
    /// Conservative by construction: the guest's static-frames declaration
    /// is required, native animations forbid parking, and harness
    /// schedules, pending text-offload replies, child instances, a network
    /// svc wire, and the probe quit path all keep the 60 Hz loop.
    /// (`--trace-frames` is allowed to park: it is measurement-only, and
    /// C4's input→present decomposition needs wake traces on the parked
    /// path; A5-style cadence runs are still gated by `--quit-after` and
    /// their script schedule.)
    fn can_suspend(&self) -> bool {
        if !self.surface.guest_static() || self.surface.animating() {
            return false;
        }
        // Review BLOCKER fix: the A3 (and any native) handler pushes
        // service replies AFTER the guest's frame of the same tick; the
        // guest consumes svc lines only by polling in its next frame. A
        // non-empty inbound queue therefore means the guest-visible state
        // can still change without an input — parking here would strand
        // the reply (a multi-file walk stalls with an undelivered image).
        if self.surface.svc_guest_pending() {
            return false;
        }
        if self.offload.outstanding() > 0 || self.wire.is_some() {
            return false;
        }
        if !self.supervisor.instances.is_empty() {
            return false;
        }
        if self.args.quit_after_ticks.is_some() {
            return false;
        }
        #[cfg(feature = "bench-harness")]
        {
            if !self.script.is_empty() || self.args.storm.is_some() {
                return false;
            }
            if !self.a3.queued.is_empty() {
                return false;
            }
            if self.a2.active && self.a2.done < A2_SCHEDULE.len() {
                return false;
            }
        }
        true
    }
    fn input(&mut self, input: Input) -> Result<bool> {
        match input {
            Input::Quit => return Ok(false),
            Input::Reset => {
                self.buttons = 0;
                for child in &mut self.supervisor.instances {
                    child.buttons = 0;
                }
                self.svc(json!({"t":"mouse","d":false}));
            }
            Input::Service(v) | Input::Pointer(v) => {
                if self.args.editor && v["t"] == "mouse" {
                    if v["b"] == 2 {
                        return Ok(true);
                    }
                    if let Some(down) = v["d"].as_bool() {
                        self.click_edge |= down && !self.mouse_down;
                        self.mouse_down = down;
                    }
                }
                self.svc(v);
            }
            Input::Button(bit, down) => {
                if !self.supervisor.set_focused_button(bit, down) {
                    if down {
                        self.buttons |= bit;
                    } else {
                        self.buttons &= !bit;
                    }
                }
            }
            Input::Resize(w, h) if !self.args.fixed => {
                self.viewport = (w, h);
                self.surface
                    .with_ui(|ui| ui.set_viewport(w as f32, h as f32));
                self.guest.eval(
                    "resize",
                    &format!("globalThis.__pocketResizeViewport?.({w},{h})"),
                )?;
                self.svc(json!({"t":"resize","w":w,"h":h}));
            }
            Input::Scale(s) => {
                // A6: monitor truth changed. The logical viewport and the
                // guest's composition state are untouched; the raster
                // density follows the scale so present stays 1:1 with
                // physical client pixels (no OS bitmap stretch).
                self.scale = Some(s);
                let density = effective_density(self.args.density, Some(s));
                if MEASUREMENT_TRACING.load(Ordering::Relaxed) {
                    eprintln!(
                        "A6EVENT,raster,density={density},scale={s},logical={}x{}",
                        self.viewport.0, self.viewport.1
                    );
                }
                self.svc(
                    json!({"t":"resize","w":self.viewport.0,"h":self.viewport.1,
                           "dpi":s*96.0,"density":density}),
                );
            }
            _ => {}
        }
        Ok(true)
    }
    fn tick(&mut self) -> Result<Vec<Value>> {
        if let Some(wire) = self.wire.as_mut() {
            for line in wire.drain() {
                self.surface.svc_push(line);
            }
        }
        #[cfg(feature = "bench-harness")]
        self.a2.tick(self.ticks, &self.surface);
        #[cfg(feature = "bench-harness")]
        self.run_script();
        #[cfg(feature = "bench-harness")]
        if let Some((cps, start, dur)) = self.args.storm
            && self.ticks >= start
            && self.ticks < start + dur
        {
            let i = self.ticks - start;
            let n = ((i + 1) * cps as u64) / 60 - (i * cps as u64) / 60;
            if n > 0 {
                self.svc(json!({"t":"ch","s":"x".repeat(n.min(512) as usize)}));
            }
        }
        self.offload.begin_frame();
        #[cfg(feature = "bench-harness")]
        let buttons = if self.args.editor {
            if self.mouse_down || self.script_mouse || self.click_edge {
                BTN_CIRCLE
            } else {
                0
            }
        } else {
            self.buttons | self.script_buttons
        };
        #[cfg(not(feature = "bench-harness"))]
        let buttons = if self.args.editor {
            if self.mouse_down || self.click_edge {
                BTN_CIRCLE
            } else {
                0
            }
        } else {
            self.buttons
        };
        self.guest.frame(buttons)?;
        self.click_edge = false;
        self.surface.tick();
        for (id, error) in self
            .supervisor
            .sync(&self.surface)
            .into_iter()
            .chain(self.supervisor.tick())
        {
            log::error!("AppInstance {id}: {error}");
        }
        let mut intents = Vec::new();
        for line in self.surface.svc_drain() {
            #[cfg(feature = "bench-harness")]
            if line.starts_with("{\"t\":\"a2") {
                // A2 boundary traffic: counted and logged, never an intent.
                self.a2.observe_rx(&line);
                continue;
            }
            if line.starts_with("{\"t\":\"a3") || line.starts_with("{\"t\":\"a4") {
                // A3/A4 file intents, acks and geometry reports: handled by
                // the WIC harness, never an app intent.
                self.a3.observe_rx(&self.surface, &line, self.ticks);
                continue;
            }
            if let Some(wire) = &self.wire {
                wire.send(line);
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if v["t"] == "save" {
                    if let (Some(file), Some(text)) = (&self.args.file, v["text"].as_str()) {
                        let tmp = file.with_extension("tmp");
                        std::fs::write(&tmp, text)?;
                        std::fs::rename(tmp, file)?;
                    }
                } else {
                    intents.push(v);
                }
            }
        }
        // A5 coalescing drain: exactly one decode per tick, newest
        // requested generation only, superseded requests cancelled
        // before any decode stage. Must run after the full drain so a
        // burst queued in this tick collapses before work starts.
        self.a3.process_pending(&self.surface, self.ticks);
        // A7: one `imgready` intent per newly bound image — the host turns
        // the first corresponding present submission into the IMGREADY
        // marker (T6 present-submitted, first useful image proxy).
        if self.a3.successes > self.seen_successes {
            self.seen_successes = self.a3.successes;
            memprobe::stage("image_bound");
            intents.push(json!({"t": "imgready"}));
        }
        self.ticks += 1;
        Ok(intents)
    }
    fn hash(&mut self) -> u64 {
        self.surface
            .with_ui(|ui| fnv1a64(&ui.draw().words) ^ ui.raster_revision().rotate_left(7))
            ^ self.supervisor.visible_hash().rotate_left(17)
            ^ ((self.viewport.0 as u64) << 32 | self.viewport.1 as u64)
    }
    #[cfg(feature = "bench-harness")]
    fn run_script(&mut self) {
        let tick = self.ticks;
        for ev in self.script.clone() {
            match ev {
                ScriptEvent::Type(t, s) if t == tick => {
                    self.svc(serde_json::json!({"t": "ch", "s": s}))
                }
                ScriptEvent::Click(t, x, y) if t == tick => {
                    self.svc(serde_json::json!({"t": "mouse", "x": x, "y": y, "d": true}));
                    self.svc(serde_json::json!({"t": "mouse", "x": x, "y": y, "d": false}));
                    self.click_edge = true;
                }
                // Held for 6 ticks so edge-detected button handlers latch.
                ScriptEvent::Press(t, bit) if tick >= t && tick < t + 6 => {
                    self.script_buttons |= bit;
                }
                ScriptEvent::Press(t, bit) if tick == t + 6 => {
                    self.script_buttons &= !bit;
                }
                ScriptEvent::Mouse(t, x, y, kind) if t == tick => {
                    if kind == 'r' {
                        // Right click: press + release in one tick (b:2 lines).
                        self.svc(serde_json::json!(
                            {"t": "mouse", "x": x, "y": y, "d": true, "b": 2, "sh": false}
                        ));
                        self.svc(serde_json::json!(
                            {"t": "mouse", "x": x, "y": y, "d": false, "b": 2, "sh": false}
                        ));
                    } else {
                        let down = match kind {
                            'd' => {
                                self.script_mouse = true;
                                true
                            }
                            'u' => {
                                self.script_mouse = false;
                                false
                            }
                            _ => self.script_mouse,
                        };
                        self.svc(serde_json::json!(
                            {"t": "mouse", "x": x, "y": y, "d": down, "sh": false}
                        ));
                    }
                }
                ScriptEvent::Key(t, ref k, cmd, alt, ctl, sh) if t == tick => {
                    self.svc(serde_json::json!(
                        {"t": "key", "k": k, "cmd": cmd, "sh": sh, "alt": alt, "ctl": ctl}
                    ));
                }
                _ => {}
            }
        }
    }
}
fn run_runtime(
    args: Args,
    inputs: Receiver<Input>,
    outputs: SyncSender<Output>,
    proxy: EventLoopProxy<Wake>,
    gpu_rx: std::sync::mpsc::Receiver<Arc<pocket3d::gpu::Gpu>>,
) -> Result<()> {
    let available = Arc::new(AtomicBool::new(true));
    // C1: guest boot does not need the GPU — it runs concurrently with the
    // GPU path (instance is built on its own thread from process entry) and
    // only the renderer build waits for the device handle. Measured
    // trade-off (C1 evidence): letting guest ticks run before the renderer
    // existed overlapped the first decode but destabilized the P95 tail
    // (three-way startup contention), so ticks stay renderer-gated.
    let mut runtime = Runtime::boot(args)?;
    phase("runtime_boot_done");
    memprobe::stage("runtime_boot_done");
    let gpu = gpu_rx
        .recv()
        .map_err(|_| anyhow!("GPU initialization failed; renderer cannot start"))?;
    let mut renderer = gpu::Renderer::new(gpu);
    phase("runtime_renderer_ready");
    memprobe::stage("runtime_renderer_ready");
    let mut hash = None;
    let mut intents = Vec::new();
    let mut deadline = Instant::now();
    loop {
        for input in inputs.try_iter().take(256) {
            if !runtime.input(input)? {
                return Ok(());
            }
        }
        let work_start = Instant::now();
        intents.extend(runtime.tick()?);
        trace_frame(runtime.args.trace_frames, "tick", runtime.ticks, work_start);
        if intents.iter().any(|v| v["t"] == "quit") {
            return Ok(());
        }
        if intents.len() > 128 {
            return Err(anyhow!("Host intent queue exceeded budget"));
        }
        let next = runtime.hash();
        if (hash != Some(next) || !intents.is_empty())
            && let Some(permit) = OutputPermit::acquire(&available)
        {
            let target = if hash != Some(next) {
                let start = Instant::now();
                let frame = renderer.render(&mut runtime)?;
                trace_frame(
                    runtime.args.trace_frames,
                    "render-submit",
                    runtime.ticks,
                    start,
                );
                frame
            } else {
                None
            };
            let rendered = target.is_some();
            let output = Output {
                _permit: permit,
                tick: runtime.ticks,
                target,
                intents: std::mem::take(&mut intents),
            };
            match outputs.try_send(output) {
                Ok(()) => {
                    if rendered {
                        hash = Some(next);
                    }
                    let _ = proxy.send_event(Wake::Output);
                }
                Err(std::sync::mpsc::TrySendError::Full(output)) => intents = output.intents,
                Err(_) => return Ok(()),
            }
        }
        trace_frame(runtime.args.trace_frames, "work", runtime.ticks, work_start);
        if runtime
            .args
            .quit_after_ticks
            .is_some_and(|n| runtime.ticks >= n)
        {
            memprobe::stage("settled_quit");
            return Ok(());
        }
        if intents.is_empty() && hash == Some(next) && runtime.can_suspend() {
            // C3: event-driven idle suspend. Nothing is pending and the
            // guest declared static frames, so park on the input channel —
            // no 60 Hz wake, no guest JS, no tick bookkeeping — until a
            // real event arrives, then run one tick immediately (deadline
            // reset; no catch-up burst).
            if MEASUREMENT_TRACING.load(Ordering::Relaxed) {
                static SUSPENDED: AtomicBool = AtomicBool::new(false);
                if !SUSPENDED.swap(true, Ordering::Relaxed) {
                    eprintln!("C3EVENT,suspend,{}ms,tick={}", proc_ms(), runtime.ticks);
                }
            }
            match inputs.recv() {
                Ok(input) => {
                    if MEASUREMENT_TRACING.load(Ordering::Relaxed) {
                        eprintln!("C3EVENT,wake,{}ms,tick={}", proc_ms(), runtime.ticks);
                    }
                    if !runtime.input(input)? {
                        return Ok(());
                    }
                }
                Err(_) => return Ok(()),
            }
            deadline = Instant::now();
        } else {
            deadline += Duration::from_nanos(1_000_000_000 / 60);
            if let Some(wait) = deadline.checked_duration_since(Instant::now()) {
                thread::sleep(wait);
            } else {
                deadline = Instant::now();
            }
        }
    }
}
struct RuntimeStartup {
    args: Args,
    inputs: Receiver<Input>,
    outputs: SyncSender<Output>,
    proxy: EventLoopProxy<Wake>,
    /// C1: the wgpu instance is created on a side thread from process entry
    /// and lands here; `Presentation::new` consumes it after window creation.
    instance_rx: std::sync::mpsc::Receiver<wgpu::Instance>,
    /// C1: the runtime thread boots the guest while the GPU path finishes;
    /// the finished device/queue handle is sent through here. Dropping the
    /// sender (GPU failure path) releases the waiting runtime thread.
    gpu_tx: std::sync::mpsc::Sender<Arc<pocket3d::gpu::Gpu>>,
    gpu_rx: std::sync::mpsc::Receiver<Arc<pocket3d::gpu::Gpu>>,
}
struct Host {
    window: Option<Arc<Window>>,
    surface: Option<gpu::Presentation>,
    startup: Option<RuntimeStartup>,
    tx: SyncSender<Input>,
    rx: Receiver<Output>,
    pending: VecDeque<Input>,
    frame: Option<(u64, Arc<gpu::Target>)>,
    title: String,
    viewport: (u32, u32),
    fixed: bool,
    modifiers: ModifiersState,
    pointer: (f64, f64),
    down: bool,
    ime: bool,
    clipboard: Option<arboard::Clipboard>,
    ready: bool,
    announce_ready: bool,
    /// A7: an image was bound and its first present submission has not
    /// been marked yet (T6 first-useful-image proxy).
    image_pending: bool,
    image_announced: bool,
    trace_frames: bool,
    #[cfg(feature = "bench-harness")]
    resize_at: Option<((u32, u32), u64)>,
    #[cfg(feature = "bench-harness")]
    resize_done: bool,
    /// A6: the same schedule on the host wall clock (nominal 60 Hz ticks
    /// from process start), consumed by about_to_wait.
    #[cfg(feature = "bench-harness")]
    scale_at_instant: Vec<(f64, u64, Instant)>,
    #[cfg(feature = "bench-harness")]
    scale_done: usize,
    /// A6 driven scale; None until the first transition, after which it
    /// governs every logical↔physical conversion in this host.
    scale_override: Option<f64>,
    /// Set once any scale transition has been driven; gates the A6
    /// physical/logical trace lines and the raster-follow logging.
    scale_driven: bool,
    failure: Option<String>,
}
impl Host {
    /// The scale every logical↔physical conversion in this host uses:
    /// the driven value once a scale transition happened (harness or OS),
    /// else the window's own monitor-derived scale.
    fn current_scale(&self) -> f64 {
        self.scale_override.unwrap_or_else(|| {
            self.window
                .as_ref()
                .map(|w| w.scale_factor())
                .unwrap_or(1.0)
        })
    }

    /// A6: one scale transition, from either the real OS event
    /// (`ScaleFactorChanged`, multi-monitor move) or the scripted harness
    /// path (`--scale-at`). The logical viewport is the invariant — it is
    /// re-asserted so physical client pixels become logical × scale — and
    /// the raster density follows the scale (see `effective_density`).
    fn apply_scale(&mut self, scale: f64, tick: u64) {
        self.scale_driven = true;
        self.scale_override = Some(scale);
        if let Some(window) = &self.window {
            let _resized = window.request_inner_size(PhysicalSize::new(
                (self.viewport.0 as f64 * scale).round() as u32,
                (self.viewport.1 as f64 * scale).round() as u32,
            ));
            window.request_redraw();
        }
        let _ = self.tx.send(Input::Scale(scale));
        if MEASUREMENT_TRACING.load(Ordering::Relaxed) {
            eprintln!(
                "A6EVENT,scale,{scale},logical={}x{},tick={tick}",
                self.viewport.0, self.viewport.1
            );
        }
    }
    fn send(&mut self, input: Input) {
        // Coalesce only motion; button and key edges retain FIFO order.
        if let Input::Pointer(value) = &input
            && let Some(Input::Pointer(last)) = self.pending.back_mut()
        {
            *last = value.clone();
            self.flush();
            return;
        }
        if self.pending.len() < 256 {
            self.pending.push_back(input);
        } else {
            self.pending.clear();
            self.pending.push_back(Input::Reset);
        }
        self.flush();
    }
    fn flush(&mut self) {
        while let Some(input) = self.pending.pop_front() {
            match self.tx.try_send(input) {
                Ok(()) => {}
                Err(std::sync::mpsc::TrySendError::Full(input)) => {
                    self.pending.push_front(input);
                    break;
                }
                Err(_) => break,
            }
        }
    }
    fn key_name(key: &Key) -> String {
        match key {
            Key::Character(s) => s.to_lowercase(),
            Key::Named(n) => match n {
                NamedKey::ArrowUp => "up",
                NamedKey::ArrowDown => "down",
                NamedKey::ArrowLeft => "left",
                NamedKey::ArrowRight => "right",
                NamedKey::Enter => "enter",
                NamedKey::Escape => "escape",
                NamedKey::Backspace => "backspace",
                NamedKey::Delete => "delete",
                NamedKey::Tab => "tab",
                NamedKey::Space => "space",
                NamedKey::Home => "home",
                NamedKey::End => "end",
                NamedKey::PageUp => "pageup",
                NamedKey::PageDown => "pagedown",
                _ => "",
            }
            .into(),
            _ => String::new(),
        }
    }
    fn present(&mut self) -> Result<()> {
        let (Some(surface), Some(window), Some(frame)) =
            (&mut self.surface, &self.window, &self.frame)
        else {
            return Ok(());
        };
        let (tick, target) = frame;
        let start = Instant::now();
        if !surface.present(window, target)? {
            return Ok(());
        }
        trace_frame(self.trace_frames, "present-submit", *tick, start);
        if !self.ready {
            self.ready = true;
            memprobe::stage("first_present");
            if self.announce_ready {
                println!("READY {}", epoch_ms());
            }
        }
        if self.image_pending {
            self.image_pending = false;
            if self.announce_ready && !self.image_announced {
                // A7: T6 present-submitted for the first useful image —
                // the startup probe's second marker.
                self.image_announced = true;
                println!("IMGREADY {}", epoch_ms());
            }
        }
        Ok(())
    }
}
impl ApplicationHandler<Wake> for Host {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        #[cfg(windows)]
        {
            // A6: pin the Per-Monitor DPI Awareness V2 contract. winit
            // already requests it for the process; the explicit call keeps
            // the host honest even if the toolkit default ever changes.
            // Idempotent (returns FALSE with ERROR_ACCESS_DENIED if the
            // awareness context is already fixed).
            use windows::Win32::UI::HiDpi::{
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
            };
            unsafe {
                let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            }
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(&self.title)
                        .with_inner_size(LogicalSize::new(self.viewport.0, self.viewport.1))
                        .with_resizable(!self.fixed),
                )
                .expect("create window"),
        );
        #[cfg(windows)]
        {
            // A6: prove the created window actually runs under PMv2.
            use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::HiDpi::{
                AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                GetWindowDpiAwarenessContext, GetDpiForWindow,
            };
            let hwnd = match window.window_handle().expect("window handle").as_raw() {
                RawWindowHandle::Win32(win) => HWND(win.hwnd.get() as *mut _),
                _ => unreachable!("windows build always has a Win32 handle"),
            };
            unsafe {
                let ctx = GetWindowDpiAwarenessContext(hwnd);
                let pmv2 = AreDpiAwarenessContextsEqual(
                    ctx,
                    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                );
                let dpi = GetDpiForWindow(hwnd);
                if MEASUREMENT_TRACING.load(Ordering::Relaxed) {
                    eprintln!(
                        "A6EVENT,dpi-awareness,per_monitor_v2={},windowDpi={}",
                        pmv2.as_bool(),
                        dpi
                    );
                }
            }
        }
        window.set_ime_allowed(true);
        memprobe::stage("window_created");
        // C1: the runtime thread spawns BEFORE the GPU path completes — its
        // guest boot (QuickJS + bundle eval) runs concurrently with
        // adapter/device/surface initialization, and only the renderer build
        // waits for the device handle. Frame/present semantics are unchanged.
        let RuntimeStartup {
            args,
            inputs,
            outputs,
            proxy,
            instance_rx,
            gpu_tx,
            gpu_rx,
        } = self.startup.take().expect("runtime startup");
        phase("runtime_thread_spawning");
        if let Err(error) = thread::Builder::new()
            .name("pocket-runtime".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_runtime(args, inputs, outputs, proxy.clone(), gpu_rx)
                }))
                .unwrap_or_else(|_| Err(anyhow!("Runtime worker panicked")));
                let _ = proxy.send_event(Wake::Exit(result.err().map(|e| format!("{e:#}"))));
            })
        {
            self.failure = Some(format!("Runtime startup: {error}"));
            event_loop.exit();
            return;
        }
        let instance = match instance_rx.recv() {
            Ok(instance) => instance,
            Err(_) => {
                drop(gpu_tx);
                self.failure = Some("GPU instance thread exited".into());
                event_loop.exit();
                return;
            }
        };
        phase("gpu_instance");
        memprobe::stage("gpu_instance");
        let presentation = match gpu::Presentation::new(window.clone(), instance) {
            Ok(presentation) => presentation,
            Err(error) => {
                drop(gpu_tx);
                self.failure = Some(format!("GPU initialization: {error:#}"));
                event_loop.exit();
                return;
            }
        };
        phase("gpu_ready");
        memprobe::stage("gpu_ready");
        let _ = gpu_tx.send(presentation.gpu.clone());
        self.surface = Some(presentation);
        self.window = Some(window);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Wake) {
        match event {
            Wake::Exit(error) => {
                if let Some(error) = error {
                    log::error!("{error}");
                    self.failure = Some(error);
                }
                event_loop.exit();
            }
            Wake::Output => {
                while let Ok(mut output) = self.rx.try_recv() {
                    for v in std::mem::take(&mut output.intents) {
                        match v["t"].as_str() {
                            Some("imgready") => {
                                // A7: an image was bound; the next
                                // successful present is its T6.
                                self.image_pending = true;
                            }
                            Some("copy") => {
                                if let (Some(clipboard), Some(text)) =
                                    (&mut self.clipboard, v["text"].as_str())
                                {
                                    let _ = clipboard.set_text(text);
                                }
                            }
                            Some("paste-req") => {
                                if let Some(Ok(text)) =
                                    self.clipboard.as_mut().map(|c| c.get_text())
                                {
                                    self.send(Input::Service(json!({"t":"paste","text":text})));
                                }
                            }
                            Some("caret") => {
                                if let Some(window) = &self.window {
                                    window.set_ime_cursor_area(
                                        LogicalPosition::new(
                                            v["x"].as_f64().unwrap_or(0.0),
                                            v["y"].as_f64().unwrap_or(0.0),
                                        ),
                                        LogicalSize::new(1.0, v["h"].as_f64().unwrap_or(16.0)),
                                    );
                                }
                            }
                            Some("cursor") => {
                                if let Some(window) = &self.window {
                                    window.set_cursor(match v["k"].as_str().unwrap_or("") {
                                        "text" => CursorIcon::Text,
                                        "pointer" => CursorIcon::Pointer,
                                        "move" => CursorIcon::Move,
                                        "grabbing" => CursorIcon::Grabbing,
                                        "ew" => CursorIcon::EwResize,
                                        "ns" => CursorIcon::NsResize,
                                        "nwse" => CursorIcon::NwseResize,
                                        "nesw" => CursorIcon::NeswResize,
                                        _ => CursorIcon::Default,
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(target) = output.target.take() {
                        self.frame = Some((output.tick, target));
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                    #[cfg(feature = "bench-harness")]
                    if !self.resize_done
                        && let Some(((w, h), at)) = self.resize_at
                        && output.tick >= at
                        && let Some(window) = &self.window
                    {
                        // Real OS-window resize: winit emits Resized, the
                        // normal live-viewport path takes over from there.
                        self.resize_done = true;
                        if MEASUREMENT_TRACING.load(Ordering::Relaxed) {
                            eprintln!("A2EVENT,resize-window,{w},{h},atTick={at}");
                        }
                        let _resized = window.request_inner_size(LogicalSize::new(w, h));
                    }
                    // A6 scale transitions are driven from about_to_wait on
                    // the host clock: a static image emits no outputs, so
                    // scripted transitions cannot wait on output ticks.
                }
            }
        }
        self.flush();
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.flush();
        // A6: drive due scale transitions on the host clock (see the
        // schedule note on Host::scale_at_instant).
        #[cfg(feature = "bench-harness")]
        while self.scale_done < self.scale_at_instant.len()
            && self.scale_at_instant[self.scale_done].2 <= Instant::now()
        {
            let (scale, tick, _due) = self.scale_at_instant[self.scale_done];
            self.scale_done += 1;
            self.apply_scale(scale, tick);
        }
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
        #[cfg(feature = "bench-harness")]
        if !self.pending.is_empty() {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(8),
            ));
        } else if let Some((_, _, next)) = self
            .scale_at_instant
            .get(self.scale_done)
            .filter(|(_, _, due)| *due > Instant::now())
        {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(*next));
        }
        #[cfg(not(feature = "bench-harness"))]
        if !self.pending.is_empty() {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(8),
            ));
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.send(Input::Quit);
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.present() {
                    log::error!("{error}");
                    self.failure = Some(error.to_string());
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(size) => {
                let scale = self.current_scale();
                let logical = logical_from_physical(size.width, size.height, scale);
                if self.scale_driven && MEASUREMENT_TRACING.load(Ordering::Relaxed) {
                    eprintln!(
                        "A6EVENT,physical,{}x{},scale={scale},logical={}x{}",
                        size.width, size.height, logical.0, logical.1
                    );
                }
                self.send(Input::Resize(logical.0, logical.1));
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // Real OS monitor-DPI transition (multi-monitor move) —
                // identical handler to the scripted harness path.
                let tick = self.frame.as_ref().map_or(0, |(t, _)| *t);
                self.apply_scale(scale_factor, tick);
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::Focused(false) => {
                self.down = false;
                self.send(Input::Reset);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.current_scale();
                self.pointer = (position.x / scale, position.y / scale);
                self.send(Input::Pointer(json!({"t":"mouse","x":self.pointer.0,"y":self.pointer.1,"d":self.down,"sh":self.modifiers.shift_key()})));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    self.down = state == ElementState::Pressed;
                }
                self.send(Input::Service(json!({"t":"mouse","x":self.pointer.0,"y":self.pointer.1,"d":state==ElementState::Pressed,"b":if button==MouseButton::Right{2}else{0},"sh":self.modifiers.shift_key()})));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -(y as f64) * 24.0,
                    MouseScrollDelta::PixelDelta(p) => -p.y,
                };
                self.send(Input::Service(json!({"t":"scroll","dy":dy})));
            }
            WindowEvent::Ime(Ime::Enabled) => {}
            WindowEvent::Ime(Ime::Disabled) => self.ime = false,
            WindowEvent::Ime(Ime::Preedit(s, cursor)) => {
                self.ime = !s.is_empty();
                let c = cursor
                    .map(|(start, _)| s[..start].encode_utf16().count())
                    .unwrap_or(0);
                self.send(Input::Service(json!({"t":"ime","s":s,"c":c})));
            }
            WindowEvent::Ime(Ime::Commit(s)) => {
                self.ime = false;
                self.send(Input::Service(json!({"t":"ime","s":"","c":0})));
                self.send(Input::Service(json!({"t":"ch","s":s})));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let name = Self::key_name(&event.logical_key);
                let down = event.state == ElementState::Pressed;
                // C4: the OS-event arrival stamp on the shared monotonic
                // epoch clock — joins INPUT_TRACE against the tick /
                // render-submit / present-submit FRAME_TRACE lines so the
                // input→present chain decomposes into wake, work, and
                // present terms. Trace-gated like every FRAME_TRACE line.
                if down && self.trace_frames {
                    eprintln!("INPUT_TRACE,key,{},{}", name, epoch_us());
                }
                let cmd = if cfg!(target_os = "macos") {
                    self.modifiers.super_key()
                } else {
                    self.modifiers.control_key()
                };
                if let Some(bit) = button_for(&name) {
                    self.send(Input::Button(bit, down));
                }
                if down {
                    if cmd && name == "q" {
                        self.send(Input::Quit);
                        event_loop.exit();
                        return;
                    }
                    if cmd && name == "v" {
                        if let Some(Ok(text)) = self.clipboard.as_mut().map(|c| c.get_text()) {
                            self.send(Input::Service(json!({"t":"paste","text":text})));
                        }
                        return;
                    }
                    self.send(Input::Service(json!({"t":"key","k":if cmd {name.clone()} else {match &event.logical_key {Key::Named(n)=>format!("{n:?}").replace("Arrow", ""),_=>name.clone()}},"cmd":cmd,"ctl":self.modifiers.control_key(),"alt":self.modifiers.alt_key(),"sh":self.modifiers.shift_key()})));
                    if !self.ime
                        && !cmd
                        && !self.modifiers.control_key()
                        && let Some(text) = event.text
                        && !text.chars().any(char::is_control)
                    {
                        self.send(Input::Service(json!({"t":"ch","s":text.as_str()})));
                    }
                }
            }
            _ => {}
        }
    }
}
fn main() -> Result<()> {
    // Anchor the monotonic phase clock at true process entry — before any
    // init cost — so A7EVENT phase values are same-origin across builds
    // regardless of which phase lines the measurement flag lets print.
    let _entry_anchor = proc_ms();
    let args = parse_args()?;
    MEASUREMENT_TRACING.store(args.announce_ready, Ordering::Relaxed);
    phase("main_entry");
    memprobe::stage("process_entry");
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let event_loop = EventLoop::<Wake>::with_user_event().build()?;
    phase("event_loop_built");
    memprobe::stage("event_loop_built");
    let (tx, inputs) = sync_channel(256);
    let (outputs, rx) = sync_channel(1);
    let mut host = Host {
        window: None,
        surface: None,
        startup: None,
        tx,
        rx,
        pending: VecDeque::new(),
        frame: None,
        title: args.title.clone(),
        viewport: args.viewport,
        fixed: args.fixed,
        modifiers: ModifiersState::empty(),
        pointer: (0.0, 0.0),
        down: false,
        ime: false,
        clipboard: arboard::Clipboard::new().ok(),
        ready: false,
        announce_ready: args.announce_ready,
        image_pending: false,
        image_announced: false,
        trace_frames: args.trace_frames,
        #[cfg(feature = "bench-harness")]
        resize_at: args.resize_at,
        #[cfg(feature = "bench-harness")]
        resize_done: false,
        #[cfg(feature = "bench-harness")]
        scale_at_instant: Vec::new(),
        #[cfg(feature = "bench-harness")]
        scale_done: 0,
        scale_override: None,
        scale_driven: false,
        failure: None,
    };
    // A6: nominal 60 Hz tick schedule on the host wall clock.
    #[cfg(feature = "bench-harness")]
    {
        let host_start = Instant::now();
        host.scale_at_instant = args
            .scale_at
            .iter()
            .map(|(scale, tick)| {
                (
                    *scale,
                    *tick,
                    host_start + Duration::from_nanos(1_000_000_000 * *tick / 60),
                )
            })
            .collect();
    }
    // C1: backend/ICD initialization is the largest single serial startup
    // term (measured ~187 ms on the reference AMD Vulkan driver) and
    // depends on nothing else — create the instance on a side thread from
    // process entry so it overlaps event-loop/window creation and guest
    // boot. Deterministic failure is preserved: an instance-thread spawn
    // error aborts here; a send-side drop releases the runtime thread.
    let (instance_tx, instance_rx) = std::sync::mpsc::channel();
    let (gpu_tx, gpu_rx) = std::sync::mpsc::channel();
    if let Err(error) = thread::Builder::new()
        .name("pocket-gpu-instance".into())
        .spawn(move || {
            let instance = gpu::Presentation::create_instance();
            let _ = instance_tx.send(instance);
        })
    {
        return Err(anyhow!("GPU instance thread: {error}"));
    }
    host.startup = Some(RuntimeStartup {
        args,
        inputs,
        outputs,
        proxy: event_loop.create_proxy(),
        instance_rx,
        gpu_tx,
        gpu_rx,
    });
    event_loop.run_app(&mut host)?;
    if let Some(error) = host.failure {
        return Err(anyhow!(error));
    }
    Ok(())
}

// These are CPU submission durations, not GPU completion or display latency.
fn trace_frame(enabled: bool, stage: &str, tick: u64, start: Instant) {
    if enabled {
        let elapsed = start.elapsed().as_micros();
        // A7: `wall` is the same monotonic process clock as A3EVENT
        // epochUs (epoch_us) — joins and deltas stay within one clock.
        let wall = epoch_us();
        eprintln!("FRAME_TRACE,{stage},{tick},{wall},{elapsed}");
    }
}

include!("tests.rs");
