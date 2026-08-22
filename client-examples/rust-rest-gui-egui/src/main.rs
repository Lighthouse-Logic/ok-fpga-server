//! # Counters.bit Interactive Demo — Rust REST + egui
//!
//! This demo provides a GUI for real-time control of the Counters.bit hardware
//! test running on an Opal Kelly FPGA, using the ok-FPGA-Server REST API.
//!
//! ## User Identity: "Rusty Rester"
//!
//! ## Features
//!
//! - User-driven connection sequence (Discover → Connect → Program → Register → Acquire)
//! - Counter #1 panel: RESET1, DISABLE1 controls + value display + trigger indicators
//! - Counter #2 panel: Count Up/Down/Reset buttons, AUTOCOUNT2 + value display + triggers
//! - On-board button status display
//! - Configurable poll interval (100ms – 2000ms)
//! - Resource locking with "Override lockout" checkbox
//! - Shutdown prompt: release resource or leave held (for timeout testing)
//!
//! ## Running
//!
//! ```bash
//! # Set bitfile path (optional — has a default)
//! set OKFPGA_TEST_BITFILE=..\..\FrontPanel-6.1.0\ExampleBitfiles\...\Counters.bit
//!
//! # Run the GUI
//! cargo run
//! ```

use eframe::egui;
use rust_rest_gui_common::{
    CounterState, CountersClient, DeviceInfo, ResourceInfo, WireInState,
    mdns_browser::DiscoveredServer,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Default REST API URL — matches config.toml [rest] port.
const DEFAULT_REST_URL: &str = "http://localhost:8088";

/// Default bitfile path (relative to the server's working directory).
const DEFAULT_BITFILE: &str =
    "FrontPanel-6.1.0/ExampleBitfiles/20190501-FP50-XEM7320-A75/XEM7320-A75/Counters.bit";

/// User identity for session tracking.
const USER_ID: &str = "Rusty Rester";

fn main() -> eframe::Result<()> {
    // Initialize tracing for debug logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 560.0])
            .with_title("Counters.bit — Rusty Rester (REST + egui)"),
        ..Default::default()
    };
    eframe::run_native(
        "Counters.bit — Rusty Rester",
        options,
        Box::new(|cc| Ok(Box::new(CountersApp::new(cc)))),
    )
}

// ============================================================================
// Application State
// ============================================================================

/// The phases of the user-controlled connection sequence.
#[derive(Debug, Clone, PartialEq)]
enum ConnectionPhase {
    /// Initial state — user can edit server URL and click Connect.
    Disconnected,
    /// Connected to server, devices listed.
    Connected,
    /// FPGA programmed with Counters.bit.
    Programmed,
    /// Resource registered on the server.
    Registered,
    /// Resource acquired — counter control panel is active.
    Acquired,
}

/// Shared state that is updated from the async polling task.
///
/// Protected by a `Mutex` so the GUI thread and the polling task can
/// both access it safely. The GUI reads it each frame; the polling task
/// writes it periodically.
#[derive(Debug, Clone)]
struct SharedState {
    counter: CounterState,
    resource: Option<ResourceInfo>,
    last_error: Option<String>,
    last_poll: Option<Instant>,
}

/// Activity log entry for the bottom panel.
#[derive(Debug, Clone)]
struct LogEntry {
    time: String,
    message: String,
    is_error: bool,
}

/// Bitfile configuration mode — server-side path or client-side upload (F5).
#[derive(Debug, Clone, Copy, PartialEq)]
enum BitfileMode {
    /// Send the path to the server (server reads the file).
    ServerPath,
    /// Read the file locally and upload as binary.
    LocalFile,
}

/// Main application state for the egui GUI.
struct CountersApp {
    // --- Connection settings ---
    rest_url: String,
    bitfile_path: String,
    /// Whether to send the bitfile path or upload local file (F5).
    bitfile_mode: BitfileMode,
    phase: ConnectionPhase,

    // --- Device / resource state ---
    devices: Vec<DeviceInfo>,
    selected_serial: Option<String>,
    resource_id: Option<String>,

    // --- Counter control ---
    wire_in: WireInState,
    poll_interval_ms: u64,
    override_lockout: bool,

    // --- Shared state (written by polling task, read by GUI) ---
    shared: Arc<Mutex<SharedState>>,

