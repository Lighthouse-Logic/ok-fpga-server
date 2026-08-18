//! # Counters.bit Interactive Demo — Rust gRPC + iced
//!
//! Same UI as the REST+iced demo but communicates via gRPC.
//!
//! ## User Identity: "Icy Garpac"
//!
//! ## Note on Architecture
//!
//! Because iced Messages must be Debug+Clone, and tonic's gRPC client
//! isn't Clone, each async operation creates a fresh gRPC connection.
//! This is fine for a demo — production code would use a shared channel.

use iced::widget::{
    button, checkbox, column, container, horizontal_rule, horizontal_space, pick_list,
    progress_bar, row, scrollable, text, text_input, Column, Row,
};
use iced::{Element, Length, Subscription, Task, Theme};
use rust_grpc_gui_common::{CounterState, CountersGrpcClient, DeviceInfo, ResourceInfo, WireInState};
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

    iced::application("Counters.bit — Icy Garpac (gRPC + iced)", update, view)
        .theme(|_| Theme::Dark)
        .subscription(subscription)
        .window_size((720.0, 560.0))
        .run_with(|| {
            let bitfile = std::env::var("OKFPGA_TEST_BITFILE")
                .unwrap_or_else(|_| DEFAULT_BITFILE.to_string());
            (AppState::new(bitfile), Task::none())
        })
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum Phase { Disconnected, Connected, Programmed, Registered, Acquired }

struct AppState {
    grpc_url: String,
    bitfile_path: String,
    phase: Phase,
    devices: Vec<DeviceInfo>,
    selected_serial: Option<String>,
    resource_id: Option<String>,
    wire_in: WireInState,
    poll_interval_ms: u64,
    override_lockout: bool,
    counter: CounterState,
    resource_info: Option<ResourceInfo>,
    log_entries: Vec<(String, bool)>,
    last_error: Option<String>,
}

impl AppState {
    fn new(bitfile: String) -> Self {
        Self {
            grpc_url: DEFAULT_GRPC_URL.to_string(), bitfile_path: bitfile,
            phase: Phase::Disconnected, devices: vec![], selected_serial: None,
            resource_id: None, wire_in: WireInState::default(), poll_interval_ms: 500,
            override_lockout: false, counter: CounterState::default(), resource_info: None,
            log_entries: vec![], last_error: None,
        }
    }
    fn serial(&self) -> String { self.selected_serial.clone().unwrap_or_default() }
    fn is_locked_out(&self) -> bool {
        if self.override_lockout { return false; }
        self.resource_info.as_ref().and_then(|r| r.locked_by.as_deref()).map(|l| l != USER_ID).unwrap_or(false)
    }
    fn log(&mut self, msg: &str) { self.log_entries.push((msg.to_string(), false)); if self.log_entries.len() > 200 { self.log_entries.remove(0); } }
    fn log_err(&mut self, msg: &str) { self.log_entries.push((msg.to_string(), true)); if self.log_entries.len() > 200 { self.log_entries.remove(0); } }
}

/// Helper: create a fresh gRPC client connection.
async fn connect(url: &str) -> Result<CountersGrpcClient, String> {
    CountersGrpcClient::connect(url, USER_ID).await.map_err(|e| e.to_string())
}

