// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # Counters.bit Interactive Demo — Rust gRPC + egui
//!
//! Same UI as the REST+egui demo but communicates via gRPC (tonic).
//!
//! ## User Identity: "Rusty Garpac"

use eframe::egui;
use rust_grpc_gui_common::{
    CounterState, CountersGrpcClient, DeviceInfo, ResourceInfo, WireInState,
    mdns_browser::DiscoveredServer,
};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as TokioMutex;

const DEFAULT_GRPC_URL: &str = "http://localhost:50051";
const DEFAULT_BITFILE: &str =
    "FrontPanel-6.1.0/ExampleBitfiles/20190501-FP50-XEM7320-A75/XEM7320-A75/Counters.bit";
const USER_ID: &str = "Rusty Garpac";

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 560.0])
            .with_title("Counters.bit — Rusty Garpac (gRPC + egui)"),
        ..Default::default()
    };
    eframe::run_native(
        "Counters.bit — Rusty Garpac",
        options,
        Box::new(|cc| Ok(Box::new(CountersApp::new(cc)))),
    )
}

#[derive(Debug, Clone, PartialEq)]
enum Phase { Disconnected, Connected, Programmed, Registered, Acquired }

#[derive(Debug, Clone)]
struct SharedState {
    counter: CounterState,
    resource: Option<ResourceInfo>,
    last_error: Option<String>,
}

/// Bitfile configuration mode — server-side path or client-side upload (F5).
#[derive(Debug, Clone, Copy, PartialEq)]
enum BitfileMode {
    /// Send the path to the server (server reads the file).
    ServerPath,
    /// Read the file locally and stream via ConfigureFPGAFromMemory.
    LocalFile,
}

struct CountersApp {
    grpc_url: String,
    bitfile_path: String,
    /// Whether to send the bitfile path or upload local file (F5).
    bitfile_mode: BitfileMode,
    phase: Phase,
    devices: Vec<DeviceInfo>,
    selected_serial: Option<String>,
    resource_id: Option<String>,
    wire_in: WireInState,
    poll_interval_ms: u64,
    override_lockout: bool,
    shared: Arc<StdMutex<SharedState>>,
    runtime: tokio::runtime::Runtime,
    log: Vec<(String, String, bool)>,
    last_poll_time: Instant,
    last_device_refresh: Instant,
    show_quit_dialog: bool,
    show_disconnect_dialog: bool,
    /// gRPC client behind a tokio::Mutex so MutexGuard can be held across .await.
    client: Option<Arc<TokioMutex<CountersGrpcClient>>>,
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
            grpc_url: DEFAULT_GRPC_URL.to_string(),
            bitfile_path: bitfile,
            bitfile_mode: BitfileMode::ServerPath,
            phase: Phase::Disconnected,
            devices: vec![],
            selected_serial: None,
            resource_id: None,
            wire_in: WireInState::default(),
            poll_interval_ms: 500,
            override_lockout: false,
            shared: Arc::new(StdMutex::new(SharedState {
                counter: CounterState::default(), resource: None, last_error: None,
            })),
            runtime: tokio::runtime::Runtime::new().expect("Tokio runtime"),
            log: vec![],
            last_poll_time: Instant::now(),
            last_device_refresh: Instant::now(),
            show_quit_dialog: false,
            show_disconnect_dialog: false,
            client: None,
            discovered_servers: vec![],
            acquired_readonly: false,
        };

        // Start mDNS discovery in background.
        let runtime = &app.runtime;
        let servers = runtime.block_on(async {
            match rust_grpc_gui_common::mdns_browser::MdnsBrowser::new() {
                Ok(browser) => {
                    let found = browser.browse_for(Duration::from_secs(3)).await;
                    browser.stop();
                    found
                }
                Err(_) => vec![],
            }
        });
        if !servers.is_empty() {
            app.add_log(&format!("mDNS: discovered {} server(s)", servers.len()), false);
            app.discovered_servers = servers;
        }

        app
    }

    fn add_log(&mut self, msg: &str, is_err: bool) {
        let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let s = d.as_secs() % 86400;
        let time = format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60);
        self.log.push((time, msg.to_string(), is_err));
        if self.log.len() > 200 { self.log.remove(0); }
    }

    fn serial(&self) -> String { self.selected_serial.clone().unwrap_or_default() }

    fn is_locked_out(&self) -> bool {
        if self.override_lockout { return false; }
        let s = self.shared.lock().unwrap();
        s.resource.as_ref().and_then(|r| r.locked_by.as_deref()).map(|l| l != USER_ID).unwrap_or(false)
    }
}