    // --- Async runtime ---
    runtime: tokio::runtime::Runtime,

    // --- Activity log ---
    log: Vec<LogEntry>,

    // --- Polling control ---
    last_poll_time: Instant,

    // --- Device refresh (5-second periodic re-query of /devices) ---
    last_device_refresh: Instant,

    // --- Quit / disconnect dialogs ---
    show_quit_dialog: bool,
    show_disconnect_dialog: bool,
    /// Discovered ok-FPGA servers via mDNS (Phase 28A).
    discovered_servers: Vec<DiscoveredServer>,
    /// True when the user acquired as ReadOnly (Phase 30, ADR-026).
    acquired_readonly: bool,
}

impl CountersApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let bitfile = std::env::var("OKFPGA_TEST_BITFILE")
            .unwrap_or_else(|_| DEFAULT_BITFILE.to_string());

        let mut app = Self {
            rest_url: DEFAULT_REST_URL.to_string(),
            bitfile_path: bitfile,
            bitfile_mode: BitfileMode::ServerPath,
            phase: ConnectionPhase::Disconnected,
            devices: vec![],
            selected_serial: None,
            resource_id: None,
            wire_in: WireInState::default(),
            poll_interval_ms: 500,
            override_lockout: false,
            shared: Arc::new(Mutex::new(SharedState {
                counter: CounterState::default(),
                resource: None,
                last_error: None,
                last_poll: None,
            })),
            runtime: tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime"),
            log: vec![],
            last_poll_time: Instant::now(),
            last_device_refresh: Instant::now(),
            show_quit_dialog: false,
            show_disconnect_dialog: false,
            discovered_servers: vec![],
            acquired_readonly: false,
        };

        // Start mDNS discovery in background.
        let runtime = &app.runtime;
        let servers = runtime.block_on(async {
            match rust_rest_gui_common::mdns_browser::MdnsBrowser::new() {
                Ok(browser) => {
                    let found = browser.browse_for(Duration::from_secs(3)).await;
                    browser.stop();
                    found
                }
                Err(_) => vec![],
            }
        });
        if !servers.is_empty() {
            app.log(&format!("mDNS: discovered {} server(s)", servers.len()));
            app.discovered_servers = servers;
        }

        app
    }

    /// Add a message to the activity log.
    fn log(&mut self, msg: &str) {
        let now = chrono_now();
        self.log.push(LogEntry {
            time: now,
            message: msg.to_string(),
            is_error: false,
        });
        // Keep log from growing unbounded.
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    /// Add an error message to the activity log.
    fn log_error(&mut self, msg: &str) {
        let now = chrono_now();
        self.log.push(LogEntry {
            time: now,
            message: msg.to_string(),
            is_error: true,
        });
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    /// Create the API client with current settings.
    fn make_client(&self) -> CountersClient {
        CountersClient::new(&self.rest_url, USER_ID)
    }

    /// Get the selected device serial, or empty string.
    fn serial(&self) -> String {
        self.selected_serial.clone().unwrap_or_default()
    }

    /// Check if controls should be disabled due to resource lock by another user.
    fn is_locked_out(&self) -> bool {
        if self.override_lockout {
            return false;
        }
        let shared = self.shared.lock().unwrap();
        if let Some(ref res) = shared.resource {
            if let Some(ref locker) = res.locked_by {
                return locker != USER_ID;
            }
        }
        false
    }

    /// Get the name of the user who holds the lock, if any.
    fn locked_by_name(&self) -> Option<String> {
        let shared = self.shared.lock().unwrap();
        shared.resource.as_ref().and_then(|r| r.locked_by.clone())
    }
}

/// Get current time as a formatted string (without pulling in chrono crate).
fn chrono_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs() % 86400;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

// ============================================================================
// egui UI Rendering
// ============================================================================

impl eframe::App for CountersApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request repaint at the poll interval so we keep updating.
        ctx.request_repaint_after(Duration::from_millis(self.poll_interval_ms.min(100)));

        // --- Polling: read counter state periodically ---
        if self.phase == ConnectionPhase::Acquired
            && self.last_poll_time.elapsed() >= Duration::from_millis(self.poll_interval_ms)
        {
            self.last_poll_time = Instant::now();
            let client = self.make_client();
            let serial = self.serial();
            let shared = self.shared.clone();
            let resource_id = self.resource_id.clone();

            self.runtime.spawn(async move {
                // Poll counter state.
                match client.poll_counter_state(&serial).await {
                    Ok(state) => {
                        let mut s = shared.lock().unwrap();
                        s.counter = state;
                        s.last_error = None;
                        s.last_poll = Some(Instant::now());
                    }
                    Err(e) => {
                        let mut s = shared.lock().unwrap();
                        s.last_error = Some(format!("Poll error: {}", e));
                    }
                }
                // Also refresh resource info (holders, lock status).
                if let Some(rid) = resource_id {
                    if let Ok(info) = client.get_resource(&rid).await {
                        let mut s = shared.lock().unwrap();
                        s.resource = Some(info);
                    }
                }
            });
        }

        // --- Device refresh: re-query /devices every 5s when connected ---
        if self.phase != ConnectionPhase::Disconnected
            && self.last_device_refresh.elapsed() >= Duration::from_secs(5)
        {
            self.last_device_refresh = Instant::now();
            let client = self.make_client();
            match self.runtime.block_on(client.list_devices()) {
                Ok(new_devices) => {
                    // Check if the selected device has disappeared.
                    if let Some(ref serial) = self.selected_serial {
                        let still_present = new_devices.iter().any(|d| &d.serial == serial);
                        if !still_present {
                            let lost_serial = serial.clone();
                            self.selected_serial = None;
                            if self.phase == ConnectionPhase::Acquired {
                                // Device disconnected during active use.
                                self.log_error(&format!("Device {} disconnected from server", lost_serial));
                                self.phase = ConnectionPhase::Connected;
                                self.resource_id = None;
                                self.show_disconnect_dialog = true;
                                // Reset shared state.
                                let mut s = self.shared.lock().unwrap();
                                s.counter = CounterState::default();
                                s.resource = None;
                                s.last_error = None;
                            } else {
                                self.log_error(&format!("Device {} is no longer available", lost_serial));
                            }
                        }
                    }
                    self.devices = new_devices;
                }
                Err(e) => {
                    self.log_error(&format!("Device refresh failed: {}", e));
                }
            }
        }

        // --- Top panel: title bar ---
        egui::TopBottomPanel::top("title_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Counters.bit — Rusty Rester");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Quit").clicked() {
                        if self.phase == ConnectionPhase::Acquired {
                            self.show_quit_dialog = true;
                        } else {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                });
            });

            // Status line.
            ui.horizontal(|ui| {
                if let Some(ref serial) = self.selected_serial {
                    ui.label(format!("Device: {}", serial));
                    ui.separator();
                }
                ui.label(format!("Phase: {:?}", self.phase));
                if self.phase == ConnectionPhase::Acquired {
                    ui.separator();
                    ui.label(format!("Polling: {}ms", self.poll_interval_ms));
                }
                // Show holders.
                let shared = self.shared.lock().unwrap();
                if let Some(ref res) = shared.resource {
                    if !res.holders.is_empty() {
                        ui.separator();
                        ui.label(format!("Holders: {}", res.holders.join(", ")));
                    }
                    if let Some(ref locker) = res.locked_by {
                        ui.separator();
                        ui.colored_label(egui::Color32::YELLOW, format!("🔒 Locked by: {}", locker));
                    }
                }
            });
        });

        // --- Bottom panel: activity log ---
        egui::TopBottomPanel::bottom("log_panel")
            .min_height(80.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Activity Log");
                    if ui.button("Clear").clicked() {
                        self.log.clear();
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for entry in &self.log {
                            let color = if entry.is_error {
                                egui::Color32::from_rgb(255, 100, 100)
                            } else {
                                egui::Color32::from_rgb(180, 180, 180)
                            };
                            ui.colored_label(color, format!("[{}] {}", entry.time, entry.message));
                        }
                    });
            });

        // --- Central panel ---
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.phase {
                ConnectionPhase::Disconnected
                | ConnectionPhase::Connected
                | ConnectionPhase::Programmed
                | ConnectionPhase::Registered => {
                    self.draw_connection_panel(ui);
                }
                ConnectionPhase::Acquired => {
                    self.draw_counter_panel(ui);
                }
            }
        });

        // --- Quit confirmation dialog ---
        if self.show_quit_dialog {
            egui::Window::new("Release resource before exit?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Release the Counter Resource before quitting?");
                    ui.label("Choosing 'No' allows testing the server's session timeout.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Yes — Release").clicked() {
                            // Phase 25-Fix-H (F8+F10): Use disconnect() for
                            // clean session + resource cleanup.
                            let client = self.make_client();
                            self.runtime.block_on(async {
                                let _ = client.disconnect().await;
                            });
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("No — Leave Held").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
        }

        // --- Device disconnect dialog ---
        if self.show_disconnect_dialog {
            egui::Window::new("⚠ Device Disconnected")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("The selected device has been disconnected from the server.");
                    ui.label("The Counter Control Panel has been closed.");
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() {
                        self.show_disconnect_dialog = false;
                    }
                });
        }
    }
}

