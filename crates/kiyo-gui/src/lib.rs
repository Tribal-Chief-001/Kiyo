//! `kiyo-gui`: High-framerate desktop graphical viewer with kinetic physics and IPC socket server.

#![allow(clippy::collapsible_if)]

use anyhow::Result;
use kiyo_render::RenderBackend;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Continuous kinetic scroll simulation engine.
///
/// Implements viscous fluid friction ($\mu_{\text{visc}} = 3.5$), quadratic aerodynamic drag
/// ($c_{\text{aero}} = 0.0012$), non-linear rubber-band overscroll pull resistance ($d = 120.0$),
/// and critically damped harmonic restitution ($\zeta = 1.0, \omega_n = 26.0\text{ rad/s}$).
#[derive(Clone, Debug, PartialEq)]
pub struct KineticScrollEngine {
    pub position: f32,
    pub velocity: f32,
    pub min_position: f32,
    pub max_position: f32,
    pub is_dragging: bool,
    drag_start_cursor: f32,
    drag_start_pos: f32,
}

impl KineticScrollEngine {
    /// Friction parameters calibrated to match human physical ergonomics
    pub const MU_VISC: f32 = 3.5;
    pub const C_AERO: f32 = 0.0012;
    pub const D_OVERSCROLL: f32 = 120.0;
    pub const OMEGA_N: f32 = 26.0;
    pub const ZETA: f32 = 1.0;
    pub const J_WHEEL: f32 = 350.0;

    /// Initializes a scroll engine configured for document length and viewport dimensions.
    pub fn new(content_length: f32, viewport_length: f32) -> Self {
        let max_pos = (content_length - viewport_length).max(0.0);
        Self {
            position: 0.0,
            velocity: 0.0,
            min_position: 0.0,
            max_position: max_pos,
            is_dragging: false,
            drag_start_cursor: 0.0,
            drag_start_pos: 0.0,
        }
    }

    /// Updates dynamic content boundaries when viewport resizes or zoom changes.
    pub fn set_bounds(&mut self, content_length: f32, viewport_length: f32) {
        self.max_position = (content_length - viewport_length).max(0.0);
    }

    /// Adds instantaneous mouse wheel detent impulse.
    pub fn add_wheel_impulse(&mut self, notches: f32) {
        self.velocity += notches * Self::J_WHEEL;
    }

    /// Initiates direct pointer drag.
    pub fn start_drag(&mut self, cursor_pos: f32) {
        self.is_dragging = true;
        self.velocity = 0.0;
        self.drag_start_cursor = cursor_pos;
        self.drag_start_pos = self.position;
    }

    /// Updates displacement during pointer drag with hyperbolic tangent overscroll clamping.
    pub fn drag_to(&mut self, cursor_pos: f32) {
        let raw_delta = self.drag_start_cursor - cursor_pos;
        let target_pos = self.drag_start_pos + raw_delta;

        if target_pos < self.min_position {
            let delta = target_pos - self.min_position;
            self.position =
                self.min_position + Self::D_OVERSCROLL * (delta / Self::D_OVERSCROLL).tanh();
        } else if target_pos > self.max_position {
            let delta = target_pos - self.max_position;
            self.position =
                self.max_position + Self::D_OVERSCROLL * (delta / Self::D_OVERSCROLL).tanh();
        } else {
            self.position = target_pos;
        }
    }

    /// Releases direct pointer drag, converting residual velocity into momentum fling.
    pub fn release_drag(&mut self, release_velocity: f32) {
        self.is_dragging = false;
        self.velocity = release_velocity;
    }

    /// Integrates the physics simulation over time interval `dt` (in seconds).
    pub fn step(&mut self, dt: f32) {
        if self.is_dragging {
            return;
        }

        // Within document bounds: Viscous and aerodynamic drag decay
        if self.position >= self.min_position && self.position <= self.max_position {
            let drag =
                -Self::MU_VISC * self.velocity - Self::C_AERO * self.velocity * self.velocity.abs();
            self.velocity += drag * dt;
            self.position += self.velocity * dt;

            // Clamp micro-creep
            if self.velocity.abs() < 1.0 {
                self.velocity = 0.0;
            }
        } else {
            // Past boundaries: Critically damped harmonic restitution (zeta = 1.0, omega_n = 26.0)
            let boundary = if self.position < self.min_position {
                self.min_position
            } else {
                self.max_position
            };

            let x = self.position - boundary;
            // 2 * zeta * omega_n = 52.0, omega_n^2 = 676.0
            let spring_acc = -52.0 * self.velocity - 676.0 * x;
            self.velocity += spring_acc * dt;
            self.position += self.velocity * dt;

            // Snap back when close enough to boundary
            if x.abs() < 0.2 && self.velocity.abs() < 2.0 {
                self.position = boundary;
                self.velocity = 0.0;
            }
        }
    }

