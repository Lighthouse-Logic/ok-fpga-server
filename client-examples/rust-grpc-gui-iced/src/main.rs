//! # Counters.bit Interactive Demo — Rust gRPC + iced
//!
//! Same UI as the REST+iced demo but communicates via gRPC.
//! Uses iced 0.14's reactive rendering and `iced_test` headless
//! simulator for GUI testing.
//!
//! ## User Identity: "Icy Garpac"
//!
//! ## Note on Architecture
//!
//! Because iced Messages must be Debug+Clone, and tonic's gRPC client
//! isn't Clone, each async operation creates a fresh gRPC connection.
//! This is fine for a demo — production code would use a shared channel.

use iced::widget::{
    button, checkbox, column, container, pick_list,
    progress_bar, row, rule, scrollable, text, text_input, Column, Row, Space,
};
use iced::{Element, Length, Subscription, Task, Theme};
use rust_grpc_gui_common::{
    CounterState, CountersGrpcClient, DeviceInfo, ResourceInfo, WireInState,
    mdns_browser::DiscoveredServer,
};
use std::time::Duration;

const DEFAULT_GRPC_URL: &str = "http://localhost:50051";
const DEFAULT_BITFILE: &str =
    "FrontPanel-6.1.0/ExampleBitfiles/20190501-FP50-XEM7320-A75/XEM7320-A75/Counters.bit";
const USER_ID: &str = "Icy Garpac";

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // iced 0.14: first arg is a boot function, title via builder.
    iced::application(boot, update, view)
        .title("Counters.bit — Icy Garpac (gRPC + iced)")
        .theme(theme)
        .subscription(subscription)
        .window_size((720.0, 560.0))
        .run()
}

/// Theme function — named function required by iced 0.14 lifetime bounds.
fn theme(_state: &AppState) -> Theme {
    Theme::Dark
}

/// Boot function for iced 0.14 — returns initial state and startup task.
fn boot() -> (AppState, Task<Message>) {
    let bitfile = std::env::var("OKFPGA_TEST_BITFILE")
        .unwrap_or_else(|_| DEFAULT_BITFILE.to_string());
    (AppState::new(bitfile), Task::perform(
        async {
            match rust_grpc_gui_common::mdns_browser::MdnsBrowser::new() {
                Ok(browser) => {
                    let servers = browser.browse_for(Duration::from_secs(3)).await;
                    browser.stop();
                    servers
                }
                Err(_) => vec![],
            }
        },
        Message::MdnsDiscovered,
    ))
}

/// Application phase — sequential startup flow.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Phase { Disconnected, Connected, Programmed, Registered, Acquired }

/// Bitfile configuration mode — server-side path or client-side upload (F5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BitfileMode {
    /// Send the path to the server (server reads the file).
    ServerPath,
    /// Read the file locally and upload as binary.
    LocalFile,
}

/// Complete application state.
pub struct AppState {
    pub grpc_url: String,
    pub bitfile_path: String,
    /// Whether to send the bitfile path or upload local file (F5).
    pub bitfile_mode: BitfileMode,
    pub phase: Phase,
    pub devices: Vec<DeviceInfo>,
    pub selected_serial: Option<String>,
    pub resource_id: Option<String>,
    pub wire_in: WireInState,
    pub poll_interval_ms: u64,
    pub override_lockout: bool,
    pub counter: CounterState,
    pub resource_info: Option<ResourceInfo>,
    pub log_entries: Vec<(String, bool)>,
    pub last_error: Option<String>,
    pub show_disconnect_dialog: bool,
    /// Discovered ok-FPGA servers via mDNS (Phase 28A).
    pub discovered_servers: Vec<DiscoveredServer>,
    /// True when the user acquired as ReadOnly (Phase 30, ADR-026).
    pub acquired_readonly: bool,
}

impl AppState {
    /// Create a new application state with default values.
    pub fn new(bitfile: String) -> Self {
        Self {
            grpc_url: DEFAULT_GRPC_URL.to_string(), bitfile_path: bitfile,
            bitfile_mode: BitfileMode::ServerPath,
            phase: Phase::Disconnected, devices: vec![], selected_serial: None,
            resource_id: None, wire_in: WireInState::default(), poll_interval_ms: 500,
            override_lockout: false, counter: CounterState::default(), resource_info: None,
            log_entries: vec![], last_error: None, show_disconnect_dialog: false,
            discovered_servers: vec![],
            acquired_readonly: false,
        }
    }

    /// Get selected device serial or empty string.
    pub fn serial(&self) -> String { self.selected_serial.clone().unwrap_or_default() }

    /// Check if locked out by another user.
    pub fn is_locked_out(&self) -> bool {
        if self.override_lockout { return false; }
        self.resource_info.as_ref().and_then(|r| r.locked_by.as_deref()).map(|l| l != USER_ID).unwrap_or(false)
    }

