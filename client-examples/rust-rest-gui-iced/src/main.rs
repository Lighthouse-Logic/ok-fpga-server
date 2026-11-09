// Copyright (C) 2026 Great Bay Synergy LLC d/b/a Lighthouse Logic
// SPDX-License-Identifier: GPL-3.0-or-later OR LicenseRef-LighthouseLogic-Commercial
//! # Counters.bit Interactive Demo — Rust REST + iced
//!
//! Same functionality as the egui demo but uses the `iced` GUI framework
//! (Elm-architecture, function-based API). Uses iced 0.14's reactive
//! rendering and `iced_test` headless simulator for GUI testing.
//!
//! ## User Identity: "Icy Rester"

use iced::widget::{
    button, checkbox, column, container, pick_list,
    progress_bar, row, rule, scrollable, text, text_input, Column, Row, Space,
};
use iced::{Element, Length, Subscription, Task, Theme};
use rust_rest_gui_common::{
    CounterState, CountersClient, DeviceInfo, ResourceInfo, WireInState,
    mdns_browser::DiscoveredServer,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const DEFAULT_REST_URL: &str = "http://localhost:8088";
const DEFAULT_BITFILE: &str =
    "FrontPanel-6.1.0/ExampleBitfiles/20190501-FP50-XEM7320-A75/XEM7320-A75/Counters.bit";
const USER_ID: &str = "Icy Rester";

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // iced 0.14: first arg is a boot function (returns initial state + Task),
    // title is set via the .title() builder method instead of as a string arg.
    iced::application(boot, update, view)
        .title("Counters.bit — Icy Rester (REST + iced)")
        .theme(theme)
        .subscription(subscription)
        .window_size((720.0, 560.0))
        .run()
}

/// Theme function for iced 0.14 — must be a named function, not a closure,
/// to satisfy the higher-ranked lifetime bounds required by the builder.
fn theme(_state: &AppState) -> Theme {
    Theme::Dark
}

/// Boot function for iced 0.14 — creates the initial application state.
///
/// In iced 0.14, the `application()` constructor takes a function that
/// returns `(State, Task<Message>)` instead of using `.run_with()`.
fn boot() -> (AppState, Task<Message>) {
    let bitfile = std::env::var("OKFPGA_TEST_BITFILE")
        .unwrap_or_else(|_| DEFAULT_BITFILE.to_string());
    (
        AppState {
            rest_url: DEFAULT_REST_URL.to_string(),
            bitfile_path: bitfile,
            bitfile_mode: BitfileMode::ServerPath,
            phase: Phase::Disconnected,
            devices: vec![],
            selected_serial: None,
            resource_id: None,
            wire_in: WireInState::default(),
            poll_interval_ms: 500,
            override_lockout: false,
            counter: CounterState::default(),
            resource_info: None,
            log_entries: vec![],
            last_error: None,
            show_disconnect_dialog: false,
            discovered_servers: vec![],
            acquired_readonly: false,
        },
        // Start mDNS discovery on boot.
        Task::perform(
            async {
                match rust_rest_gui_common::mdns_browser::MdnsBrowser::new() {
                    Ok(browser) => {
                        let servers = browser.browse_for(Duration::from_secs(3)).await;
                        browser.stop();
                        servers
                    }
                    Err(e) => {
                        tracing::warn!("mDNS browse failed: {}", e);
                        vec![]
                    }
                }
            },
            Message::MdnsDiscovered,
        ),
    )
}

// ============================================================================
// State & Messages
// ============================================================================

/// Application phase — represents the sequential startup flow.
///
/// Implements `PartialOrd` so we can compare phases (e.g., `phase >= Programmed`
/// to determine which buttons should be enabled).
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Phase {
    Disconnected,
    Connected,
    Programmed,
    Registered,
    Acquired,
}

/// Bitfile configuration mode — server-side path or client-side upload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BitfileMode {
    /// Send the path to the server (server reads the file).
    ServerPath,
    /// Read the file locally and upload as binary.
    LocalFile,
}

/// The complete application state — holds everything the GUI needs.
pub struct AppState {
    pub rest_url: String,
    pub bitfile_path: String,
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
    /// Get the selected device serial, or empty string if none selected.
    pub fn serial(&self) -> String {
        self.selected_serial.clone().unwrap_or_default()
    }

    /// Check if the resource is locked by another user (not us).
    ///
    /// Returns `false` if:
    /// - The override lockout checkbox is checked
    /// - No resource info is available
    /// - The resource is not locked
    /// - The resource is locked by us (USER_ID)
    pub fn is_locked_out(&self) -> bool {
        if self.override_lockout { return false; }
        if let Some(ref res) = self.resource_info {
            if let Some(ref locker) = res.locked_by {
                return locker != USER_ID;
            }
        }
        false
    }

    /// Add a normal (non-error) log entry. Keeps at most 200 entries.
    pub fn log(&mut self, msg: &str) {
        self.log_entries.push((msg.to_string(), false));
        if self.log_entries.len() > 200 { self.log_entries.remove(0); }
    }