    /// Checks if the engine has settled to a full stop within bounds.
    pub fn is_settled(&self) -> bool {
        !self.is_dragging
            && self.velocity.abs() < 0.1
            && self.position >= self.min_position - 0.1
            && self.position <= self.max_position + 0.1
    }
}

/// Commands accepted over UNIX domain socket IPC from `kiyo-remote`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", content = "payload")]
pub enum IpcCommand {
    NextPage,
    PrevPage,
    Goto { page: usize },
    Zoom { level: String },
    ToggleDark,
    JumpTag { tag: String },
    GetStatus,
}

/// Status payload emitted by active Kiyo GUI instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpcStatus {
    pub file: Option<String>,
    pub page: usize,
    pub total: usize,
    pub zoom: f32,
    pub dark: bool,
}

/// Active UNIX Domain Socket IPC Server.
pub struct IpcServer {
    pub socket_path: PathBuf,
    pub symlink_path: PathBuf,
    running: Arc<AtomicBool>,
}

impl IpcServer {
    /// Locates the appropriate runtime directory for UNIX domain sockets.
    pub fn socket_dir() -> PathBuf {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let path = PathBuf::from(runtime_dir).join("kiyo");
            if std::fs::create_dir_all(&path).is_ok() {
                return path;
            }
        }
        let fallback = std::env::temp_dir().join(format!("kiyo-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&fallback);
        fallback
    }

    /// Starts an active IPC server listener thread linked to the application state.
    pub fn start(state: Arc<Mutex<GuiState>>) -> Result<Self> {
        Self::start_with_id(state, None)
    }

    /// Starts an active IPC server with an optional custom instance identifier.
    pub fn start_with_id(state: Arc<Mutex<GuiState>>, instance_id: Option<&str>) -> Result<Self> {
        let dir = Self::socket_dir();
        let socket_name = if let Some(id) = instance_id {
            format!("{id}.sock")
        } else {
            format!("{}.sock", std::process::id())
        };
        let socket_path = dir.join(&socket_name);
        let symlink_path = if let Some(id) = instance_id {
            dir.join(format!("{id}_active.sock"))
        } else {
            dir.join("active.sock")
        };

        // Clean up stale socket file if it exists
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }

        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;

        // Update atomic symlink via temp symlink and atomic rename
        let tmp_symlink = dir.join(format!("{}.tmp", socket_name));
        if tmp_symlink.exists() || tmp_symlink.is_symlink() {
            let _ = std::fs::remove_file(&tmp_symlink);
        }
        if std::os::unix::fs::symlink(&socket_path, &tmp_symlink).is_ok() {
            let _ = std::fs::rename(&tmp_symlink, &symlink_path);
        }

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let socket_path_clone = socket_path.clone();

        thread::spawn(move || {
            while running_clone.load(Ordering::Relaxed) {
                let mut processed_any = false;
                loop {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            processed_any = true;
                            let _ = stream
                                .set_read_timeout(Some(std::time::Duration::from_millis(500)));
                            let _ = stream
                                .set_write_timeout(Some(std::time::Duration::from_millis(500)));
                            let mut buf = String::new();
                            let _ = stream.read_to_string(&mut buf);
                            if let Ok(cmd) = serde_json::from_str::<IpcCommand>(buf.trim()) {
                                let mut app = state.lock().unwrap();
                                let response = app.handle_ipc_command(cmd);
                                if let Ok(resp_json) = serde_json::to_string(&response) {
                                    let _ = stream.write_all(resp_json.as_bytes());
                                }
                            }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            break;
                        }
                        Err(_) => return,
                    }
                }
                if !processed_any {
                    thread::sleep(std::time::Duration::from_millis(5));
                }
            }
            let _ = std::fs::remove_file(socket_path_clone);
        });

        Ok(Self {
            socket_path,
            symlink_path,
            running,
        })
    }

    /// Stops the IPC server and cleans up socket files.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = std::fs::remove_file(&self.socket_path);
        if self.symlink_path.is_symlink() {
            let _ = std::fs::remove_file(&self.symlink_path);
        }
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Shared state of the desktop GUI viewer.
pub struct GuiState {
    pub current_file: Option<PathBuf>,
    pub current_page: usize,
    pub total_pages: usize,
    pub zoom: f32,
    pub dark_mode: bool,
    pub kinetic_scroll: KineticScrollEngine,
}