    /// Add a normal log entry (max 200).
    pub fn log(&mut self, msg: &str) { self.log_entries.push((msg.to_string(), false)); if self.log_entries.len() > 200 { self.log_entries.remove(0); } }

    /// Add an error log entry (max 200).
    pub fn log_err(&mut self, msg: &str) { self.log_entries.push((msg.to_string(), true)); if self.log_entries.len() > 200 { self.log_entries.remove(0); } }
}

/// Helper: create a fresh gRPC client connection.
async fn connect(url: &str) -> Result<CountersGrpcClient, String> {
    CountersGrpcClient::connect(url, USER_ID).await.map_err(|e| e.to_string())
}

/// All possible user actions and async operation results.
#[derive(Debug, Clone)]
pub enum Message {
    UrlChanged(String), BitfileChanged(String),
    ConnectPressed, ConnectResult(Result<Vec<DeviceInfo>, String>),
    SelectDevice(String),
    ProgramPressed, ProgramResult(Result<(), String>),
    RegisterSharedPressed, RegisterExclusivePressed, RegisterResult(Result<ResourceInfo, String>),
    AcquirePressed, AcquireReadOnlyPressed, AcquireResult(Result<(), String>),
    SetReset1(bool), SetDisable1(bool), SetAutocount2(bool), WireInResult(Result<(), String>),
    CountUp, CountDown, ResetCounter2, TriggerResult(Result<(), String>),
    PollTick, PollResult(Result<(CounterState, Option<ResourceInfo>), String>),
    PollIntervalChanged(u64),
    LockPressed, UnlockPressed, LockResult(Result<(), String>),
    OverrideLockout(bool),
    QuitRelease, QuitLeaveHeld, DisconnectResult(Result<(), String>),
    RefreshDevicesTick, RefreshDevicesResult(Result<Vec<DeviceInfo>, String>),
    DismissDisconnect,
    // Phase 25-Fix-F additions
    SetBitfileMode(BitfileMode),
    CheckDeviceState,
    CheckDeviceStateResult(Result<(bool, Option<ResourceInfo>), String>),
    // Phase 28A: mDNS discovery.
    MdnsDiscovered(Vec<DiscoveredServer>),
    MdnsSelectServer(String),
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::UrlChanged(u) => { state.grpc_url = u; Task::none() }
        Message::BitfileChanged(p) => { state.bitfile_path = p; Task::none() }
        Message::MdnsDiscovered(servers) => {
            state.log(&format!("mDNS: discovered {} server(s)", servers.len()));
            state.discovered_servers = servers;
            Task::none()
        }
        Message::MdnsSelectServer(url) => {
            state.grpc_url = url;
            Task::none()
        }
        Message::ConnectPressed => {
            let url = state.grpc_url.clone();
            Task::perform(async move {
                let mut c = connect(&url).await?;
                c.get_server_status().await.map_err(|e| e.to_string())?;
                c.list_devices().await.map_err(|e| e.to_string())
            }, Message::ConnectResult)
        }
        Message::ConnectResult(r) => {
            match r {
                Ok(devices) => {
                    state.log(&format!("Connected. {} device(s)", devices.len()));
                    state.devices = devices;
                    state.selected_serial = None; // Force user to select
                    state.phase = Phase::Connected;
                }
                Err(e) => state.log_err(&format!("Connect: {}", e)),
            } Task::none()
        }
        Message::SelectDevice(s) => {
            state.log(&format!("Selected: {}", s));
            state.selected_serial = Some(s);
            // F4: Probe device state and existing resources (smart-skip).
            update(state, Message::CheckDeviceState)
        }
        Message::ProgramPressed => {
            let url = state.grpc_url.clone(); let s = state.serial(); let bf = state.bitfile_path.clone();
            let mode = state.bitfile_mode;
            Task::perform(async move {
                let mut c = connect(&url).await?;
                // F5: Branch on bitfile mode.
                match mode {
                    BitfileMode::ServerPath => c.configure_fpga(&s, &bf).await.map_err(|e| e.to_string()),
                    BitfileMode::LocalFile => c.configure_fpga_from_file(&s, std::path::Path::new(&bf)).await.map_err(|e| e.to_string()),
                }
            }, Message::ProgramResult)
        }
        Message::ProgramResult(r) => { match r { Ok(()) => { state.log("Programmed"); state.phase = Phase::Programmed; } Err(e) => state.log_err(&e) } Task::none() }
        Message::RegisterSharedPressed | Message::RegisterExclusivePressed => {
            let resource_type = if matches!(message, Message::RegisterExclusivePressed) { "exclusive" } else { "shared" };
            let url = state.grpc_url.clone(); let s = state.serial();
            let rt = resource_type.to_string();
            Task::perform(async move {
                let mut c = connect(&url).await?;
                c.create_resource("Counter Resource", &s, &rt).await.map_err(|e| e.to_string())
            }, Message::RegisterResult)
        }
        Message::RegisterResult(r) => {
            match r {
                Ok(i) => {
                    state.log(&format!("Registered: {}", i.id));
                    state.resource_id = Some(i.id);
                    state.phase = Phase::Registered;
                }
                // F3: AlreadyExists / 409 Conflict — look up the existing resource.
                Err(e) if e.contains("AlreadyExists") || e.contains("409") || e.contains("already exists") => {
                    state.log("Resource already exists — looking up...");
                    let url = state.grpc_url.clone();
                    let serial = state.serial();
                    return Task::perform(async move {
                        let mut c = connect(&url).await?;
                        let resources = c.list_resources().await.map_err(|e| e.to_string())?;
                        resources.into_iter().find(|r| r.device_serial == serial)
                            .ok_or_else(|| "No matching resource found".to_string())
                    }, Message::RegisterResult);
                }
                Err(e) => state.log_err(&e),
            }
            Task::none()
        }
        Message::AcquirePressed => {
            state.acquired_readonly = false;
            let url = state.grpc_url.clone(); let rid = state.resource_id.clone().unwrap();
            Task::perform(async move {
                let mut c = connect(&url).await?;
                c.acquire_resource(&rid).await.map_err(|e| e.to_string())
            }, Message::AcquireResult)
        }
        Message::AcquireReadOnlyPressed => {
            state.acquired_readonly = true;
            let url = state.grpc_url.clone(); let rid = state.resource_id.clone().unwrap();
            Task::perform(async move {
                let mut c = connect(&url).await?;
                c.acquire_resource_readonly(&rid).await.map_err(|e| e.to_string())
            }, Message::AcquireResult)
        }
        Message::AcquireResult(r) => {
            match r {
                Ok(()) => {
                    let mode = if state.acquired_readonly { "read-only" } else { "shared" };
                    state.log(&format!("Acquired ({})", mode));
                    state.phase = Phase::Acquired;
                }
                Err(e) => state.log_err(&e),
            }
            Task::none()
        }
        Message::SetReset1(v) => { state.wire_in.reset1 = v; send_wire(state) }
        Message::SetDisable1(v) => { state.wire_in.disable1 = v; send_wire(state) }
        Message::SetAutocount2(v) => { state.wire_in.autocount2 = v; send_wire(state) }
        Message::WireInResult(r) => { if let Err(e) = r { state.log_err(&e); } Task::none() }
        Message::CountUp => fire(state, 0x40, 1),
        Message::CountDown => fire(state, 0x40, 2),
        Message::ResetCounter2 => fire(state, 0x40, 0),
        Message::TriggerResult(r) => { if let Err(e) = r { state.log_err(&e); } Task::none() }
        Message::PollTick => {
            if state.phase != Phase::Acquired { return Task::none(); }
            let url = state.grpc_url.clone(); let s = state.serial(); let rid = state.resource_id.clone();
            Task::perform(async move {
                let mut c = connect(&url).await?;
                let counter = c.poll_counter_state(&s).await.map_err(|e| e.to_string())?;
                let res = if let Some(rid) = rid { c.get_resource(&rid).await.ok() } else { None };
                Ok((counter, res))
            }, Message::PollResult)
        }
        Message::PollResult(r) => { match r { Ok((c, res)) => { state.counter = c; state.resource_info = res; state.last_error = None; } Err(e) => state.last_error = Some(e) } Task::none() }
        Message::PollIntervalChanged(ms) => { state.poll_interval_ms = ms; Task::none() }
        Message::LockPressed => {
            let url = state.grpc_url.clone(); let rid = state.resource_id.clone().unwrap();
            Task::perform(async move { let mut c = connect(&url).await?; c.lock_resource(&rid).await.map_err(|e| e.to_string()) }, Message::LockResult)
        }
        Message::UnlockPressed => {
            let url = state.grpc_url.clone(); let rid = state.resource_id.clone().unwrap();
            Task::perform(async move { let mut c = connect(&url).await?; c.unlock_resource(&rid).await.map_err(|e| e.to_string()) }, Message::LockResult)
        }
        Message::LockResult(r) => { match r { Ok(()) => state.log("Lock/unlock OK"), Err(e) => state.log_err(&e) } Task::none() }
        Message::OverrideLockout(v) => { state.override_lockout = v; Task::none() }
        Message::QuitRelease => {
            // Phase 25-Fix-H (F8+F10): Use disconnect() via Task::perform
            // instead of fire-and-forget tokio::spawn.
            let url = state.grpc_url.clone();
            Task::perform(async move {
                match connect(&url).await {
                    Ok(mut c) => c.disconnect().await.map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                }
            }, Message::DisconnectResult)
        }
        Message::QuitLeaveHeld => iced::exit(),
        Message::DisconnectResult(_) => iced::exit(),