    /// Add an error log entry. Keeps at most 200 entries.
    pub fn log_err(&mut self, msg: &str) {
        self.log_entries.push((msg.to_string(), true));
        if self.log_entries.len() > 200 { self.log_entries.remove(0); }
    }

    /// Create a new REST client using the current URL and USER_ID.
    fn make_client(&self) -> CountersClient {
        CountersClient::new(&self.rest_url, USER_ID)
    }
}

/// All possible user actions and async operation results.
#[derive(Debug, Clone)]
pub enum Message {
    UrlChanged(String),
    BitfileChanged(String),
    ConnectPressed,
    ConnectResult(Result<Vec<DeviceInfo>, String>),
    SelectDevice(String),
    ProgramPressed,
    ProgramResult(Result<(), String>),
    RegisterSharedPressed, RegisterExclusivePressed,
    RegisterResult(Result<ResourceInfo, String>),
    AcquirePressed,
    AcquireReadOnlyPressed,
    AcquireResult(Result<(), String>),
    SetReset1(bool),
    SetDisable1(bool),
    SetAutocount2(bool),
    WireInResult(Result<(), String>),
    CountUp,
    CountDown,
    ResetCounter2,
    TriggerResult(Result<(), String>),
    PollTick,
    PollResult(Result<(CounterState, Option<ResourceInfo>), String>),
    PollIntervalChanged(u64),
    LockPressed,
    UnlockPressed,
    LockResult(Result<(), String>),
    OverrideLockout(bool),
    QuitRelease,
    QuitLeaveHeld,
    DisconnectResult(Result<(), String>),
    RefreshDevicesTick,
    RefreshDevicesResult(Result<Vec<DeviceInfo>, String>),
    DismissDisconnect,
    // Phase 25-Fix-F additions
    SetBitfileMode(BitfileMode),
    CheckDeviceState,
    CheckDeviceStateResult(Result<(bool, Option<ResourceInfo>), String>),
    // Phase 28A: mDNS discovery.
    MdnsDiscovered(Vec<DiscoveredServer>),
    MdnsSelectServer(String),
}