impl GuiState {
    pub fn new() -> Self {
        Self {
            current_file: None,
            current_page: 0,
            total_pages: 1,
            zoom: 1.0,
            dark_mode: false,
            kinetic_scroll: KineticScrollEngine::new(1000.0, 800.0),
        }
    }

    pub fn handle_ipc_command(&mut self, cmd: IpcCommand) -> IpcStatus {
        match cmd {
            IpcCommand::NextPage => {
                if self.current_page + 1 < self.total_pages {
                    self.current_page += 1;
                }
            }
            IpcCommand::PrevPage => {
                if self.current_page > 0 {
                    self.current_page -= 1;
                }
            }
            IpcCommand::Goto { page } => {
                if page < self.total_pages {
                    self.current_page = page;
                }
            }
            IpcCommand::Zoom { level } => match level.as_str() {
                "in" => self.zoom = (self.zoom * 1.2).min(10.0),
                "out" => self.zoom = (self.zoom / 1.2).max(0.1),
                "fit-width" => self.zoom = 1.0,
                "fit-page" => self.zoom = 0.85,
                other => {
                    if let Ok(factor) = other.parse::<f32>() {
                        self.zoom = factor.clamp(0.1, 10.0);
                    }
                }
            },
            IpcCommand::ToggleDark => {
                self.dark_mode = !self.dark_mode;
            }
            IpcCommand::JumpTag { .. } => {}
            IpcCommand::GetStatus => {}
        }

        self.status()
    }

    pub fn status(&self) -> IpcStatus {
        IpcStatus {
            file: self
                .current_file
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            page: self.current_page,
            total: self.total_pages,
            zoom: self.zoom,
            dark: self.dark_mode,
        }
    }
}

impl Default for GuiState {
    fn default() -> Self {
        Self::new()
    }
}

/// High-framerate desktop graphical viewer application.
pub struct GuiApp {
    pub state: Arc<Mutex<GuiState>>,
    pub ipc_server: Option<IpcServer>,
}

impl GuiApp {
    pub fn new() -> Result<Self> {
        let state = Arc::new(Mutex::new(GuiState::new()));
        let ipc_server = IpcServer::start(Arc::clone(&state)).ok();
        Ok(Self { state, ipc_server })
    }

    pub fn with_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let app = Self::new()?;
        {
            let mut state = app.state.lock().unwrap();
            state.current_file = Some(path.as_ref().to_path_buf());
            if let Ok(doc) = kiyo_render::open_universal_document(path.as_ref()) {
                state.total_pages = doc.page_count() as usize;
            }
        }
        Ok(app)
    }

    pub fn run(self) -> Result<()> {
        // Check if a graphical display server (Wayland or X11) is available
        let has_display =
            std::env::var("WAYLAND_DISPLAY").is_ok() || std::env::var("DISPLAY").is_ok();

        if !has_display {
            tracing::info!(
                "Headless environment detected; GUI loop initialized and exiting cleanly."
            );
            return Ok(());
        }

        // Initialize and execute the interactive winit desktop event loop
        let event_loop = winit::event_loop::EventLoop::new()
            .map_err(|e| anyhow::anyhow!("Failed to initialize winit event loop: {:?}", e))?;
        
        let mut window_app = WindowApp::new(Arc::clone(&self.state));
        event_loop
            .run_app(&mut window_app)
            .map_err(|e| anyhow::anyhow!("Kiyo GUI event loop error: {:?}", e))?;

        Ok(())
    }
}