        Message::RefreshDevicesTick => {
            let url = state.grpc_url.clone();
            Task::perform(async move {
                let mut c = connect(&url).await?;
                c.list_devices().await.map_err(|e| e.to_string())
            }, Message::RefreshDevicesResult)
        }
        Message::RefreshDevicesResult(result) => {
            match result {
                Ok(new_devices) => {
                    if let Some(ref serial) = state.selected_serial {
                        let still_present = new_devices.iter().any(|d| &d.serial == serial);
                        if !still_present {
                            let lost = serial.clone();
                            state.selected_serial = None;
                            if state.phase == Phase::Acquired {
                                state.log_err(&format!("Device {} disconnected from server", lost));
                                state.phase = Phase::Connected;
                                state.resource_id = None;
                                state.counter = CounterState::default();
                                state.resource_info = None;
                                state.last_error = None;
                                state.show_disconnect_dialog = true;
                            } else {
                                state.log_err(&format!("Device {} is no longer available", lost));
                            }
                        }
                    }
                    state.devices = new_devices;
                }
                Err(e) => state.log_err(&format!("Device refresh: {}", e)),
            }
            Task::none()
        }
        Message::DismissDisconnect => { state.show_disconnect_dialog = false; Task::none() }

        // ================================================================
        // Phase 25-Fix-F: Bitfile mode toggle + device state check
        // ================================================================