#[derive(Debug, Clone)]
enum Message {
    UrlChanged(String), BitfileChanged(String),
    ConnectPressed, ConnectResult(Result<Vec<DeviceInfo>, String>),
    SelectDevice(String),
    ProgramPressed, ProgramResult(Result<(), String>),
    RegisterPressed, RegisterResult(Result<ResourceInfo, String>),
    AcquirePressed, AcquireResult(Result<(), String>),
    SetReset1(bool), SetDisable1(bool), SetAutocount2(bool), WireInResult(Result<(), String>),
    CountUp, CountDown, ResetCounter2, TriggerResult(Result<(), String>),
    PollTick, PollResult(Result<(CounterState, Option<ResourceInfo>), String>),
    PollIntervalChanged(u64),
    LockPressed, UnlockPressed, LockResult(Result<(), String>),
    OverrideLockout(bool),
    QuitRelease, QuitLeaveHeld,
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
    match message {
        Message::UrlChanged(u) => { state.grpc_url = u; Task::none() }
        Message::BitfileChanged(p) => { state.bitfile_path = p; Task::none() }
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
                    if let Some(d) = devices.first() { state.selected_serial = Some(d.serial.clone()); }
                    state.devices = devices; state.phase = Phase::Connected;
                }
                Err(e) => state.log_err(&format!("Connect: {}", e)),
            } Task::none()
        }
        Message::SelectDevice(s) => { state.log(&format!("Selected: {}", s)); state.selected_serial = Some(s); Task::none() }
        Message::ProgramPressed => {
            let url = state.grpc_url.clone(); let s = state.serial(); let bf = state.bitfile_path.clone();
            Task::perform(async move {
                let mut c = connect(&url).await?;
                c.configure_fpga(&s, &bf).await.map_err(|e| e.to_string())
            }, Message::ProgramResult)
        }
        Message::ProgramResult(r) => { match r { Ok(()) => { state.log("Programmed"); state.phase = Phase::Programmed; } Err(e) => state.log_err(&e) } Task::none() }
        Message::RegisterPressed => {
            let url = state.grpc_url.clone(); let s = state.serial();
            Task::perform(async move {
                let mut c = connect(&url).await?;
                c.create_resource("Counter Resource", &s).await.map_err(|e| e.to_string())
            }, Message::RegisterResult)
        }
        Message::RegisterResult(r) => { match r { Ok(i) => { state.log(&format!("Registered: {}", i.id)); state.resource_id = Some(i.id); state.phase = Phase::Registered; } Err(e) => state.log_err(&e) } Task::none() }
        Message::AcquirePressed => {
            let url = state.grpc_url.clone(); let rid = state.resource_id.clone().unwrap();
            Task::perform(async move {
                let mut c = connect(&url).await?;
                c.acquire_resource(&rid).await.map_err(|e| e.to_string())
            }, Message::AcquireResult)
        }
        Message::AcquireResult(r) => { match r { Ok(()) => { state.log("Acquired"); state.phase = Phase::Acquired; } Err(e) => state.log_err(&e) } Task::none() }
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
            let url = state.grpc_url.clone();
            if let Some(rid) = state.resource_id.clone() {
                tokio::spawn(async move { if let Ok(mut c) = connect(&url).await { let _ = c.release_resource(&rid).await; } });
            }
            iced::exit()
        }
        Message::QuitLeaveHeld => iced::exit(),
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
    if state.phase == Phase::Acquired { iced::time::every(Duration::from_millis(state.poll_interval_ms)).map(|_| Message::PollTick) }
    else { Subscription::none() }
}

fn view(state: &AppState) -> Element<Message> {
    let main: Element<Message> = if state.phase == Phase::Acquired { view_counters(state) } else { view_connect(state) };
    let log_items: Vec<Element<Message>> = state.log_entries.iter().map(|(m, e)| {
        let t = text(m.clone()).size(12); if *e { t.color(iced::Color::from_rgb(1.0,0.4,0.4)).into() } else { t.color(iced::Color::from_rgb(0.7,0.7,0.7)).into() }
    }).collect();
    container(column![
        text("Counters.bit — Icy Garpac").size(20), horizontal_rule(1), main, horizontal_rule(1),
        text("Activity Log").size(14), scrollable(Column::with_children(log_items)).height(80),
    ].spacing(8).padding(12)).width(Length::Fill).height(Length::Fill).into()
}