/// Interactive desktop window application handler managing surface presentation and input.
struct WindowApp {
    state: Arc<Mutex<GuiState>>,
    window: Option<Arc<winit::window::Window>>,
    context: Option<softbuffer::Context<Arc<winit::window::Window>>>,
    surface: Option<softbuffer::Surface<Arc<winit::window::Window>, Arc<winit::window::Window>>>,
    render_backend: Option<Arc<kiyo_render::UniversalRenderBackend>>,
}

impl WindowApp {
    fn new(state: Arc<Mutex<GuiState>>) -> Self {
        Self {
            state,
            window: None,
            context: None,
            surface: None,
            render_backend: None,
        }
    }

    fn update_title(&self) {
        if let Some(win) = &self.window {
            let s = self.state.lock().unwrap();
            let name = s
                .current_file
                .as_ref()
                .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
                .unwrap_or_else(|| "Kiyo Reader".to_string());
            let dark_str = if s.dark_mode { "ON" } else { "OFF" };
            let title = format!(
                "🌸 Kiyo — {} [Page {}/{}] [Zoom: {}%] [d: Smart Dark {}]",
                name,
                s.current_page + 1,
                s.total_pages.max(1),
                (s.zoom * 100.0) as u32,
                dark_str
            );
            win.set_title(&title);
        }
    }

    fn render_frame(
        state: &Arc<Mutex<GuiState>>,
        render_backend: Option<&Arc<kiyo_render::UniversalRenderBackend>>,
        buffer: &mut [u32],
        width: u32,
        height: u32,
    ) {
        let (curr_page, zoom, dark_mode) = {
            let s = state.lock().unwrap();
            (s.current_page as u32, s.zoom, s.dark_mode)
        };

        // 1. Desk background color
        let desk_bg = if dark_mode { 0x0014171C } else { 0x002B303A };
        buffer.fill(desk_bg);

        // 2. Render page if backend is loaded
        if let Some(backend) = render_backend {
            let tile_key = kiyo_render::TileKey {
                page_index: curr_page,
                level_of_detail: 0,
                tile_col: 0,
                tile_row: 0,
            };
            if let Ok(tile) = backend.render_tile(tile_key, zoom) {
                let mut pixels = tile.pixels;
                if dark_mode {
                    kiyo_gpu::apply_smart_dark_cpu(&mut pixels, kiyo_gpu::DarkModeUniforms::default());
                }

                let tile_w = tile.width as usize;
                let tile_h = tile.height as usize;

                let available_h = height.saturating_sub(72) as usize;
                let available_w = width as usize;

                let start_x = if available_w > tile_w {
                    (available_w - tile_w) / 2
                } else {
                    0
                };
                let start_y = if available_h > tile_h {
                    40 + (available_h - tile_h) / 2
                } else {
                    40
                };

                for ty in 0..tile_h {
                    let dest_y = start_y + ty;
                    if dest_y >= height.saturating_sub(32) as usize {
                        break;
                    }
                    for tx in 0..tile_w {
                        let dest_x = start_x + tx;
                        if dest_x >= width as usize {
                            break;
                        }
                        let src_idx = (ty * tile_w + tx) * 4;
                        let r = pixels[src_idx] as u32;
                        let g = pixels[src_idx + 1] as u32;
                        let b = pixels[src_idx + 2] as u32;
                        let argb = (r << 16) | (g << 8) | b;

                        let dest_idx = dest_y * width as usize + dest_x;
                        buffer[dest_idx] = argb;
                    }
                }
            }
        }

        // 3. Draw top header bar (40px high)
        let header_bg = 0x001A1D24;
        let header_h = 40.min(height as usize);
        for y in 0..header_h {
            for x in 0..width as usize {
                buffer[y * width as usize + x] = header_bg;
            }
        }

        // 4. Draw bottom footer bar (32px high)
        let footer_bg = 0x0015181F;
        let footer_start = height.saturating_sub(32) as usize;
        for y in footer_start..height as usize {
            for x in 0..width as usize {
                buffer[y * width as usize + x] = footer_bg;
            }
        }
    }
}

impl winit::application::ApplicationHandler for WindowApp {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = winit::window::Window::default_attributes()
            .with_title("🌸 Kiyo Reader")
            .with_inner_size(winit::dpi::LogicalSize::new(1024.0, 768.0));