        Message::SetBitfileMode(mode) => {
            state.bitfile_mode = mode;
            state.log(&format!("Bitfile mode: {:?}", mode));
            Task::none()
        }

        Message::CheckDeviceState => {
            let url = state.grpc_url.clone();
            let serial = state.serial();
            if serial.is_empty() {
                return Task::none();
            }
            Task::perform(
                async move {
                    let mut c = connect(&url).await?;
                    // Check if device is programmed.
                    let dev_info = c.get_device_info(&serial).await.map_err(|e| e.to_string())?;
                    let is_programmed = dev_info.get("state")
                        .and_then(|v| v.as_str())
                        .map(|s| s.contains("Programmed"))
                        .unwrap_or(false);
                    // Check for existing resource.
                    let resources = c.list_resources().await.map_err(|e| e.to_string())?;
                    let existing = resources.into_iter().find(|r| r.device_serial == serial);
                    Ok((is_programmed, existing))
                },
                Message::CheckDeviceStateResult,
            )
        }
        Message::CheckDeviceStateResult(result) => {
            match result {
                Ok((true, Some(resource))) => {
                    state.log(&format!(
                        "Device already programmed, resource '{}' available — skipping to Registered",
                        resource.name
                    ));
                    state.resource_id = Some(resource.id);
                    state.phase = Phase::Registered;
                }
                Ok((true, None)) => {
                    state.log("Device already programmed — skipping to Register");
                    state.phase = Phase::Programmed;
                }
                Ok((false, _)) => {
                    // Not programmed — normal flow.
                }
                Err(e) => {
                    state.log(&format!("Could not check device state: {}", e));
                }
            }
            Task::none()
        }
    }
}

fn send_wire(state: &AppState) -> Task<Message> {
    let url = state.grpc_url.clone(); let s = state.serial(); let w = state.wire_in.clone();
    Task::perform(async move { let mut c = connect(&url).await?; c.send_wire_in(&s, &w).await.map_err(|e| e.to_string()) }, Message::WireInResult)
}

fn fire(state: &AppState, ep: i32, bit: i32) -> Task<Message> {
    let url = state.grpc_url.clone(); let s = state.serial();
    Task::perform(async move { let mut c = connect(&url).await?; c.activate_trigger_in(&s, ep, bit).await.map_err(|e| e.to_string()) }, Message::TriggerResult)
}

fn subscription(state: &AppState) -> Subscription<Message> {
    let mut subs = vec![];
    if state.phase == Phase::Acquired {
        subs.push(iced::time::every(Duration::from_millis(state.poll_interval_ms)).map(|_| Message::PollTick));
    }
    if state.phase != Phase::Disconnected {
        subs.push(iced::time::every(Duration::from_secs(5)).map(|_| Message::RefreshDevicesTick));
    }
    Subscription::batch(subs)
}