// ============================================================================
// Connection Panel
// ============================================================================

impl CountersApp {
    fn draw_connection_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);

        // Phase 28A: mDNS discovered servers dropdown.
        if !self.discovered_servers.is_empty() {
            ui.horizontal(|ui| {
                ui.label("Discovered (mDNS):");
                let mut selected_label = "Select a discovered server...".to_string();
                egui::ComboBox::from_id_salt("mdns_servers")
                    .selected_text(&selected_label)
                    .show_ui(ui, |ui| {
                        for server in &self.discovered_servers {
                            let port_str = server.rest_port.map(|p| format!(":{}", p)).unwrap_or_default();
                            let label = format!("{} — http://{}{}" , server.description, server.host, port_str);
                            let url = format!("http://{}{}", server.host, port_str);
                            if ui.selectable_label(false, &label).clicked() {
                                self.rest_url = url;
                            }
                        }
                    });
            });
            ui.add_space(4.0);
        }

        // Server URL input.
        ui.horizontal(|ui| {
            ui.label("Server URL:");
            ui.text_edit_singleline(&mut self.rest_url);
        });

        ui.add_space(4.0);

        // Step 1: Connect to Server.
        let can_connect = self.phase == ConnectionPhase::Disconnected;
        if ui.add_enabled(can_connect, egui::Button::new("1. Connect to Server")).clicked() {
            let client = self.make_client();
            match self.runtime.block_on(async {
                let _status = client.get_server_status().await?;
                client.list_devices().await
            }) {
                Ok(devices) => {
                    self.log(&format!("Connected. Found {} device(s)", devices.len()));
                    self.devices = devices;
                    self.selected_serial = None; // Force user to select
                    self.phase = ConnectionPhase::Connected;
                    self.last_device_refresh = Instant::now();
                }
                Err(e) => self.log_error(&format!("Connect failed: {}", e)),
            }
        }

        // Device selection dropdown.
        if !self.devices.is_empty() {
            ui.add_space(4.0);
            let mut newly_selected: Option<String> = None;
            ui.horizontal(|ui| {
                ui.label("Select Device:");
                let display = self.selected_serial.as_ref()
                    .and_then(|s| self.devices.iter().find(|d| &d.serial == s))
                    .map(|d| format!("{} — {}", d.serial, d.product_name))
                    .unwrap_or_else(|| "No device selected".to_string());
                egui::ComboBox::from_id_salt("device_select")
                    .selected_text(display)
                    .width(400.0)
                    .show_ui(ui, |ui| {
                        for d in &self.devices {
                            let label = format!("{} — {}", d.serial, d.product_name);
                            let is_selected = self.selected_serial.as_deref() == Some(&d.serial);
                            if ui.selectable_label(is_selected, &label).clicked() {
                                newly_selected = Some(d.serial.clone());
                            }
                        }
                    });
            });
            if let Some(serial) = newly_selected {
                self.log(&format!("Selected device: {}", serial));
                self.selected_serial = Some(serial.clone());

                // F4: Smart-skip — probe device state and skip phases if
                // the FPGA is already programmed or a resource exists.
                let client = self.make_client();
                match self.runtime.block_on(async {
                    let dev_info = client.get_device_info(&serial).await.map_err(|e| e.to_string())?;
                    let is_programmed = dev_info["state"].as_str()
                        .map(|s| s.contains("Programmed"))
                        .unwrap_or(false);
                    let resources = client.list_resources().await.map_err(|e| e.to_string())?;
                    let existing = resources.into_iter().find(|r| r.device_serial == serial);
                    Ok::<_, String>((is_programmed, existing))
                }) {
                    Ok((true, Some(resource))) => {
                        self.log(&format!(
                            "Device already programmed, resource '{}' available — skipping to Registered",
                            resource.name
                        ));
                        self.resource_id = Some(resource.id);
                        self.phase = ConnectionPhase::Registered;
                    }
                    Ok((true, None)) => {
                        self.log("Device already programmed — skipping to Register");
                        self.phase = ConnectionPhase::Programmed;
                    }
                    Ok((false, _)) => {
                        // Not programmed — normal flow.
                    }
                    Err(e) => {
                        self.log(&format!("Could not check device state: {}", e));
                    }
                }
            }
        }

        ui.add_space(4.0);

        // Step 2: Program Bitfile.
        let can_program = self.phase == ConnectionPhase::Connected && self.selected_serial.is_some();
        ui.horizontal(|ui| {
            ui.label("Bitfile:");
            ui.text_edit_singleline(&mut self.bitfile_path);
        });

        // F5: Bitfile mode toggle — Server Path vs Local File.
        ui.horizontal(|ui| {
            ui.label("Mode:");
            ui.radio_value(&mut self.bitfile_mode, BitfileMode::ServerPath, "Server Path");
            ui.radio_value(&mut self.bitfile_mode, BitfileMode::LocalFile, "Local File");
        });

        if ui.add_enabled(can_program, egui::Button::new("2. Program Bitfile")).clicked() {
            let client = self.make_client();
            let serial = self.serial();
            let bitfile = self.bitfile_path.clone();
            let mode = self.bitfile_mode;
            // F5: Branch on bitfile mode.
            let result = self.runtime.block_on(async {
                match mode {
                    BitfileMode::ServerPath => {
                        client.configure_fpga(&serial, &bitfile).await
                            .map(|_| ()).map_err(|e| e.to_string())
                    }
                    BitfileMode::LocalFile => {
                        client.configure_fpga_from_file(&serial, std::path::Path::new(&bitfile)).await
                            .map(|_| ()).map_err(|e| e.to_string())
                    }
                }
            });
            match result {
                Ok(()) => {
                    self.log(&format!("FPGA programmed: {} ({:?})", bitfile, mode));
                    self.phase = ConnectionPhase::Programmed;
                }
                Err(e) => self.log_error(&format!("Program failed: {}", e)),
            }
        }

        ui.add_space(4.0);

        // Step 3: Register Resource (Phase 34-E: Shared or Exclusive).
        let can_register = self.phase == ConnectionPhase::Programmed;
        let register_shared = ui.add_enabled(can_register, egui::Button::new("3. Register (Shared)")).clicked();
        let register_exclusive = ui.add_enabled(can_register, egui::Button::new("Register (Exclusive)")).clicked();
        if register_shared || register_exclusive {
            let resource_type = if register_exclusive { "exclusive" } else { "shared" };
            let client = self.make_client();
            let serial = self.serial();
            match self.runtime.block_on(client.create_resource("Counter Resource", &serial, resource_type)) {
                Ok(info) => {
                    self.log(&format!("Resource registered ({}): {}", resource_type, info.id));
                    self.resource_id = Some(info.id);
                    self.phase = ConnectionPhase::Registered;
                }
                Err(e) => {
                    let err_str = e.to_string();
                    // F3: 409 Conflict — resource already exists. Look it up.
                    if err_str.contains("409") || err_str.contains("already exists") {
                        self.log("Resource already exists — looking up...");
                        let client2 = self.make_client();
                        match self.runtime.block_on(client2.list_resources()) {
                            Ok(resources) => {
                                if let Some(existing) = resources.into_iter().find(|r| r.device_serial == serial) {
                                    self.log(&format!("Found existing resource: {}", existing.id));
                                    self.resource_id = Some(existing.id);
                                    self.phase = ConnectionPhase::Registered;
                                } else {
                                    self.log_error("No matching resource found");
                                }
                            }
                            Err(e2) => self.log_error(&format!("Lookup failed: {}", e2)),
                        }
                    } else {
                        self.log_error(&format!("Register failed: {}", err_str));
                    }
                }
            }
        }

        ui.add_space(4.0);

        // Step 4: Acquire Resource.
        let can_acquire = self.phase == ConnectionPhase::Registered && self.resource_id.is_some();
        if ui.add_enabled(can_acquire, egui::Button::new("4. Acquire (Shared)")).clicked() {
            self.acquired_readonly = false;
            let client = self.make_client();
            let rid = self.resource_id.clone().unwrap();
            match self.runtime.block_on(client.acquire_resource(&rid)) {
                Ok(_) => {
                    self.log("Resource acquired (shared) — entering counter control mode");
                    self.phase = ConnectionPhase::Acquired;
                }
                Err(e) => self.log_error(&format!("Acquire failed: {}", e)),
            }
        }

        // Step 4b: Acquire Read-Only (Phase 30, ADR-026).
        if ui.add_enabled(can_acquire, egui::Button::new("Acquire R/O")).clicked() {
            self.acquired_readonly = true;
            let client = self.make_client();
            let rid = self.resource_id.clone().unwrap();
            match self.runtime.block_on(client.acquire_resource_readonly(&rid)) {
                Ok(_) => {
                    self.log("Resource acquired (read-only) — entering counter control mode");
                    self.phase = ConnectionPhase::Acquired;
                }
                Err(e) => self.log_error(&format!("Acquire R/O failed: {}", e)),
            }
        }
    }

    // ========================================================================
    // Counter Control Panel
    // ========================================================================

    fn draw_counter_panel(&mut self, ui: &mut egui::Ui) {
        // ReadOnly holders are treated like locked-out users for write controls.
        let locked_out = self.is_locked_out() || self.acquired_readonly;

        if self.acquired_readonly {
            ui.colored_label(egui::Color32::YELLOW, "📖 Read-Only Mode — write controls disabled");
        }

        // Read current counter state from shared data.
        let (counter, last_error) = {
            let s = self.shared.lock().unwrap();
            (s.counter.clone(), s.last_error.clone())
        };

        if let Some(err) = &last_error {
            ui.colored_label(egui::Color32::RED, err);
        }

        // Lock warning banner.
        if let Some(locker) = self.locked_by_name() {
            if locker != USER_ID {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!("⚠ Resource locked by '{}' — controls disabled", locker),
                );
            }
        }

        ui.columns(2, |cols| {
            // ---- Counter #1 (left panel) ----
            cols[0].group(|ui| {
                ui.heading("Counter #1 (Auto-count)");
                ui.add_space(4.0);

                // Value display with progress bar.
                let pct = counter.count1 as f32 / 255.0;
                ui.label(format!("Value: 0x{:02X} ({})", counter.count1, counter.count1));
                ui.add(egui::ProgressBar::new(pct).show_percentage());

                ui.add_space(8.0);

                // RESET1 checkbox.
                let mut reset1 = self.wire_in.reset1;
                if ui.add_enabled(!locked_out, egui::Checkbox::new(&mut reset1, "RESET1 (hold at 0x00)")).changed() {
                    self.wire_in.reset1 = reset1;
                    self.send_wire_in();
                }

                // DISABLE1 checkbox.
                let mut disable1 = self.wire_in.disable1;
                if ui.add_enabled(!locked_out, egui::Checkbox::new(&mut disable1, "DISABLE1 (freeze)")).changed() {
                    self.wire_in.disable1 = disable1;
                    self.send_wire_in();
                }

                ui.add_space(8.0);
                ui.label("Triggers:");
                Self::draw_trigger_indicator(ui, "COUNT1 == 0x00", counter.count1_eq_00);
                Self::draw_trigger_indicator(ui, "COUNT1 == 0x80", counter.count1_eq_80);
            });

            // ---- Counter #2 (right panel) ----
            cols[1].group(|ui| {
                ui.heading("Counter #2 (Manual)");
                ui.add_space(4.0);

                // Value display with progress bar.
                let pct = counter.count2 as f32 / 255.0;
                ui.label(format!("Value: 0x{:02X} ({})", counter.count2, counter.count2));
                ui.add(egui::ProgressBar::new(pct).show_percentage());

                ui.add_space(8.0);

                // Control buttons.
                ui.horizontal(|ui| {
                    if ui.add_enabled(!locked_out, egui::Button::new("▲ Count Up")).clicked() {
                        self.fire_trigger(0x40, 1); // UP2
                    }
                    if ui.add_enabled(!locked_out, egui::Button::new("▼ Count Down")).clicked() {
                        self.fire_trigger(0x40, 2); // DOWN2
                    }
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(!locked_out, egui::Button::new("⟲ Reset")).clicked() {
                        self.fire_trigger(0x40, 0); // RESET2
                    }
                    // AUTOCOUNT2 checkbox.
                    let mut autocount2 = self.wire_in.autocount2;
                    if ui.add_enabled(!locked_out, egui::Checkbox::new(&mut autocount2, "AUTOCOUNT2")).changed() {
                        self.wire_in.autocount2 = autocount2;
                        self.send_wire_in();
                    }
                });

                ui.add_space(8.0);
                ui.label("Triggers:");
                Self::draw_trigger_indicator(ui, "COUNT2 == 0xFF", counter.count2_eq_ff);
            });
        });

        ui.add_space(8.0);

        // --- Bottom controls: buttons, poll interval, lock ---
        ui.horizontal(|ui| {
            ui.label("Buttons:");
            for i in 0..4 {
                let pressed = (counter.buttons >> i) & 1 != 0;
                let color = if pressed {
                    egui::Color32::from_rgb(0, 200, 100)
                } else {
                    egui::Color32::from_rgb(80, 80, 80)
                };
                ui.colored_label(color, format!("[{}]", i + 1));
            }

            ui.separator();

            ui.label("Poll:");
            egui::ComboBox::from_id_salt("poll_interval")
                .selected_text(format!("{}ms", self.poll_interval_ms))
                .show_ui(ui, |ui| {
                    for ms in [100, 250, 500, 1000, 2000] {
                        ui.selectable_value(&mut self.poll_interval_ms, ms, format!("{}ms", ms));
                    }
                });
        });

        ui.horizontal(|ui| {
            // Lock/Unlock button.
            let locked_by_me = self.locked_by_name().as_deref() == Some(USER_ID);
            if locked_by_me {
                if ui.button("🔓 Unlock Resource").clicked() {
                    if let Some(ref rid) = self.resource_id {
                        let client = self.make_client();
                        let rid = rid.clone();
                        match self.runtime.block_on(client.unlock_resource(&rid)) {
                            Ok(_) => self.log("Resource unlocked"),
                            Err(e) => self.log_error(&format!("Unlock failed: {}", e)),
                        }
                    }
                }
            } else {
                if ui.button("🔒 Lock Resource").clicked() {
                    if let Some(ref rid) = self.resource_id {
                        let client = self.make_client();
                        let rid = rid.clone();
                        match self.runtime.block_on(client.lock_resource(&rid)) {
                            Ok(_) => self.log("Resource locked"),
                            Err(e) => self.log_error(&format!("Lock failed: {}", e)),
                        }
                    }
                }
            }

            // Override lockout checkbox.
            ui.checkbox(&mut self.override_lockout, "Override lockout");
        });
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    /// Draw a trigger indicator (green dot for active, dark dot for inactive).
    fn draw_trigger_indicator(ui: &mut egui::Ui, label: &str, active: bool) {
        ui.horizontal(|ui| {
            let color = if active {
                egui::Color32::from_rgb(0, 220, 100)
            } else {
                egui::Color32::from_rgb(60, 60, 60)
            };
            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 5.0, color);
            ui.label(label);
        });
    }

    /// Send the current WireIn state to the FPGA.
    fn send_wire_in(&mut self) {
        let client = self.make_client();
        let serial = self.serial();
        let wire_in = self.wire_in.clone();
        match self.runtime.block_on(client.send_wire_in(&serial, &wire_in)) {
            Ok(()) => self.log(&format!("WireIn 0x00 = 0x{:04X}", wire_in.to_value())),
            Err(e) => self.log_error(&format!("WireIn failed: {}", e)),
        }
    }

    /// Fire a trigger (one-shot event).
    fn fire_trigger(&mut self, endpoint: u8, bit: u8) {
        let client = self.make_client();
        let serial = self.serial();
        match self.runtime.block_on(client.activate_trigger_in(&serial, endpoint, bit)) {
            Ok(_) => self.log(&format!("TriggerIn 0x{:02X} bit {} fired", endpoint, bit)),
            Err(e) => self.log_error(&format!("TriggerIn failed: {}", e)),
        }
    }
}