impl eframe::App for CountersApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(self.poll_interval_ms.min(100)));

        // Polling — spawns a tokio task that uses TokioMutex (Send-safe).
        if self.phase == Phase::Acquired
            && self.last_poll_time.elapsed() >= Duration::from_millis(self.poll_interval_ms)
        {
            self.last_poll_time = Instant::now();
            if let Some(ref ca) = self.client {
                let ca = ca.clone();
                let serial = self.serial();
                let shared = self.shared.clone();
                let rid = self.resource_id.clone();
                self.runtime.spawn(async move {
                    let mut client = ca.lock().await;
                    match client.poll_counter_state(&serial).await {
                        Ok(state) => { let mut s = shared.lock().unwrap(); s.counter = state; s.last_error = None; }
                        Err(e) => { let mut s = shared.lock().unwrap(); s.last_error = Some(e.to_string()); }
                    }
                    if let Some(rid) = rid {
                        if let Ok(info) = client.get_resource(&rid).await {
                            let mut s = shared.lock().unwrap(); s.resource = Some(info);
                        }
                    }
                });
            }
        }
        // --- Device refresh: re-query /devices every 5s when connected ---
        if self.phase != Phase::Disconnected
            && self.last_device_refresh.elapsed() >= Duration::from_secs(5)
        {
            self.last_device_refresh = Instant::now();
            if let Some(ref ca) = self.client {
                let ca = ca.clone();
                match self.runtime.block_on(async { let mut cl = ca.lock().await; cl.list_devices().await }) {
                    Ok(new_devices) => {
                        if let Some(ref serial) = self.selected_serial {
                            let still_present = new_devices.iter().any(|d| &d.serial == serial);
                            if !still_present {
                                let lost = serial.clone();
                                self.selected_serial = None;
                                if self.phase == Phase::Acquired {
                                    self.add_log(&format!("Device {} disconnected from server", lost), true);
                                    self.phase = Phase::Connected;
                                    self.resource_id = None;
                                    self.show_disconnect_dialog = true;
                                    let mut s = self.shared.lock().unwrap();
                                    s.counter = CounterState::default();
                                    s.resource = None;
                                    s.last_error = None;
                                } else {
                                    self.add_log(&format!("Device {} is no longer available", lost), true);
                                }
                            }
                        }
                        self.devices = new_devices;
                    }
                    Err(e) => self.add_log(&format!("Device refresh: {}", e), true),
                }
            }
        }

        // Title bar.
        egui::TopBottomPanel::top("title").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Counters.bit — Rusty Garpac (gRPC)");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Quit").clicked() {
                        if self.phase == Phase::Acquired { self.show_quit_dialog = true; }
                        else { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                    }
                });
            });
        });

        // Log panel.
        egui::TopBottomPanel::bottom("log").min_height(80.0).show(ctx, |ui| {
            ui.horizontal(|ui| { ui.label("Activity Log"); if ui.button("Clear").clicked() { self.log.clear(); } });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                for (time, msg, is_err) in &self.log {
                    let c = if *is_err { egui::Color32::from_rgb(255, 100, 100) } else { egui::Color32::from_rgb(180, 180, 180) };
                    ui.colored_label(c, format!("[{}] {}", time, msg));
                }
            });
        });

        // Central panel.
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.phase == Phase::Acquired { self.draw_counter_panel(ui); }
            else { self.draw_connection_panel(ui); }
        });

        // Quit dialog.
        if self.show_quit_dialog {
            egui::Window::new("Release resource?").collapsible(false).resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label("Release the Counter Resource before quitting?");
                ui.horizontal(|ui| {
                    if ui.button("Yes — Release").clicked() {
                        // Phase 25-Fix-H (F8+F10): Use disconnect() for
                        // clean session + resource cleanup.
                        if let Some(ca) = self.client.clone() {
                            self.runtime.block_on(async {
                                let mut cl = ca.lock().await;
                                let _ = cl.disconnect().await;
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

        // Device disconnect dialog.
        if self.show_disconnect_dialog {
            egui::Window::new("⚠ Device Disconnected").collapsible(false).resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.label("The selected device has been disconnected from the server.");
                ui.label("The Counter Control Panel has been closed.");
                if ui.button("OK").clicked() { self.show_disconnect_dialog = false; }
            });
        }
    }
}

impl CountersApp {
    fn draw_connection_panel(&mut self, ui: &mut egui::Ui) {
        // Phase 28A: mDNS discovered servers dropdown.
        if !self.discovered_servers.is_empty() {
            ui.horizontal(|ui| {
                ui.label("Discovered (mDNS):");
                egui::ComboBox::from_id_salt("mdns_servers")
                    .selected_text("Select a discovered server...")
                    .show_ui(ui, |ui| {
                        for server in &self.discovered_servers {
                            let port_str = server.grpc_port.map(|p| format!(":{}", p)).unwrap_or_default();
                            let label = format!("{} — http://{}{}" , server.description, server.host, port_str);
                            let url = format!("http://{}{}", server.host, port_str);
                            if ui.selectable_label(false, &label).clicked() {
                                self.grpc_url = url;
                            }
                        }
                    });
            });
            ui.add_space(4.0);
        }

        ui.horizontal(|ui| { ui.label("gRPC URL:"); ui.text_edit_singleline(&mut self.grpc_url); });

        if ui.add_enabled(self.phase == Phase::Disconnected, egui::Button::new("1. Connect")).clicked() {
            let url = self.grpc_url.clone();
            match self.runtime.block_on(async {
                let mut client = CountersGrpcClient::connect(&url, USER_ID).await?;
                client.get_server_status().await?;
                let devices = client.list_devices().await?;
                Ok::<(CountersGrpcClient, Vec<DeviceInfo>), rust_grpc_gui_common::ClientError>((client, devices))
            }) {
                Ok((client, devices)) => {
                    self.add_log(&format!("Connected. {} device(s)", devices.len()), false);
                    self.devices = devices;
                    self.selected_serial = None; // Force user to select
                    self.client = Some(Arc::new(TokioMutex::new(client)));
                    self.phase = Phase::Connected;
                    self.last_device_refresh = Instant::now();
                }
                Err(e) => self.add_log(&format!("Connect failed: {}", e), true),
            }
        }

        // Device selection dropdown.
        if !self.devices.is_empty() {
            let mut newly_selected: Option<String> = None;
            ui.horizontal(|ui| {
                ui.label("Select Device:");
                let display = self.selected_serial.as_ref()
                    .and_then(|s| self.devices.iter().find(|d| &d.serial == s))
                    .map(|d| d.display_label())
                    .unwrap_or_else(|| "No device selected".to_string());
                egui::ComboBox::from_id_salt("device_select")
                    .selected_text(display)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for d in &self.devices {
                            let label = d.display_label();
                            let is_selected = self.selected_serial.as_deref() == Some(&d.serial);
                            if ui.selectable_label(is_selected, &label).clicked() {
                                newly_selected = Some(d.serial.clone());
                            }
                        }
                    });
            });
            if let Some(serial) = newly_selected {
                self.add_log(&format!("Selected: {}", serial), false);
                self.selected_serial = Some(serial.clone());

                // F4: Smart-skip — probe device state on selection.
                if let Some(ca) = self.client.clone() {
                    match self.runtime.block_on(async {
                        let mut cl = ca.lock().await;
                        let dev_info = cl.get_device_info(&serial).await.map_err(|e| e.to_string())?;
                        let is_programmed = dev_info.get("state")
                            .and_then(|v| v.as_str())
                            .map(|s| s.contains("Programmed"))
                            .unwrap_or(false);
                        let resources = cl.list_resources().await.map_err(|e| e.to_string())?;
                        let existing = resources.into_iter().find(|r| r.device_serial == serial);
                        Ok::<_, String>((is_programmed, existing))
                    }) {
                        Ok((true, Some(resource))) => {
                            self.add_log(&format!("Device already programmed, resource '{}' available — skipping to Registered", resource.name), false);
                            self.resource_id = Some(resource.id);
                            self.phase = Phase::Registered;
                        }
                        Ok((true, None)) => {
                            self.add_log("Device already programmed — skipping to Register", false);
                            self.phase = Phase::Programmed;
                        }
                        Ok((false, _)) => {}
                        Err(e) => self.add_log(&format!("Could not check device state: {}", e), false),
                    }
                }
            }
        }

        ui.horizontal(|ui| { ui.label("Bitfile:"); ui.text_edit_singleline(&mut self.bitfile_path); });

        // F5: Bitfile mode toggle.
        ui.horizontal(|ui| {
            ui.label("Mode:");
            ui.radio_value(&mut self.bitfile_mode, BitfileMode::ServerPath, "Server Path");
            ui.radio_value(&mut self.bitfile_mode, BitfileMode::LocalFile, "Local File");
        });

        if ui.add_enabled(self.phase == Phase::Connected && self.selected_serial.is_some(), egui::Button::new("2. Program")).clicked() {
            let ca = self.client.clone().unwrap(); let s = self.serial(); let bf = self.bitfile_path.clone();
            let mode = self.bitfile_mode;
            // F5: Branch on bitfile mode.
            let result = self.runtime.block_on(async {
                let mut cl = ca.lock().await;
                match mode {
                    BitfileMode::ServerPath => cl.configure_fpga(&s, &bf).await.map_err(|e| e.to_string()),
                    BitfileMode::LocalFile => cl.configure_fpga_from_file(&s, std::path::Path::new(&bf)).await.map_err(|e| e.to_string()),
                }
            });
            match result {
                Ok(()) => { self.add_log(&format!("FPGA programmed ({:?})", mode), false); self.phase = Phase::Programmed; }
                Err(e) => self.add_log(&format!("Program: {}", e), true),
            }
        }

        // Phase 34-E: Two register buttons — Shared and Exclusive.
        let register_shared = ui.add_enabled(self.phase == Phase::Programmed, egui::Button::new("3. Register (Shared)")).clicked();
        let register_exclusive = ui.add_enabled(self.phase == Phase::Programmed, egui::Button::new("Register (Exclusive)")).clicked();
        if register_shared || register_exclusive {
            let resource_type = if register_exclusive { "exclusive" } else { "shared" };
            let ca = self.client.clone().unwrap(); let s = self.serial();
            let rt = resource_type.to_string();
            match self.runtime.block_on(async { let mut cl = ca.lock().await; cl.create_resource("Counter Resource", &s, &rt).await }) {
                Ok(info) => { self.add_log(&format!("Registered ({}): {}", resource_type, info.id), false); self.resource_id = Some(info.id); self.phase = Phase::Registered; }
                Err(e) => {
                    let err_str = e.to_string();
                    // F3: AlreadyExists / conflict — look up the existing resource.
                    if err_str.contains("AlreadyExists") || err_str.contains("already exists") {
                        self.add_log("Resource already exists — looking up...", false);
                        if let Some(ca2) = self.client.clone() {
                            match self.runtime.block_on(async {
                                let mut cl = ca2.lock().await;
                                cl.list_resources().await.map_err(|e| e.to_string())
                            }) {
                                Ok(resources) => {
                                    if let Some(existing) = resources.into_iter().find(|r| r.device_serial == s) {
                                        self.add_log(&format!("Found existing: {}", existing.id), false);
                                        self.resource_id = Some(existing.id);
                                        self.phase = Phase::Registered;
                                    } else {
                                        self.add_log("No matching resource found", true);
                                    }
                                }
                                Err(e2) => self.add_log(&format!("Lookup: {}", e2), true),
                            }
                        }
                    } else {
                        self.add_log(&format!("Register: {}", err_str), true);
                    }
                }
            }
        }

        if ui.add_enabled(self.phase == Phase::Registered, egui::Button::new("4. Acquire (Shared)")).clicked() {
            self.acquired_readonly = false;
            let ca = self.client.clone().unwrap(); let rid = self.resource_id.clone().unwrap();
            match self.runtime.block_on(async { let mut cl = ca.lock().await; cl.acquire_resource(&rid).await }) {
                Ok(()) => { self.add_log("Acquired (shared)", false); self.phase = Phase::Acquired; }
                Err(e) => self.add_log(&format!("Acquire: {}", e), true),
            }
        }

        // Step 4b: Acquire Read-Only (Phase 30, ADR-026).
        if ui.add_enabled(self.phase == Phase::Registered, egui::Button::new("Acquire R/O")).clicked() {
            self.acquired_readonly = true;
            let ca = self.client.clone().unwrap(); let rid = self.resource_id.clone().unwrap();
            match self.runtime.block_on(async { let mut cl = ca.lock().await; cl.acquire_resource_readonly(&rid).await }) {
                Ok(()) => { self.add_log("Acquired (read-only)", false); self.phase = Phase::Acquired; }
                Err(e) => self.add_log(&format!("Acquire R/O: {}", e), true),
            }
        }
    }

    fn draw_counter_panel(&mut self, ui: &mut egui::Ui) {
        let locked_out = self.is_locked_out() || self.acquired_readonly;

        if self.acquired_readonly {
            ui.colored_label(egui::Color32::YELLOW, "📖 Read-Only Mode — write controls disabled");
        }
        let counter = { self.shared.lock().unwrap().counter.clone() };

        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.heading("Counter #1");
                ui.label(format!("0x{:02X}", counter.count1));
                ui.add(egui::ProgressBar::new(counter.count1 as f32 / 255.0));
                let mut r = self.wire_in.reset1;
                if ui.add_enabled(!locked_out, egui::Checkbox::new(&mut r, "RESET1")).changed() {
                    self.wire_in.reset1 = r; self.grpc_send_wire_in();
                }
                let mut d = self.wire_in.disable1;
                if ui.add_enabled(!locked_out, egui::Checkbox::new(&mut d, "DISABLE1")).changed() {
                    self.wire_in.disable1 = d; self.grpc_send_wire_in();
                }
                ui.label(format!("==0x00: {} | ==0x80: {}", if counter.count1_eq_00 {"🟢"} else {"⚫"}, if counter.count1_eq_80 {"🟢"} else {"⚫"}));
            });
            cols[1].group(|ui| {
                ui.heading("Counter #2");
                ui.label(format!("0x{:02X}", counter.count2));
                ui.add(egui::ProgressBar::new(counter.count2 as f32 / 255.0));
                ui.horizontal(|ui| {
                    if ui.add_enabled(!locked_out, egui::Button::new("▲")).clicked() { self.grpc_trigger(0x40, 1); }
                    if ui.add_enabled(!locked_out, egui::Button::new("▼")).clicked() { self.grpc_trigger(0x40, 2); }
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(!locked_out, egui::Button::new("⟲")).clicked() { self.grpc_trigger(0x40, 0); }
                    let mut a = self.wire_in.autocount2;
                    if ui.add_enabled(!locked_out, egui::Checkbox::new(&mut a, "AUTO")).changed() {
                        self.wire_in.autocount2 = a; self.grpc_send_wire_in();
                    }
                });
                ui.label(format!("==0xFF: {}", if counter.count2_eq_ff {"🟢"} else {"⚫"}));
            });
        });

        ui.horizontal(|ui| {
            ui.label("Buttons:"); for i in 0..4 { let p = (counter.buttons >> i) & 1 != 0;
                let c = if p { egui::Color32::from_rgb(0,200,100) } else { egui::Color32::from_rgb(80,80,80) };
                ui.colored_label(c, format!("[{}]", i+1));
            }
            ui.separator(); ui.label("Poll:");
            egui::ComboBox::from_id_salt("poll").selected_text(format!("{}ms", self.poll_interval_ms)).show_ui(ui, |ui| {
                for ms in [100,250,500,1000,2000] { ui.selectable_value(&mut self.poll_interval_ms, ms, format!("{}ms", ms)); }
            });
        });
        ui.horizontal(|ui| {
            let me = self.shared.lock().unwrap().resource.as_ref().and_then(|r| r.locked_by.clone()).map(|l| l == USER_ID).unwrap_or(false);
            if me { if ui.button("🔓 Unlock").clicked() { self.grpc_lock_op(false); } }
            else { if ui.button("🔒 Lock").clicked() { self.grpc_lock_op(true); } }
            ui.checkbox(&mut self.override_lockout, "Override lockout");
        });
    }

    fn grpc_send_wire_in(&mut self) {
        let ca = self.client.clone().unwrap(); let s = self.serial(); let w = self.wire_in.clone();
        match self.runtime.block_on(async { let mut cl = ca.lock().await; cl.send_wire_in(&s, &w).await }) {
            Ok(()) => self.add_log(&format!("WireIn = 0x{:04X}", w.to_value()), false),
            Err(e) => self.add_log(&format!("WireIn: {}", e), true),
        }
    }

    fn grpc_trigger(&mut self, ep: i32, bit: i32) {
        let ca = self.client.clone().unwrap(); let s = self.serial();
        match self.runtime.block_on(async { let mut cl = ca.lock().await; cl.activate_trigger_in(&s, ep, bit).await }) {
            Ok(()) => self.add_log(&format!("Trigger 0x{:02X} bit {}", ep, bit), false),
            Err(e) => self.add_log(&format!("Trigger: {}", e), true),
        }
    }

    fn grpc_lock_op(&mut self, lock: bool) {
        let ca = self.client.clone().unwrap(); let rid = self.resource_id.clone().unwrap();
        let result = self.runtime.block_on(async {
            let mut cl = ca.lock().await;
            if lock { cl.lock_resource(&rid).await } else { cl.unlock_resource(&rid).await }
        });
        match result {
            Ok(()) => self.add_log(if lock { "Locked" } else { "Unlocked" }, false),
            Err(e) => self.add_log(&format!("Lock op: {}", e), true),
        }
    }
}