/// Top-level view function.
pub fn view(state: &AppState) -> Element<'_, Message> {
    let main: Element<Message> = if state.phase == Phase::Acquired { view_counters(state) } else { view_connect(state) };
    let log_items: Vec<Element<Message>> = state.log_entries.iter().map(|(m, e)| {
        let t = text(m.clone()).size(12); if *e { t.color(iced::Color::from_rgb(1.0,0.4,0.4)).into() } else { t.color(iced::Color::from_rgb(0.7,0.7,0.7)).into() }
    }).collect();
    container(column![
        text("Counters.bit — Icy Garpac").size(20), rule::horizontal(1), main, rule::horizontal(1),
        text("Activity Log").size(14), scrollable(Column::with_children(log_items)).height(80),
    ].spacing(8).padding(12)).width(Length::Fill).height(Length::Fill).into()
}

fn view_connect(s: &AppState) -> Element<'_, Message> {
    let con = if s.phase == Phase::Disconnected { button("1. Connect").on_press(Message::ConnectPressed) } else { button("1. Connect ✓") };

    // Device selection dropdown. Phase 34-B: show SYZYGY in labels.
    let device_options: Vec<String> = s.devices.iter().map(|d| d.display_label()).collect();
    let selected_display: Option<String> = s.selected_serial.as_ref()
        .and_then(|sel| s.devices.iter().find(|d| &d.serial == sel))
        .map(|d| d.display_label());
    let device_picker = row![
        text("Select Device:").width(100),
        pick_list(device_options, selected_display, |picked| {
            // Extract serial from "Product (SERIAL) | ..." format.
            let serial = picked.split('(').nth(1)
                .and_then(|s| s.split(')').next())
                .unwrap_or(&picked).to_string();
            Message::SelectDevice(serial)
        }).placeholder("No device selected").width(400),
    ].spacing(8);

    let prog = if s.phase >= Phase::Programmed { button("2. Program ✓") }
        else if s.phase == Phase::Connected && s.selected_serial.is_some() { button("2. Program").on_press(Message::ProgramPressed) }
        else { button("2. Program") };
    let reg = if s.phase >= Phase::Registered { button("3. Register ✓") } else if s.phase == Phase::Programmed { button("3. Register (Shared)").on_press(Message::RegisterSharedPressed) } else { button("3. Register (Shared)") };
    let reg_excl = if s.phase >= Phase::Registered { button("Exclusive") } else if s.phase == Phase::Programmed { button("Register (Exclusive)").on_press(Message::RegisterExclusivePressed) } else { button("Register (Exclusive)") };
    let acq = if s.phase >= Phase::Acquired {
        let mode = if s.acquired_readonly { "R/O" } else { "Shared" };
        button(text(format!("4. Acquire ({}) ✓", mode)))
    } else if s.phase == Phase::Registered { button("4. Acquire").on_press(Message::AcquirePressed) } else { button("4. Acquire") };
    let acq_ro = if s.phase >= Phase::Acquired { button("R/O") } else if s.phase == Phase::Registered { button("R/O").on_press(Message::AcquireReadOnlyPressed) } else { button("R/O") };
    let acq_row: Element<Message> = row![acq, acq_ro].spacing(8).into();

    // Disconnect message.
    let disconnect_msg: Element<Message> = if s.show_disconnect_dialog {
        column![
            text("⚠ The selected device has been disconnected from the server.").color(iced::Color::from_rgb(1.0, 0.6, 0.0)),
            text("The Counter Control Panel has been closed.").color(iced::Color::from_rgb(1.0, 0.6, 0.0)),
            button("OK").on_press(Message::DismissDisconnect),
        ].spacing(4).into()
    } else { text("").into() };

    // F5: Bitfile mode toggle — Server Path vs Local File.
    let mode_row = row![
        text("Mode:").width(100),
        button(if s.bitfile_mode == BitfileMode::ServerPath { "● Server Path" } else { "○ Server Path" })
            .on_press(Message::SetBitfileMode(BitfileMode::ServerPath)),
        button(if s.bitfile_mode == BitfileMode::LocalFile { "● Local File" } else { "○ Local File" })
            .on_press(Message::SetBitfileMode(BitfileMode::LocalFile)),
    ].spacing(8);

    // Phase 28A: mDNS discovered servers dropdown.
    let mdns_section: Element<Message> = if !s.discovered_servers.is_empty() {
        let options: Vec<String> = s.discovered_servers.iter()
            .map(|srv| {
                let port_str = srv.grpc_port.map(|p| format!(":{}", p)).unwrap_or_default();
                format!("{} — http://{}{}" , srv.description, srv.host, port_str)
            })
            .collect();
        row![
            text("Discovered (mDNS):").width(100),
            pick_list(options, Option::<String>::None, |picked| {
                if let Some(url) = picked.split(" — ").nth(1) {
                    Message::MdnsSelectServer(url.to_string())
                } else {
                    Message::MdnsSelectServer(picked)
                }
            }).placeholder("Select a discovered server...").width(400),
        ].spacing(8).into()
    } else {
        text("").into()
    };

    let reg_row: Element<Message> = row![reg, reg_excl].spacing(8).into();

    column![
        disconnect_msg,
        mdns_section,
        row![text("gRPC URL:").width(100), text_input("http://localhost:50051", &s.grpc_url).on_input(Message::UrlChanged)].spacing(8),
        con, device_picker,
        row![text("Bitfile:").width(100), text_input("path", &s.bitfile_path).on_input(Message::BitfileChanged)].spacing(8),
        mode_row,
        prog, reg_row, acq_row,
    ].spacing(8).into()
}