// ============================================================================
// Update
// ============================================================================

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::UrlChanged(url) => { state.rest_url = url; Task::none() }
        Message::BitfileChanged(path) => { state.bitfile_path = path; Task::none() }

        // Phase 28A: mDNS discovery results.
        Message::MdnsDiscovered(servers) => {
            state.log(&format!("mDNS: discovered {} server(s)", servers.len()));
            state.discovered_servers = servers;
            Task::none()
        }
        Message::MdnsSelectServer(url) => {
            state.rest_url = url;
            Task::none()
        }

        Message::ConnectPressed => {
            let client = state.make_client();
            Task::perform(
                async move {
                    let _s = client.get_server_status().await.map_err(|e| e.to_string())?;
                    client.list_devices().await.map_err(|e| e.to_string())
                },
                Message::ConnectResult,
            )
        }
        Message::ConnectResult(result) => {
            match result {
                Ok(devices) => {
                    state.log(&format!("Connected. Found {} device(s)", devices.len()));
                    state.devices = devices;
                    state.selected_serial = None; // Force user to select
                    state.phase = Phase::Connected;
                }
                Err(e) => state.log_err(&format!("Connect failed: {}", e)),
            }
            Task::none()
        }
        Message::SelectDevice(serial) => {
            state.log(&format!("Selected: {}", serial));
            state.selected_serial = Some(serial);
            // Probe device state and existing resources (F4 smart-skip).
            update(state, Message::CheckDeviceState)
        }

        Message::ProgramPressed => {
            let client = state.make_client();
            let serial = state.serial();
            let bitfile = state.bitfile_path.clone();
            let mode = state.bitfile_mode;
            Task::perform(
                async move {
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
                },
                Message::ProgramResult,
            )
        }
        Message::ProgramResult(result) => {
            match result {
                Ok(()) => { state.log("FPGA programmed"); state.phase = Phase::Programmed; }
                Err(e) => state.log_err(&format!("Program failed: {}", e)),
            }
            Task::none()
        }

        Message::RegisterSharedPressed | Message::RegisterExclusivePressed => {
            let resource_type = if matches!(message, Message::RegisterExclusivePressed) { "exclusive" } else { "shared" };
            let client = state.make_client();
            let serial = state.serial();
            let rt = resource_type.to_string();
            Task::perform(
                async move { client.create_resource("Counter Resource", &serial, &rt).await.map_err(|e| e.to_string()) },
                Message::RegisterResult,
            )
        }
        Message::RegisterResult(result) => {
            match result {
                Ok(info) => {
                    state.log(&format!("Resource registered: {}", info.id));
                    state.resource_id = Some(info.id);
                    state.phase = Phase::Registered;
                }
                Err(e) if e.contains("409") || e.contains("already exists") => {
                    // F3: 409 Conflict — resource already exists. Look it up.
                    state.log("Resource already exists — looking up...");
                    let client = state.make_client();
                    let serial = state.serial();
                    return Task::perform(
                        async move {
                            let resources = client.list_resources().await.map_err(|e| e.to_string())?;
                            let existing = resources.into_iter()
                                .find(|r| r.device_serial == serial);
                            match existing {
                                Some(info) => Ok(info),
                                None => Err("No matching resource found".to_string()),
                            }
                        },
                        Message::RegisterResult,
                    );
                }
                Err(e) => state.log_err(&format!("Register failed: {}", e)),
            }
            Task::none()
        }

        Message::AcquirePressed => {
            state.acquired_readonly = false;
            let client = state.make_client();
            let rid = state.resource_id.clone().unwrap_or_default();
            Task::perform(
                async move { client.acquire_resource(&rid).await.map(|_| ()).map_err(|e| e.to_string()) },
                Message::AcquireResult,
            )
        }
        Message::AcquireReadOnlyPressed => {
            state.acquired_readonly = true;
            let client = state.make_client();
            let rid = state.resource_id.clone().unwrap_or_default();
            Task::perform(
                async move { client.acquire_resource_readonly(&rid).await.map(|_| ()).map_err(|e| e.to_string()) },
                Message::AcquireResult,
            )
        }
        Message::AcquireResult(result) => {
            match result {
                Ok(()) => {
                    let mode_label = if state.acquired_readonly { "read-only" } else { "shared" };
                    state.log(&format!("Resource acquired ({}) — counter control active", mode_label));
                    state.phase = Phase::Acquired;
                }
                Err(e) => state.log_err(&format!("Acquire failed: {}", e)),
            }
            Task::none()
        }

        Message::SetReset1(v) => {
            state.wire_in.reset1 = v;
            let client = state.make_client(); let s = state.serial(); let w = state.wire_in.clone();
            Task::perform(async move { client.send_wire_in(&s, &w).await.map_err(|e| e.to_string()) }, Message::WireInResult)
        }
        Message::SetDisable1(v) => {
            state.wire_in.disable1 = v;
            let client = state.make_client(); let s = state.serial(); let w = state.wire_in.clone();
            Task::perform(async move { client.send_wire_in(&s, &w).await.map_err(|e| e.to_string()) }, Message::WireInResult)
        }
        Message::SetAutocount2(v) => {
            state.wire_in.autocount2 = v;
            let client = state.make_client(); let s = state.serial(); let w = state.wire_in.clone();
            Task::perform(async move { client.send_wire_in(&s, &w).await.map_err(|e| e.to_string()) }, Message::WireInResult)
        }
        Message::WireInResult(result) => {
            if let Err(e) = result { state.log_err(&format!("WireIn failed: {}", e)); }
            Task::none()
        }

        Message::CountUp => {
            let c = state.make_client(); let s = state.serial();
            Task::perform(async move { c.activate_trigger_in(&s, 0x40, 1).await.map(|_| ()).map_err(|e| e.to_string()) }, Message::TriggerResult)
        }
        Message::CountDown => {
            let c = state.make_client(); let s = state.serial();
            Task::perform(async move { c.activate_trigger_in(&s, 0x40, 2).await.map(|_| ()).map_err(|e| e.to_string()) }, Message::TriggerResult)
        }
        Message::ResetCounter2 => {
            let c = state.make_client(); let s = state.serial();
            Task::perform(async move { c.activate_trigger_in(&s, 0x40, 0).await.map(|_| ()).map_err(|e| e.to_string()) }, Message::TriggerResult)
        }
        Message::TriggerResult(result) => {
            if let Err(e) = result { state.log_err(&format!("Trigger failed: {}", e)); }
            Task::none()
        }

        Message::PollTick => {
            if state.phase != Phase::Acquired { return Task::none(); }
            let c = state.make_client(); let s = state.serial(); let rid = state.resource_id.clone();
            Task::perform(async move {
                let counter = c.poll_counter_state(&s).await.map_err(|e| e.to_string())?;
                let res = if let Some(rid) = rid { c.get_resource(&rid).await.ok() } else { None };
                Ok((counter, res))
            }, Message::PollResult)
        }
        Message::PollResult(result) => {
            match result {
                Ok((counter, res)) => { state.counter = counter; state.resource_info = res; state.last_error = None; }
                Err(e) => state.last_error = Some(e),
            }
            Task::none()
        }
        Message::PollIntervalChanged(ms) => { state.poll_interval_ms = ms; Task::none() }

        Message::LockPressed => {
            let c = state.make_client(); let rid = state.resource_id.clone().unwrap_or_default();
            Task::perform(async move { c.lock_resource(&rid).await.map(|_| ()).map_err(|e| e.to_string()) }, Message::LockResult)
        }
        Message::UnlockPressed => {
            let c = state.make_client(); let rid = state.resource_id.clone().unwrap_or_default();
            Task::perform(async move { c.unlock_resource(&rid).await.map(|_| ()).map_err(|e| e.to_string()) }, Message::LockResult)
        }
        Message::LockResult(result) => {
            match result {
                Ok(()) => state.log("Lock/unlock OK"),
                Err(e) => state.log_err(&format!("Lock/unlock failed: {}", e)),
            }
            Task::none()
        }
        Message::OverrideLockout(v) => { state.override_lockout = v; Task::none() }

        Message::QuitRelease => {
            // Phase 25-Fix-H (F8+F10): Use disconnect() instead of
            // fire-and-forget release_resource(). Task::perform awaits
            // the HTTP call before exiting, eliminating the race.
            let client = state.make_client();
            Task::perform(
                async move {
                    client.disconnect().await.map(|_| ()).map_err(|e| e.to_string())
                },
                Message::DisconnectResult,
            )
        }
        Message::QuitLeaveHeld => iced::exit(),
        Message::DisconnectResult(_) => {
            // Whether disconnect succeeded or failed, exit the app.
            iced::exit()
        }

        Message::RefreshDevicesTick => {
            let client = state.make_client();
            Task::perform(
                async move { client.list_devices().await.map_err(|e| e.to_string()) },
                Message::RefreshDevicesResult,
            )
        }
        Message::RefreshDevicesResult(result) => {
            match result {
                Ok(new_devices) => {
                    // Check if selected device disappeared.
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
                Err(e) => state.log_err(&format!("Device refresh failed: {}", e)),
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
            let client = state.make_client();
            let serial = state.serial();
            if serial.is_empty() {
                return Task::none();
            }
            Task::perform(
                async move {
                    // Check if device is programmed.
                    let dev_info = client.get_device_info(&serial).await.map_err(|e| e.to_string())?;
                    let is_programmed = dev_info["state"].as_str()
                        .map(|s| s.contains("Programmed"))
                        .unwrap_or(false);
                    // Check for existing resource.
                    let resources = client.list_resources().await.map_err(|e| e.to_string())?;
                    let existing = resources.into_iter()
                        .find(|r| r.device_serial == serial);
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
                    // Not programmed — normal flow, stay in Connected phase.
                }
                Err(e) => {
                    // Non-fatal: just log and let user proceed manually.
                    state.log(&format!("Could not check device state: {}", e));
                }
            }
            Task::none()
        }
    }
}

