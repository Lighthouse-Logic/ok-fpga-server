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
use rust_rest_gui_common::{CounterState, CountersClient, DeviceInfo, ResourceInfo, WireInState};
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

/// Main application state for the egui GUI.
struct CountersApp {
    // --- Connection settings ---
    rest_url: String,
    bitfile_path: String,
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

    // --- Quit confirmation ---
    show_quit_dialog: bool,
}

impl CountersApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let bitfile = std::env::var("OKFPGA_TEST_BITFILE")
            .unwrap_or_else(|_| DEFAULT_BITFILE.to_string());

        Self {
            rest_url: DEFAULT_REST_URL.to_string(),
            bitfile_path: bitfile,
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
            show_quit_dialog: false,
        }
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
                            // Release resource, then close.
                            if let Some(ref rid) = self.resource_id {
                                let client = self.make_client();
                                let rid = rid.clone();
                                self.runtime.block_on(async {
                                    let _ = client.release_resource(&rid).await;
                                });
                            }
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("No — Leave Held").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
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
                    if let Some(first) = devices.first() {
                        self.selected_serial = Some(first.serial.clone());
                        self.log(&format!("Selected device: {} ({})", first.serial, first.product_name));
                    }
                    self.devices = devices;
                    self.phase = ConnectionPhase::Connected;
                }
                Err(e) => self.log_error(&format!("Connect failed: {}", e)),
            }
        }

        // Show devices if connected.
        if !self.devices.is_empty() {
            ui.add_space(4.0);
            ui.label("Connected devices:");
            let mut newly_selected: Option<String> = None;
            for d in &self.devices {
                let selected = self.selected_serial.as_deref() == Some(&d.serial);
                if ui.selectable_label(selected, format!("{} — {}", d.serial, d.product_name)).clicked() {
                    newly_selected = Some(d.serial.clone());
                }
            }
            if let Some(serial) = newly_selected {
                self.log(&format!("Selected device: {}", serial));
                self.selected_serial = Some(serial);
            }
        }

        ui.add_space(4.0);

        // Step 2: Program Bitfile.
        let can_program = self.phase == ConnectionPhase::Connected && self.selected_serial.is_some();
        ui.horizontal(|ui| {
            ui.label("Bitfile:");
            ui.text_edit_singleline(&mut self.bitfile_path);
        });
        if ui.add_enabled(can_program, egui::Button::new("2. Program Bitfile")).clicked() {
            let client = self.make_client();
            let serial = self.serial();
            let bitfile = self.bitfile_path.clone();
            match self.runtime.block_on(client.configure_fpga(&serial, &bitfile)) {
                Ok(_) => {
                    self.log(&format!("FPGA programmed: {}", bitfile));
                    self.phase = ConnectionPhase::Programmed;
                }
                Err(e) => self.log_error(&format!("Program failed: {}", e)),
            }
        }

        ui.add_space(4.0);

        // Step 3: Register Resource.
        let can_register = self.phase == ConnectionPhase::Programmed;
        if ui.add_enabled(can_register, egui::Button::new("3. Register Resource")).clicked() {
            let client = self.make_client();
            let serial = self.serial();
            match self.runtime.block_on(client.create_resource("Counter Resource", &serial)) {
                Ok(info) => {
                    self.log(&format!("Resource registered: {}", info.id));
                    self.resource_id = Some(info.id);
                    self.phase = ConnectionPhase::Registered;
                }
                Err(e) => self.log_error(&format!("Register failed: {}", e)),
            }
        }

        ui.add_space(4.0);

        // Step 4: Acquire Resource.
        let can_acquire = self.phase == ConnectionPhase::Registered && self.resource_id.is_some();
        if ui.add_enabled(can_acquire, egui::Button::new("4. Acquire Resource (Shared)")).clicked() {
            let client = self.make_client();
            let rid = self.resource_id.clone().unwrap();
            match self.runtime.block_on(client.acquire_resource(&rid)) {
                Ok(_) => {
                    self.log("Resource acquired — entering counter control mode");
                    self.phase = ConnectionPhase::Acquired;
                }
                Err(e) => self.log_error(&format!("Acquire failed: {}", e)),
            }
        }
    }

    // ========================================================================
    // Counter Control Panel
    // ========================================================================

    fn draw_counter_panel(&mut self, ui: &mut egui::Ui) {
        let locked_out = self.is_locked_out();

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