fn view_counters(s: &AppState) -> Element<'_, Message> {
    let lo = s.is_locked_out() || s.acquired_readonly;
    let err: Element<Message> = if let Some(ref e) = s.last_error { text(e).color(iced::Color::from_rgb(1.0,0.3,0.3)).into() } else { text("").into() };
    // iced 0.14: checkbox(bool_value).label("text") instead of checkbox("text", bool_value)
    let c1 = column![text("Counter #1").size(16), text(format!("0x{:02X}", s.counter.count1)), progress_bar(0.0..=255.0, s.counter.count1 as f32),
        if lo { checkbox(s.wire_in.reset1).label("RESET1") } else { checkbox(s.wire_in.reset1).label("RESET1").on_toggle(Message::SetReset1) },
        if lo { checkbox(s.wire_in.disable1).label("DISABLE1") } else { checkbox(s.wire_in.disable1).label("DISABLE1").on_toggle(Message::SetDisable1) },
        text(format!("==0x00: {} | ==0x80: {}", if s.counter.count1_eq_00 {"🟢"} else {"⚫"}, if s.counter.count1_eq_80 {"🟢"} else {"⚫"})),
    ].spacing(4).width(Length::FillPortion(1));
    let c2 = column![text("Counter #2").size(16), text(format!("0x{:02X}", s.counter.count2)), progress_bar(0.0..=255.0, s.counter.count2 as f32),
        row![if lo { button("▲") } else { button("▲").on_press(Message::CountUp) }, if lo { button("▼") } else { button("▼").on_press(Message::CountDown) }].spacing(4),
        row![if lo { button("⟲") } else { button("⟲").on_press(Message::ResetCounter2) }, if lo { checkbox(s.wire_in.autocount2).label("AUTO") } else { checkbox(s.wire_in.autocount2).label("AUTO").on_toggle(Message::SetAutocount2) }].spacing(4),
        text(format!("==0xFF: {}", if s.counter.count2_eq_ff {"🟢"} else {"⚫"})),
    ].spacing(4).width(Length::FillPortion(1));
    let btns: Vec<Element<Message>> = (0..4).map(|i| { let p = (s.counter.buttons >> i) & 1 != 0; if p { text(format!("[{}]",i+1)).color(iced::Color::from_rgb(0.0,0.8,0.4)).into() } else { text(format!("[{}]",i+1)).color(iced::Color::from_rgb(0.3,0.3,0.3)).into() } }).collect();
    let poll_opts: Vec<u64> = vec![100,250,500,1000,2000];
    let me = s.resource_info.as_ref().and_then(|r| r.locked_by.as_deref()).map(|l| l == USER_ID).unwrap_or(false);
    let lock = if me { button("🔓 Unlock").on_press(Message::UnlockPressed) } else { button("🔒 Lock").on_press(Message::LockPressed) };
    column![err, row![c1, c2].spacing(16), rule::horizontal(1),
        row![text("Buttons:"), Row::with_children(btns).spacing(4), Space::new().width(Length::Fill), text("Poll:"), pick_list(poll_opts, Some(s.poll_interval_ms), Message::PollIntervalChanged), text("ms")].spacing(8),
        row![lock, checkbox(s.override_lockout).label("Override").on_toggle(Message::OverrideLockout)].spacing(8), rule::horizontal(1),
        row![button("Quit — Release").on_press(Message::QuitRelease), button("Quit — Leave Held").on_press(Message::QuitLeaveHeld)].spacing(8),
    ].spacing(8).into()
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> AppState {
        let (state, _) = boot();
        state
    }

    // --- Phase ordering ---

    #[test]
    fn test_phase_ordering() {
        assert!(Phase::Disconnected < Phase::Connected);
        assert!(Phase::Connected < Phase::Programmed);
        assert!(Phase::Programmed < Phase::Registered);
        assert!(Phase::Registered < Phase::Acquired);
    }

    // --- AppState defaults ---

    #[test]
    fn test_initial_state() {
        let state = make_state();
        assert_eq!(state.phase, Phase::Disconnected);
        assert_eq!(state.grpc_url, DEFAULT_GRPC_URL);
        assert!(state.devices.is_empty());
        assert!(state.selected_serial.is_none());
        assert!(!state.override_lockout);
        assert_eq!(state.poll_interval_ms, 500);
    }

    // --- serial() ---

    #[test]
    fn test_serial_empty_when_none() {
        assert_eq!(make_state().serial(), "");
    }

    #[test]
    fn test_serial_returns_selected() {
        let mut s = make_state();
        s.selected_serial = Some("SN001".to_string());
        assert_eq!(s.serial(), "SN001");
    }

    // --- is_locked_out() ---

    #[test]
    fn test_not_locked_out_no_resource() {
        assert!(!make_state().is_locked_out());
    }

    #[test]
    fn test_not_locked_out_no_lock() {
        let mut s = make_state();
        s.resource_info = Some(ResourceInfo {
            id: "r1".to_string(), name: "R".to_string(),
            device_serial: "S".to_string(), access_mode: "shared".to_string(),
            holders: vec![], locked_by: None,
        });
        assert!(!s.is_locked_out());
    }

    #[test]
    fn test_locked_out_by_other() {
        let mut s = make_state();
        s.resource_info = Some(ResourceInfo {
            id: "r1".to_string(), name: "R".to_string(),
            device_serial: "S".to_string(), access_mode: "shared".to_string(),
            holders: vec![], locked_by: Some("other".to_string()),
        });
        assert!(s.is_locked_out());
    }

    #[test]
    fn test_not_locked_out_by_us() {
        let mut s = make_state();
        s.resource_info = Some(ResourceInfo {
            id: "r1".to_string(), name: "R".to_string(),
            device_serial: "S".to_string(), access_mode: "shared".to_string(),
            holders: vec![], locked_by: Some(USER_ID.to_string()),
        });
        assert!(!s.is_locked_out());
    }

    #[test]
    fn test_override_bypasses_lock() {
        let mut s = make_state();
        s.override_lockout = true;
        s.resource_info = Some(ResourceInfo {
            id: "r1".to_string(), name: "R".to_string(),
            device_serial: "S".to_string(), access_mode: "shared".to_string(),
            holders: vec![], locked_by: Some("other".to_string()),
        });
        assert!(!s.is_locked_out());
    }

    // --- Log ---

    #[test]
    fn test_log_and_log_err() {
        let mut s = make_state();
        s.log("msg");
        assert!(!s.log_entries[0].1);
        s.log_err("err");
        assert!(s.log_entries[1].1);
    }

    #[test]
    fn test_log_trims_at_200() {
        let mut s = make_state();
        for i in 0..210 { s.log(&format!("e{}", i)); }
        assert_eq!(s.log_entries.len(), 200);
    }

    // --- Sync messages ---

    #[test]
    fn test_url_changed() {
        let mut s = make_state();
        let _ = update(&mut s, Message::UrlChanged("http://x:1".to_string()));
        assert_eq!(s.grpc_url, "http://x:1");
    }

    #[test]
    fn test_bitfile_changed() {
        let mut s = make_state();
        let _ = update(&mut s, Message::BitfileChanged("new.bit".to_string()));
        assert_eq!(s.bitfile_path, "new.bit");
    }

    #[test]
    fn test_select_device() {
        let mut s = make_state();
        let _ = update(&mut s, Message::SelectDevice("SN123".to_string()));
        assert_eq!(s.selected_serial.as_deref(), Some("SN123"));
    }

    #[test]
    fn test_poll_interval() {
        let mut s = make_state();
        let _ = update(&mut s, Message::PollIntervalChanged(2000));
        assert_eq!(s.poll_interval_ms, 2000);
    }

    #[test]
    fn test_override_lockout_msg() {
        let mut s = make_state();
        let _ = update(&mut s, Message::OverrideLockout(true));
        assert!(s.override_lockout);
    }

    // --- Result messages ---

    #[test]
    fn test_connect_ok() {
        let mut s = make_state();
        let devs = vec![DeviceInfo { serial: "S1".to_string(), product_name: "P".to_string(), syzygy_label: String::new() }];
        let _ = update(&mut s, Message::ConnectResult(Ok(devs)));
        assert_eq!(s.phase, Phase::Connected);
        assert_eq!(s.selected_serial, None); // User must select explicitly
    }

    #[test]
    fn test_connect_err() {
        let mut s = make_state();
        let _ = update(&mut s, Message::ConnectResult(Err("fail".to_string())));
        assert_eq!(s.phase, Phase::Disconnected);
        assert!(s.log_entries.last().unwrap().1);
    }

    #[test]
    fn test_program_ok() {
        let mut s = make_state();
        s.phase = Phase::Connected;
        let _ = update(&mut s, Message::ProgramResult(Ok(())));
        assert_eq!(s.phase, Phase::Programmed);
    }

    #[test]
    fn test_register_ok() {
        let mut s = make_state();
        let info = ResourceInfo {
            id: "r1".to_string(), name: "C".to_string(),
            device_serial: "S".to_string(), access_mode: "shared".to_string(),
            holders: vec![], locked_by: None,
        };
        let _ = update(&mut s, Message::RegisterResult(Ok(info)));
        assert_eq!(s.phase, Phase::Registered);
        assert_eq!(s.resource_id.as_deref(), Some("r1"));
    }

    #[test]
    fn test_acquire_ok() {
        let mut s = make_state();
        s.phase = Phase::Registered;
        let _ = update(&mut s, Message::AcquireResult(Ok(())));
        assert_eq!(s.phase, Phase::Acquired);
    }

    #[test]
    fn test_poll_result_ok() {
        let mut s = make_state();
        let c = CounterState { count1: 42, count2: 10, ..Default::default() };
        let _ = update(&mut s, Message::PollResult(Ok((c, None))));
        assert_eq!(s.counter.count1, 42);
        assert!(s.last_error.is_none());
    }

    #[test]
    fn test_poll_result_err() {
        let mut s = make_state();
        let _ = update(&mut s, Message::PollResult(Err("timeout".to_string())));
        assert_eq!(s.last_error.as_deref(), Some("timeout"));
    }

    // ====================================================================
    // Phase 25-Fix-F: F3/F4/F5 tests
    // ====================================================================

    #[test]
    fn test_initial_bitfile_mode_is_server_path() {
        let s = make_state();
        assert_eq!(s.bitfile_mode, BitfileMode::ServerPath);
    }

    #[test]
    fn test_set_bitfile_mode_to_local_file() {
        let mut s = make_state();
        let _ = update(&mut s, Message::SetBitfileMode(BitfileMode::LocalFile));
        assert_eq!(s.bitfile_mode, BitfileMode::LocalFile);
    }

    #[test]
    fn test_set_bitfile_mode_back_to_server_path() {
        let mut s = make_state();
        s.bitfile_mode = BitfileMode::LocalFile;
        let _ = update(&mut s, Message::SetBitfileMode(BitfileMode::ServerPath));
        assert_eq!(s.bitfile_mode, BitfileMode::ServerPath);
    }

    #[test]
    fn test_register_result_409_logs_lookup_message() {
        let mut s = make_state();
        s.phase = Phase::Programmed;
        // Simulate a 409 / AlreadyExists error from RegisterResult.
        let _ = update(
            &mut s,
            Message::RegisterResult(Err("AlreadyExists: resource already exists".to_string())),
        );
        // Should log a lookup message, not crash.
        let has_lookup = s.log_entries.iter().any(|(msg, _)| msg.contains("already exists"));
        assert!(has_lookup, "Expected 'already exists' in log entries");
    }

    #[test]
    fn test_check_device_state_programmed_with_resource_skips_to_registered() {
        let mut s = make_state();
        s.phase = Phase::Connected;
        let resource = ResourceInfo {
            id: "res-1".to_string(),
            name: "Counter Resource".to_string(),
            device_serial: "SN001".to_string(),
            access_mode: "shared".to_string(),
            holders: vec![],
            locked_by: None,
        };
        let _ = update(
            &mut s,
            Message::CheckDeviceStateResult(Ok((true, Some(resource)))),
        );
        assert_eq!(s.phase, Phase::Registered);
        assert_eq!(s.resource_id.as_deref(), Some("res-1"));
    }

    #[test]
    fn test_check_device_state_programmed_no_resource_skips_to_programmed() {
        let mut s = make_state();
        s.phase = Phase::Connected;
        let _ = update(
            &mut s,
            Message::CheckDeviceStateResult(Ok((true, None))),
        );
        assert_eq!(s.phase, Phase::Programmed);
    }

    #[test]
    fn test_check_device_state_not_programmed_stays_connected() {
        let mut s = make_state();
        s.phase = Phase::Connected;
        let _ = update(
            &mut s,
            Message::CheckDeviceStateResult(Ok((false, None))),
        );
        assert_eq!(s.phase, Phase::Connected);
    }

    #[test]
    fn test_check_device_state_error_is_non_fatal() {
        let mut s = make_state();
        s.phase = Phase::Connected;
        let _ = update(
            &mut s,
            Message::CheckDeviceStateResult(Err("network error".to_string())),
        );
        // Phase should remain unchanged; error is logged but not fatal.
        assert_eq!(s.phase, Phase::Connected);
    }
}