        if let Ok(window) = event_loop.create_window(attrs) {
            let window = Arc::new(window);
            if let Ok(context) = softbuffer::Context::new(Arc::clone(&window)) {
                if let Ok(surface) = softbuffer::Surface::new(&context, Arc::clone(&window)) {
                    self.context = Some(context);
                    self.surface = Some(surface);
                    self.window = Some(Arc::clone(&window));

                    // Load document backend if file path is available
                    let file_path = self.state.lock().unwrap().current_file.clone();
                    if let Some(path) = file_path {
                        if let Ok(doc) = kiyo_render::open_universal_document(&path) {
                            let backend =
                                Arc::new(kiyo_render::UniversalRenderBackend::new(Arc::from(doc)));
                            let page_count = backend.page_count();
                            {
                                let mut s = self.state.lock().unwrap();
                                s.total_pages = page_count as usize;
                            }
                            self.render_backend = Some(backend);
                        }
                    }

                    self.update_title();
                    window.request_redraw();
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        match event {
            winit::event::WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            winit::event::WindowEvent::RedrawRequested => {
                let win_size = self.window.as_ref().map(|w| w.inner_size());
                if let (Some(size), Some(surface)) = (win_size, &mut self.surface) {
                    if let (Some(w), Some(h)) = (
                        std::num::NonZeroU32::new(size.width),
                        std::num::NonZeroU32::new(size.height),
                    ) {
                        if surface.resize(w, h).is_ok() {
                            if let Ok(mut buffer) = surface.buffer_mut() {
                                Self::render_frame(
                                    &self.state,
                                    self.render_backend.as_ref(),
                                    &mut buffer,
                                    size.width,
                                    size.height,
                                );
                                let _ = buffer.present();
                            }
                        }
                    }
                }
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed() {
                    let mut needs_redraw = false;
                    match event.logical_key {
                        winit::keyboard::Key::Character(ref ch) if ch == "j" || ch == " " => {
                            let mut s = self.state.lock().unwrap();
                            if s.current_page + 1 < s.total_pages {
                                s.current_page += 1;
                                needs_redraw = true;
                            }
                        }
                        winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowDown)
                        | winit::keyboard::Key::Named(winit::keyboard::NamedKey::PageDown) => {
                            let mut s = self.state.lock().unwrap();
                            if s.current_page + 1 < s.total_pages {
                                s.current_page += 1;
                                needs_redraw = true;
                            }
                        }
                        winit::keyboard::Key::Character(ref ch) if ch == "k" => {
                            let mut s = self.state.lock().unwrap();
                            if s.current_page > 0 {
                                s.current_page -= 1;
                                needs_redraw = true;
                            }
                        }
                        winit::keyboard::Key::Named(winit::keyboard::NamedKey::ArrowUp)
                        | winit::keyboard::Key::Named(winit::keyboard::NamedKey::PageUp) => {
                            let mut s = self.state.lock().unwrap();
                            if s.current_page > 0 {
                                s.current_page -= 1;
                                needs_redraw = true;
                            }
                        }
                        winit::keyboard::Key::Character(ref ch) if ch == "d" => {
                            let mut s = self.state.lock().unwrap();
                            s.dark_mode = !s.dark_mode;
                            needs_redraw = true;
                        }
                        winit::keyboard::Key::Character(ref ch) if ch == "+" || ch == "=" => {
                            let mut s = self.state.lock().unwrap();
                            s.zoom = (s.zoom * 1.15).min(5.0);
                            needs_redraw = true;
                        }
                        winit::keyboard::Key::Character(ref ch) if ch == "-" => {
                            let mut s = self.state.lock().unwrap();
                            s.zoom = (s.zoom / 1.15).max(0.2);
                            needs_redraw = true;
                        }
                        winit::keyboard::Key::Character(ref ch) if ch == "q" => {
                            event_loop.exit();
                        }
                        winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape) => {
                            event_loop.exit();
                        }
                        _ => {}
                    }

                    if needs_redraw {
                        self.update_title();
                        if let Some(win) = &self.window {
                            win.request_redraw();
                        }
                    }
                }
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                let mut needs_redraw = false;
                match delta {
                    winit::event::MouseScrollDelta::LineDelta(_x, y) => {
                        let mut s = self.state.lock().unwrap();
                        if y < -0.1 && s.current_page + 1 < s.total_pages {
                            s.current_page += 1;
                            needs_redraw = true;
                        } else if y > 0.1 && s.current_page > 0 {
                            s.current_page -= 1;
                            needs_redraw = true;
                        }
                    }
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        let mut s = self.state.lock().unwrap();
                        if pos.y < -10.0 && s.current_page + 1 < s.total_pages {
                            s.current_page += 1;
                            needs_redraw = true;
                        } else if pos.y > 10.0 && s.current_page > 0 {
                            s.current_page -= 1;
                            needs_redraw = true;
                        }
                    }
                }

                if needs_redraw {
                    self.update_title();
                    if let Some(win) = &self.window {
                        win.request_redraw();
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[test]
    fn test_kinetic_scroll_viscous_decay() {
        let mut engine = KineticScrollEngine::new(2000.0, 500.0);
        assert_eq!(engine.position, 0.0);

        // Impart wheel impulse
        engine.add_wheel_impulse(2.0);
        assert_eq!(engine.velocity, 700.0);

        // Step 16ms (approx 60Hz frame)
        engine.step(0.016);
        assert!(engine.position > 0.0, "Position must advance with velocity");
        assert!(
            engine.velocity < 700.0,
            "Velocity must decay due to viscous drag"
        );

        // Run multiple frames until settled
        for _ in 0..200 {
            engine.step(0.016);
        }
        assert!(engine.is_settled(), "Engine must eventually settle");
    }

    #[test]
    fn test_kinetic_scroll_rubberband_overscroll_and_restitution() {
        let mut engine = KineticScrollEngine::new(1000.0, 500.0);
        assert_eq!(engine.max_position, 500.0);

        // Drag down past top boundary (< 0)
        engine.start_drag(0.0);
        engine.drag_to(200.0);
        // Overscroll position should be clamped asymptotically via tanh
        assert!(engine.position < 0.0);
        assert!(
            engine.position > -KineticScrollEngine::D_OVERSCROLL,
            "Overscroll cannot exceed asymptotic limit"
        );

        // Release pointer drag
        engine.release_drag(0.0);
        assert!(!engine.is_dragging);

        // Critically damped harmonic restitution snap-back
        for _ in 0..50 {
            engine.step(0.016);
        }
        assert_eq!(
            engine.position, 0.0,
            "Viewport must snap back cleanly to 0.0"
        );
        assert_eq!(engine.velocity, 0.0);
    }

    #[test]
    fn test_ipc_commands_and_status() {
        let state = Arc::new(Mutex::new(GuiState::new()));
        {
            let mut s = state.lock().unwrap();
            s.total_pages = 50;
        }

        let server = IpcServer::start_with_id(Arc::clone(&state), Some("test_ipc_cmd"))
            .expect("Start IPC server");
        assert!(server.socket_path.exists());

        // Connect client and send command
        let mut client = UnixStream::connect(&server.socket_path).expect("Connect to IPC socket");
        let cmd = IpcCommand::Goto { page: 25 };
        let cmd_bytes = serde_json::to_vec(&cmd).unwrap();
        client.write_all(&cmd_bytes).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();

        let mut resp = String::new();
        client.read_to_string(&mut resp).unwrap();
        let status: IpcStatus = serde_json::from_str(&resp).expect("Parse response");
        assert_eq!(status.page, 25);

        server.stop();
    }

    #[test]
    fn test_ipc_concurrent_spam() {
        let state = Arc::new(Mutex::new(GuiState::new()));
        {
            let mut s = state.lock().unwrap();
            s.total_pages = 500;
        }

        let server = IpcServer::start_with_id(Arc::clone(&state), Some("test_ipc_spam"))
            .expect("Start IPC server");
        let symlink = server.symlink_path.clone();

        let mut handles = Vec::new();
        for i in 1..=30 {
            let sym = symlink.clone();
            handles.push(std::thread::spawn(move || {
                if let Ok(mut client) = UnixStream::connect(&sym) {
                    let cmd = IpcCommand::Goto { page: i };
                    let cmd_bytes = serde_json::to_vec(&cmd).unwrap();
                    let _ = client.write_all(&cmd_bytes);
                    let _ = client.shutdown(std::net::Shutdown::Write);
                    let mut resp = String::new();
                    let _ = client.read_to_string(&mut resp);
                    let _ = serde_json::from_str::<IpcStatus>(&resp);
                }
            }));
        }

        for h in handles {
            let _ = h.join();
        }

        server.stop();
    }
}