fn view_connect(s: &AppState) -> Element<Message> {
    let con = if s.phase == Phase::Disconnected { button("1. Connect").on_press(Message::ConnectPressed) } else { button("1. Connect ✓") };
    let devs: Vec<Element<Message>> = s.devices.iter().map(|d| button(text(format!("{} — {}", d.serial, d.product_name)).size(14)).on_press(Message::SelectDevice(d.serial.clone())).into()).collect();
    let prog = if s.phase >= Phase::Programmed { button("2. Program ✓") } else if s.phase == Phase::Connected { button("2. Program").on_press(Message::ProgramPressed) } else { button("2. Program") };
    let reg = if s.phase >= Phase::Registered { button("3. Register ✓") } else if s.phase == Phase::Programmed { button("3. Register").on_press(Message::RegisterPressed) } else { button("3. Register") };
    let acq = if s.phase >= Phase::Acquired { button("4. Acquire ✓") } else if s.phase == Phase::Registered { button("4. Acquire").on_press(Message::AcquirePressed) } else { button("4. Acquire") };
    column![
        row![text("gRPC URL:").width(100), text_input("http://localhost:50051", &s.grpc_url).on_input(Message::UrlChanged)].spacing(8),
        con, Column::with_children(devs).spacing(2),
        row![text("Bitfile:").width(100), text_input("path", &s.bitfile_path).on_input(Message::BitfileChanged)].spacing(8),
        prog, reg, acq,
    ].spacing(8).into()
}

fn view_counters(s: &AppState) -> Element<Message> {
    let lo = s.is_locked_out();
    let err: Element<Message> = if let Some(ref e) = s.last_error { text(e).color(iced::Color::from_rgb(1.0,0.3,0.3)).into() } else { text("").into() };
    let c1 = column![text("Counter #1").size(16), text(format!("0x{:02X}", s.counter.count1)), progress_bar(0.0..=255.0, s.counter.count1 as f32),
        if lo { checkbox("RESET1", s.wire_in.reset1) } else { checkbox("RESET1", s.wire_in.reset1).on_toggle(Message::SetReset1) },
        if lo { checkbox("DISABLE1", s.wire_in.disable1) } else { checkbox("DISABLE1", s.wire_in.disable1).on_toggle(Message::SetDisable1) },
        text(format!("==0x00: {} | ==0x80: {}", if s.counter.count1_eq_00 {"🟢"} else {"⚫"}, if s.counter.count1_eq_80 {"🟢"} else {"⚫"})),
    ].spacing(4).width(Length::FillPortion(1));
    let c2 = column![text("Counter #2").size(16), text(format!("0x{:02X}", s.counter.count2)), progress_bar(0.0..=255.0, s.counter.count2 as f32),
        row![if lo { button("▲") } else { button("▲").on_press(Message::CountUp) }, if lo { button("▼") } else { button("▼").on_press(Message::CountDown) }].spacing(4),
        row![if lo { button("⟲") } else { button("⟲").on_press(Message::ResetCounter2) }, if lo { checkbox("AUTO", s.wire_in.autocount2) } else { checkbox("AUTO", s.wire_in.autocount2).on_toggle(Message::SetAutocount2) }].spacing(4),
        text(format!("==0xFF: {}", if s.counter.count2_eq_ff {"🟢"} else {"⚫"})),
    ].spacing(4).width(Length::FillPortion(1));
    let btns: Vec<Element<Message>> = (0..4).map(|i| { let p = (s.counter.buttons >> i) & 1 != 0; if p { text(format!("[{}]",i+1)).color(iced::Color::from_rgb(0.0,0.8,0.4)).into() } else { text(format!("[{}]",i+1)).color(iced::Color::from_rgb(0.3,0.3,0.3)).into() } }).collect();
    let poll_opts: Vec<u64> = vec![100,250,500,1000,2000];
    let me = s.resource_info.as_ref().and_then(|r| r.locked_by.as_deref()).map(|l| l == USER_ID).unwrap_or(false);
    let lock = if me { button("🔓 Unlock").on_press(Message::UnlockPressed) } else { button("🔒 Lock").on_press(Message::LockPressed) };
    column![err, row![c1, c2].spacing(16), horizontal_rule(1),
        row![text("Buttons:"), Row::with_children(btns).spacing(4), horizontal_space(), text("Poll:"), pick_list(poll_opts, Some(s.poll_interval_ms), Message::PollIntervalChanged), text("ms")].spacing(8),
        row![lock, checkbox("Override", s.override_lockout).on_toggle(Message::OverrideLockout)].spacing(8), horizontal_rule(1),
        row![button("Quit — Release").on_press(Message::QuitRelease), button("Quit — Leave Held").on_press(Message::QuitLeaveHeld)].spacing(8),
    ].spacing(8).into()
}