// ============================================================================
// Subscription (polling timer)
// ============================================================================

fn subscription(state: &AppState) -> Subscription<Message> {
    let mut subs = vec![];
    if state.phase == Phase::Acquired {
        subs.push(
            iced::time::every(Duration::from_millis(state.poll_interval_ms))
                .map(|_| Message::PollTick)
        );
    }
    // Refresh device list every 5s when connected.
    if state.phase != Phase::Disconnected {
        subs.push(
            iced::time::every(Duration::from_secs(5))
                .map(|_| Message::RefreshDevicesTick)
        );
    }
    Subscription::batch(subs)
}

// ============================================================================
// View
// ============================================================================

/// Top-level view — renders connection panel or counter control based on phase.
pub fn view(state: &AppState) -> Element<'_, Message> {
    let main_content: Element<Message> = match state.phase {
        Phase::Disconnected | Phase::Connected | Phase::Programmed | Phase::Registered => {
            view_connection(state)
        }
        Phase::Acquired => view_counters(state),
    };

    let log_entries: Vec<Element<Message>> = state.log_entries.iter().map(|(msg, is_err)| {
        let t = text(msg.clone()).size(12);
        if *is_err { t.color(iced::Color::from_rgb(1.0, 0.4, 0.4)).into() }
        else { t.color(iced::Color::from_rgb(0.7, 0.7, 0.7)).into() }
    }).collect();

    let log_panel = column![
        text("Activity Log").size(14),
        rule::horizontal(1),
        scrollable(Column::with_children(log_entries)).height(80),
    ];

    container(column![
        text("Counters.bit — Icy Rester").size(20),
        rule::horizontal(1),
        main_content,
        rule::horizontal(1),
        log_panel,
    ].spacing(8).padding(12))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_connection(state: &AppState) -> Element<'_, Message> {
    let url_row = row![
        text("Server URL:").width(100),
        text_input("http://localhost:8088", &state.rest_url).on_input(Message::UrlChanged),
    ].spacing(8);

    let connect_btn = if state.phase == Phase::Disconnected {
        button("1. Connect to Server").on_press(Message::ConnectPressed)
    } else {
        button("1. Connect to Server ✓")
    };

    // Device selection dropdown. Phase 34-B: show SYZYGY in labels.
    let device_options: Vec<String> = state.devices.iter()
        .map(|d| d.display_label())
        .collect();
    let selected_display: Option<String> = state.selected_serial.as_ref()
        .and_then(|s| state.devices.iter().find(|d| &d.serial == s))
        .map(|d| d.display_label());
    let device_picker = row![
        text("Select Device:").width(100),
        pick_list(device_options, selected_display, |picked| {
            // Extract serial from "Product (SERIAL) | ..." format.
            let serial = picked.split('(').nth(1)
                .and_then(|s| s.split(')').next())
                .unwrap_or(&picked).to_string();
            Message::SelectDevice(serial)
        }).placeholder("No device selected").width(Length::Fill),
    ].spacing(8);

    let bitfile_row = row![
        text("Bitfile:").width(100),
        text_input("path/to/Counters.bit", &state.bitfile_path).on_input(Message::BitfileChanged),
    ].spacing(8);

    // F5: Bitfile mode toggle — Server Path vs Local File.
    let mode_row = row![
        text("Mode:").width(100),
        button(if state.bitfile_mode == BitfileMode::ServerPath { "● Server Path" } else { "○ Server Path" })
            .on_press(Message::SetBitfileMode(BitfileMode::ServerPath)),
        button(if state.bitfile_mode == BitfileMode::LocalFile { "● Local File" } else { "○ Local File" })
            .on_press(Message::SetBitfileMode(BitfileMode::LocalFile)),
    ].spacing(8);

    let program_btn = if state.phase >= Phase::Programmed { button("2. Program Bitfile ✓") }
        else if state.phase == Phase::Connected && state.selected_serial.is_some() { button("2. Program Bitfile").on_press(Message::ProgramPressed) }
        else { button("2. Program Bitfile") };

    let register_btn = if state.phase >= Phase::Registered { button("3. Register Resource ✓") }
        else if state.phase == Phase::Programmed { button("3. Register (Shared)").on_press(Message::RegisterSharedPressed) }
        else { button("3. Register (Shared)") };

    let register_excl_btn = if state.phase >= Phase::Registered { button("Exclusive") }
        else if state.phase == Phase::Programmed { button("Register (Exclusive)").on_press(Message::RegisterExclusivePressed) }
        else { button("Register (Exclusive)") };

    let register_row: Element<Message> = row![register_btn, register_excl_btn].spacing(8).into();

    let acquire_btn = if state.phase >= Phase::Acquired {
        let mode_label = if state.acquired_readonly { "R/O" } else { "Shared" };
        button(text(format!("4. Acquire ({}) ✓", mode_label)))
    } else if state.phase == Phase::Registered {
        button("4. Acquire (Shared)").on_press(Message::AcquirePressed)
    } else { button("4. Acquire (Shared)") };

    let acquire_ro_btn = if state.phase >= Phase::Acquired { button("Acquire R/O") }
        else if state.phase == Phase::Registered { button("Acquire R/O").on_press(Message::AcquireReadOnlyPressed) }
        else { button("Acquire R/O") };

    let acquire_row: Element<Message> = row![acquire_btn, acquire_ro_btn].spacing(8).into();

    // Show disconnect dialog if needed.
    let disconnect_msg: Element<Message> = if state.show_disconnect_dialog {
        column![
            text("⚠ The selected device has been disconnected from the server.").color(iced::Color::from_rgb(1.0, 0.6, 0.0)),
            text("The Counter Control Panel has been closed.").color(iced::Color::from_rgb(1.0, 0.6, 0.0)),
            button("OK").on_press(Message::DismissDisconnect),
        ].spacing(4).into()
    } else { text("").into() };

    // Phase 28A: mDNS discovered servers dropdown.
    let mdns_section: Element<Message> = if !state.discovered_servers.is_empty() {
        let options: Vec<String> = state.discovered_servers.iter()
            .map(|s| {
                let port_str = s.rest_port.map(|p| format!(":{}", p)).unwrap_or_default();
                format!("{} — http://{}{}" , s.description, s.host, port_str)
            })
            .collect();
        row![
            text("Discovered (mDNS):").width(100),
            pick_list(options, Option::<String>::None, |picked| {
                // Extract the URL from the display string.
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

    column![
        disconnect_msg,
        mdns_section,
        url_row, connect_btn,
        device_picker,
        bitfile_row, mode_row, program_btn, register_row, acquire_row,
    ].spacing(8).into()
}

fn view_counters(state: &AppState) -> Element<'_, Message> {
    // ReadOnly holders are treated like locked-out users for write controls.
    let locked_out = state.is_locked_out() || state.acquired_readonly;

    let readonly_banner: Element<Message> = if state.acquired_readonly {
        text("📖 Read-Only Mode — write controls disabled").color(iced::Color::from_rgb(1.0, 0.85, 0.0)).into()
    } else { text("").into() };

    let error_text: Element<Message> = if let Some(ref e) = state.last_error {
        text(e).color(iced::Color::from_rgb(1.0, 0.3, 0.3)).into()
    } else { text("").into() };

    // Counter #1 — iced 0.14: checkbox() takes just the bool value,
    // label is set via .label() builder method.
    let c1 = column![
        text("Counter #1 (Auto-count)").size(16),
        text(format!("Value: 0x{:02X} ({})", state.counter.count1, state.counter.count1)),
        progress_bar(0.0..=255.0, state.counter.count1 as f32),
        if locked_out { checkbox(state.wire_in.reset1).label("RESET1 (hold at 0x00)") }
        else { checkbox(state.wire_in.reset1).label("RESET1 (hold at 0x00)").on_toggle(Message::SetReset1) },
        if locked_out { checkbox(state.wire_in.disable1).label("DISABLE1 (freeze)") }
        else { checkbox(state.wire_in.disable1).label("DISABLE1 (freeze)").on_toggle(Message::SetDisable1) },
        text("Triggers:").size(13),
        text(format!("  COUNT1==0x00: {}", if state.counter.count1_eq_00 {"🟢"} else {"⚫"})),
        text(format!("  COUNT1==0x80: {}", if state.counter.count1_eq_80 {"🟢"} else {"⚫"})),
    ].spacing(4).width(Length::FillPortion(1));

    // Counter #2
    let up = if locked_out { button("▲ Up") } else { button("▲ Up").on_press(Message::CountUp) };
    let dn = if locked_out { button("▼ Down") } else { button("▼ Down").on_press(Message::CountDown) };
    let rst = if locked_out { button("⟲ Reset") } else { button("⟲ Reset").on_press(Message::ResetCounter2) };

    let c2 = column![
        text("Counter #2 (Manual)").size(16),
        text(format!("Value: 0x{:02X} ({})", state.counter.count2, state.counter.count2)),
        progress_bar(0.0..=255.0, state.counter.count2 as f32),
        row![up, dn].spacing(4),
        row![rst, if locked_out { checkbox(state.wire_in.autocount2).label("AUTOCOUNT2") }
             else { checkbox(state.wire_in.autocount2).label("AUTOCOUNT2").on_toggle(Message::SetAutocount2) }
        ].spacing(4),
        text("Triggers:").size(13),
        text(format!("  COUNT2==0xFF: {}", if state.counter.count2_eq_ff {"🟢"} else {"⚫"})),
    ].spacing(4).width(Length::FillPortion(1));

    // Buttons
    let btn_indicators: Vec<Element<Message>> = (0..4).map(|i| {
        let pressed = (state.counter.buttons >> i) & 1 != 0;
        let lbl = format!("[{}]", i + 1);
        if pressed { text(lbl).color(iced::Color::from_rgb(0.0, 0.8, 0.4)).into() }
        else { text(lbl).color(iced::Color::from_rgb(0.3, 0.3, 0.3)).into() }
    }).collect();

    let poll_opts: Vec<u64> = vec![100, 250, 500, 1000, 2000];
    let poll_pick = pick_list(poll_opts, Some(state.poll_interval_ms), Message::PollIntervalChanged);

    let locked_by_me = state.resource_info.as_ref()
        .and_then(|r| r.locked_by.as_deref())
        .map(|l| l == USER_ID).unwrap_or(false);
    let lock_btn: Element<Message> = if state.acquired_readonly {
        button("🔒 Lock (N/A in R/O)").into()
    } else if locked_by_me {
        button("🔓 Unlock").on_press(Message::UnlockPressed).into()
    } else {
        button("🔒 Lock").on_press(Message::LockPressed).into()
    };

    column![
        readonly_banner,
        error_text,
        row![c1, c2].spacing(16),
        rule::horizontal(1),
        row![text("Buttons:"), Row::with_children(btn_indicators).spacing(4),
             Space::new().width(Length::Fill), text("Poll:"), poll_pick, text("ms")].spacing(8),
        row![lock_btn, checkbox(state.override_lockout).label("Override lockout").on_toggle(Message::OverrideLockout)].spacing(8),
        rule::horizontal(1),
        row![button("Quit — Release").on_press(Message::QuitRelease),
             button("Quit — Leave Held").on_press(Message::QuitLeaveHeld)].spacing(8),
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
    fn test_initial_state_defaults() {
        let state = make_state();
        assert_eq!(state.phase, Phase::Disconnected);
        assert_eq!(state.rest_url, DEFAULT_REST_URL);
        assert!(state.devices.is_empty());
        assert!(state.selected_serial.is_none());
        assert!(state.resource_id.is_none());
        assert!(!state.override_lockout);
        assert_eq!(state.poll_interval_ms, 500);
        assert!(state.log_entries.is_empty());
        assert!(state.last_error.is_none());
    }

    // --- serial() helper ---

    #[test]
    fn test_serial_returns_empty_when_none() {
        let state = make_state();
        assert_eq!(state.serial(), "");
    }

    #[test]
    fn test_serial_returns_selected() {
        let mut state = make_state();
        state.selected_serial = Some("SN001".to_string());
        assert_eq!(state.serial(), "SN001");
    }

    // --- is_locked_out() ---

    #[test]
    fn test_not_locked_out_when_no_resource_info() {
        let state = make_state();
        assert!(!state.is_locked_out());
    }

    #[test]
    fn test_not_locked_out_when_no_lock() {
        let mut state = make_state();
        state.resource_info = Some(ResourceInfo {
            id: "r1".to_string(), name: "R".to_string(),
            device_serial: "S".to_string(), access_mode: "shared".to_string(),
            holders: vec![], locked_by: None,
        });
        assert!(!state.is_locked_out());
    }

    #[test]
    fn test_not_locked_out_when_locked_by_us() {
        let mut state = make_state();
        state.resource_info = Some(ResourceInfo {
            id: "r1".to_string(), name: "R".to_string(),
            device_serial: "S".to_string(), access_mode: "shared".to_string(),
            holders: vec![], locked_by: Some(USER_ID.to_string()),
        });
        assert!(!state.is_locked_out());
    }

    #[test]
    fn test_locked_out_when_locked_by_other() {
        let mut state = make_state();
        state.resource_info = Some(ResourceInfo {
            id: "r1".to_string(), name: "R".to_string(),
            device_serial: "S".to_string(), access_mode: "shared".to_string(),
            holders: vec![], locked_by: Some("someone_else".to_string()),
        });
        assert!(state.is_locked_out());
    }

    #[test]
    fn test_override_lockout_bypasses_lock() {
        let mut state = make_state();
        state.override_lockout = true;
        state.resource_info = Some(ResourceInfo {
            id: "r1".to_string(), name: "R".to_string(),
            device_serial: "S".to_string(), access_mode: "shared".to_string(),
            holders: vec![], locked_by: Some("someone_else".to_string()),
        });
        assert!(!state.is_locked_out());
    }

    // --- Log management ---

    #[test]
    fn test_log_adds_entry() {
        let mut state = make_state();
        state.log("test message");
        assert_eq!(state.log_entries.len(), 1);
        assert_eq!(state.log_entries[0].0, "test message");
        assert!(!state.log_entries[0].1); // not an error
    }

    #[test]
    fn test_log_err_marks_as_error() {
        let mut state = make_state();
        state.log_err("error message");
        assert!(state.log_entries[0].1);
    }

    #[test]
    fn test_log_trims_to_200() {
        let mut state = make_state();
        for i in 0..210 {
            state.log(&format!("entry {}", i));
        }
        assert_eq!(state.log_entries.len(), 200);
        // First entry should be "entry 10" (entries 0-9 were trimmed)
        assert!(state.log_entries[0].0.starts_with("entry 10"));
    }

    // --- Message handling (sync messages) ---

    #[test]
    fn test_url_changed_updates_state() {
        let mut state = make_state();
        let _ = update(&mut state, Message::UrlChanged("http://new-url:1234".to_string()));
        assert_eq!(state.rest_url, "http://new-url:1234");
    }

    #[test]
    fn test_bitfile_changed_updates_state() {
        let mut state = make_state();
        let _ = update(&mut state, Message::BitfileChanged("new.bit".to_string()));
        assert_eq!(state.bitfile_path, "new.bit");
    }

    #[test]
    fn test_select_device_updates_serial() {
        let mut state = make_state();
        let _ = update(&mut state, Message::SelectDevice("SN123".to_string()));
        assert_eq!(state.selected_serial.as_deref(), Some("SN123"));
    }

    #[test]
    fn test_poll_interval_changed() {
        let mut state = make_state();
        let _ = update(&mut state, Message::PollIntervalChanged(1000));
        assert_eq!(state.poll_interval_ms, 1000);
    }

    #[test]
    fn test_override_lockout_message() {
        let mut state = make_state();
        let _ = update(&mut state, Message::OverrideLockout(true));
        assert!(state.override_lockout);
        let _ = update(&mut state, Message::OverrideLockout(false));
        assert!(!state.override_lockout);
    }

    // --- Connect result ---

    #[test]
    fn test_connect_result_ok_sets_phase() {
        let mut state = make_state();
        let devices = vec![DeviceInfo {
            serial: "SN001".to_string(), product_name: "XEM7320".to_string(),
            device_interface: "usb3".to_string(), usb_speed: "super".to_string(),
            ..Default::default()
        }];
        let _ = update(&mut state, Message::ConnectResult(Ok(devices)));
        assert_eq!(state.phase, Phase::Connected);
        assert_eq!(state.devices.len(), 1);
        assert_eq!(state.selected_serial, None); // User must select explicitly
    }

    #[test]
    fn test_connect_result_err_logs_error() {
        let mut state = make_state();
        let _ = update(&mut state, Message::ConnectResult(Err("timeout".to_string())));
        assert_eq!(state.phase, Phase::Disconnected); // unchanged
        assert!(state.log_entries.last().unwrap().1); // is error
    }

    // --- Program result ---

    #[test]
    fn test_program_result_ok_sets_phase() {
        let mut state = make_state();
        state.phase = Phase::Connected;
        let _ = update(&mut state, Message::ProgramResult(Ok(())));
        assert_eq!(state.phase, Phase::Programmed);
    }

    // --- Register result ---

    #[test]
    fn test_register_result_ok_sets_phase() {
        let mut state = make_state();
        state.phase = Phase::Programmed;
        let info = ResourceInfo {
            id: "res-abc".to_string(), name: "Counter".to_string(),
            device_serial: "S".to_string(), access_mode: "shared".to_string(),
            holders: vec![], locked_by: None,
        };
        let _ = update(&mut state, Message::RegisterResult(Ok(info)));
        assert_eq!(state.phase, Phase::Registered);
        assert_eq!(state.resource_id.as_deref(), Some("res-abc"));
    }

    // --- Acquire result ---

    #[test]
    fn test_acquire_result_ok_sets_phase() {
        let mut state = make_state();
        state.phase = Phase::Registered;
        let _ = update(&mut state, Message::AcquireResult(Ok(())));
        assert_eq!(state.phase, Phase::Acquired);
    }

    // --- Wire in result ---

    #[test]
    fn test_wire_in_result_err_logs() {
        let mut state = make_state();
        let _ = update(&mut state, Message::WireInResult(Err("fail".to_string())));
        assert!(state.log_entries.last().unwrap().1);
    }

    // --- Poll result ---

    #[test]
    fn test_poll_result_updates_counter() {
        let mut state = make_state();
        state.phase = Phase::Acquired;
        let counter = CounterState {
            count1: 42, count2: 128, buttons: 0x0F,
            count1_eq_00: true, count1_eq_80: false, count2_eq_ff: false,
        };
        let _ = update(&mut state, Message::PollResult(Ok((counter.clone(), None))));
        assert_eq!(state.counter.count1, 42);
        assert_eq!(state.counter.count2, 128);
        assert!(state.last_error.is_none());
    }

    #[test]
    fn test_poll_result_err_sets_last_error() {
        let mut state = make_state();
        let _ = update(&mut state, Message::PollResult(Err("oops".to_string())));
        assert_eq!(state.last_error.as_deref(), Some("oops"));
    }

    // ========================================================================
    // Phase 25-Fix-F tests
    // ========================================================================

    // --- F5: Bitfile mode ---

    #[test]
    fn test_initial_bitfile_mode_is_server_path() {
        let state = make_state();
        assert_eq!(state.bitfile_mode, BitfileMode::ServerPath);
    }

    #[test]
    fn test_set_bitfile_mode_to_local_file() {
        let mut state = make_state();
        let _ = update(&mut state, Message::SetBitfileMode(BitfileMode::LocalFile));
        assert_eq!(state.bitfile_mode, BitfileMode::LocalFile);
    }

    #[test]
    fn test_set_bitfile_mode_back_to_server_path() {
        let mut state = make_state();
        let _ = update(&mut state, Message::SetBitfileMode(BitfileMode::LocalFile));
        let _ = update(&mut state, Message::SetBitfileMode(BitfileMode::ServerPath));
        assert_eq!(state.bitfile_mode, BitfileMode::ServerPath);
    }

    // --- F3: 409 handling in RegisterResult ---

    #[test]
    fn test_register_result_409_logs_lookup_message() {
        let mut state = make_state();
        state.phase = Phase::Programmed;
        state.selected_serial = Some("SN001".to_string());
        let _ = update(&mut state, Message::RegisterResult(
            Err("409 Conflict: resource already exists".to_string()),
        ));
        // On 409, it should log a lookup message (not an error).
        let has_lookup_msg = state.log_entries.iter()
            .any(|(msg, is_err)| msg.contains("looking up") && !is_err);
        assert!(has_lookup_msg, "Expected a 'looking up' log entry on 409");
        // Phase should NOT advance (the lookup is async, so we stay where we are).
        assert_eq!(state.phase, Phase::Programmed);
    }

    // --- F4: CheckDeviceStateResult smart-skip ---

    #[test]
    fn test_check_device_state_programmed_with_resource_skips_to_registered() {
        let mut state = make_state();
        state.phase = Phase::Connected;
        state.selected_serial = Some("SN001".to_string());

        let resource = ResourceInfo {
            id: "r1".to_string(),
            name: "Counter Resource".to_string(),
            device_serial: "SN001".to_string(),
            access_mode: "shared".to_string(),
            holders: vec![],
            locked_by: None,
        };
        let _ = update(&mut state, Message::CheckDeviceStateResult(
            Ok((true, Some(resource))),
        ));
        assert_eq!(state.phase, Phase::Registered);
        assert_eq!(state.resource_id.as_deref(), Some("r1"));
    }

    #[test]
    fn test_check_device_state_programmed_no_resource_skips_to_programmed() {
        let mut state = make_state();
        state.phase = Phase::Connected;
        state.selected_serial = Some("SN001".to_string());

        let _ = update(&mut state, Message::CheckDeviceStateResult(
            Ok((true, None)),
        ));
        assert_eq!(state.phase, Phase::Programmed);
        assert!(state.resource_id.is_none());
    }

    #[test]
    fn test_check_device_state_not_programmed_stays_connected() {
        let mut state = make_state();
        state.phase = Phase::Connected;
        state.selected_serial = Some("SN001".to_string());

        let _ = update(&mut state, Message::CheckDeviceStateResult(
            Ok((false, None)),
        ));
        assert_eq!(state.phase, Phase::Connected);
    }

    #[test]
    fn test_check_device_state_error_is_non_fatal() {
        let mut state = make_state();
        state.phase = Phase::Connected;
        state.selected_serial = Some("SN001".to_string());

        let _ = update(&mut state, Message::CheckDeviceStateResult(
            Err("network error".to_string()),
        ));
        // Phase stays Connected (non-fatal).
        assert_eq!(state.phase, Phase::Connected);
        // But a log entry should exist.
        let has_log = state.log_entries.iter()
            .any(|(msg, _)| msg.contains("Could not check device state"));
        assert!(has_log);
    }
}
