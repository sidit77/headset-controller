#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::future::Future;
use std::pin::Pin;
use async_hid::{Device as HidDevice, DeviceInfo};
use color_eyre::Result;
use tokio::runtime::Builder;
use tokio::signal::ctrl_c;
use tokio::spawn;
use tokio::task::JoinHandle;
use tracing::instrument;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Interface {
    pub product_id: u16,
    pub vendor_id: u16,
    pub usage_id: u16,
    pub usage_page: u16
}

impl Interface {
    pub const fn new(usage_page: u16, usage_id: u16, vendor_id: u16, product_id: u16) -> Self {
        Self { product_id, vendor_id, usage_id, usage_page }
    }
}

impl From<&DeviceInfo> for Interface {
    fn from(value: &DeviceInfo) -> Self {
        Interface::new(value.usage_page, value.usage_id, value.vendor_id, value.product_id)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct SupportedDevice {
    name: &'static str,
    required_interfaces: &'static [Interface],
    open: fn(interfaces: &HashMap<Interface, DeviceInfo>) -> BoxedDeviceFuture
}

impl Display for SupportedDevice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

pub const ARCTIS_NOVA_X: SupportedDevice = SupportedDevice {
    name: "Steelseries Arctis Nova X",
    required_interfaces: &[
        Interface::new(0xFFC0, 0x1, 0x1038, 0x2206),
        Interface::new(0xFF00, 0x1, 0x1038, 0x2206)
    ],
    open: ArctisNova::open_xbox,
};

//pub const DUMMY_DEVICE: SupportedDevice = SupportedDevice {
//    name: "Dummy Device",
//    required_interfaces: &[],
//};


pub const SUPPORTED_DEVICES: &[SupportedDevice] = &[ARCTIS_NOVA_X];

#[derive(Debug, Clone, Default)]
pub struct DeviceManager {
    interfaces: HashMap<Interface, DeviceInfo>,
    devices: Vec<SupportedDevice>
}

impl DeviceManager {

    pub async fn new() -> Result<Self> {
        let mut result = Self::default();
        result.refresh().await?;
        Ok(result)
    }

    #[instrument(skip_all)]
    pub async fn refresh(&mut self) -> Result<()> {
        self.interfaces.clear();
        self.interfaces.extend(DeviceInfo::enumerate()
            .await?
            .into_iter()
            .map(|dev| (Interface::from(&dev), dev)));

        self.devices.clear();
        self.devices.extend(SUPPORTED_DEVICES
            .iter()
            .filter(|dev| dev
                .required_interfaces
                .iter()
                .all(|i| self.interfaces.contains_key(i)))
            .inspect(|dev| println!("Found {}", dev.name)));

        Ok(())
    }

    pub fn supported_devices(&self) -> &Vec<SupportedDevice> {
        &self.devices
    }

    pub async fn open(&self, supported: &SupportedDevice) -> Result<BoxedDevice> {
        println!("Opening {}", supported.name);
        let dev = (supported.open)(&self.interfaces).await?;

        Ok(dev)
    }
}

pub trait Device {
    fn is_connected(&self) -> bool;
}
pub type BoxedDevice = Box<dyn Device>;
pub type BoxedDeviceFuture<'a> = Pin<Box<dyn Future<Output=Result<BoxedDevice>> + 'a>>;

const STEELSERIES: u16 = 0x1038;

const PID_ARCTIS_NOVA_7: u16 = 0x2202;
const PID_ARCTIS_NOVA_7X: u16 = 0x2206;
const PID_ARCTIS_NOVA_7P: u16 = 0x220a;


pub struct ArctisNova {
    update_task: JoinHandle<()>,
    config_channel: HidDevice,
    name: &'static str,
    connected: bool
}

impl ArctisNova {
    
    async fn open(name: &'static str, pid: u16, interfaces: &HashMap<Interface, DeviceInfo>) -> Result<BoxedDevice> {
        let notification_channel = interfaces
            .get(&Interface::new(0xFF00, 0x1, 0x1038, pid))
            .unwrap()
            .open()
            .await?;

        let update_task = spawn(async move {
            let mut buf = [0u8; 8];
            loop {
                notification_channel.read_input_report(&mut buf)
                    .await
                    .unwrap_or_else(|err| {
                        println!("notification task: {}", err);
                        0
                    });
                println!("{:?}", buf);
            }
        });

        let config_channel = interfaces
            .get(&Interface::new(0xFFC0, 0x1, 0x1038, pid))
            .unwrap()
            .open()
            .await?;
        
        Ok(Box::new(Self {
            update_task,
            config_channel,
            name,
            connected: false,
        }))
    }
    
    pub fn open_xbox(interfaces: &HashMap<Interface, DeviceInfo>) -> BoxedDeviceFuture {
        Box::pin(Self::open("Steelseries Arctis Nova 7X", PID_ARCTIS_NOVA_7X, interfaces))
    }
    
}

impl Drop for ArctisNova {
    fn drop(&mut self) {
        self.update_task.abort();
        println!("Drop");
    }
}

impl Device for ArctisNova {
    fn is_connected(&self) -> bool {
        self.connected
    }
}



fn main() -> Result<()> {
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let manager = DeviceManager::new().await?;
        let dev = manager
            .supported_devices()
            .first()
            .unwrap();

        let dev = manager.open(dev).await?;

        ctrl_c().await?;
        Ok(())
    })
}

/*
mod audio;
mod config;
mod debouncer;
mod devices;
mod notification;
mod renderer;
mod tray;
mod ui;
mod util;

use std::ops::Not;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use color_eyre::Result;
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoop};
use tao::platform::run_return::EventLoopExtRunReturn;
use tracing::instrument;
use tracing_error::ErrorLayer;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::audio::AudioSystem;
use crate::config::{log_file, Config, EqualizerConfig, HeadsetConfig, START_QUIET, CLOSE_IMMEDIATELY};
use crate::debouncer::{Action, Debouncer};
use crate::devices::{BatteryLevel, BoxedDevice, Device, DeviceManager};
use crate::renderer::EguiWindow;
use crate::tray::{AppTray, TrayEvent};

fn main() -> Result<()> {
    color_eyre::install()?;
    let logfile = Mutex::new(log_file());
    tracing_subscriber::registry()
        .with(ErrorLayer::default())
        .with(Targets::new().with_default(LevelFilter::TRACE))
        .with(layer().without_time())
        .with(layer().with_ansi(false).with_writer(logfile))
        .init();

    let span = tracing::info_span!("init").entered();

    let mut config = Config::load()?;

    let mut audio_system = AudioSystem::new();

    let mut device_manager = DeviceManager::new()?;
    let mut device = device_manager.find_preferred_device(&config.preferred_device);

    let mut event_loop = EventLoop::new();

    let mut tray = AppTray::new(&event_loop);

    let mut window: Option<EguiWindow> = START_QUIET
        .not()
        .then(|| EguiWindow::new(&event_loop));

    let mut next_device_poll = Instant::now();
    let mut debouncer = Debouncer::new();
    debouncer.submit_all([Action::UpdateSystemAudio, Action::UpdateTrayTooltip, Action::UpdateTray]);

    span.exit();
    event_loop.run_return(move |event, event_loop, control_flow| {
        if window
            .as_mut()
            .map(|w| {
                w.handle_events(&event, |egui_ctx| match &device {
                    Some(device) => ui::config_ui(
                        egui_ctx,
                        &mut debouncer,
                        &mut config,
                        device.as_ref(),
                        device_manager.supported_devices(),
                        &mut audio_system
                    ),
                    None => ui::no_device_ui(egui_ctx, &mut debouncer)
                })
            })
            .unwrap_or(false)
        {
            debouncer.force(Action::SaveConfig);
            window.take();
            if *CLOSE_IMMEDIATELY {
                *control_flow = ControlFlow::Exit;
            }
        }

        match event {
            Event::MenuEvent { menu_id, .. } => {
                let _span = tracing::info_span!("tray_menu_event").entered();
                match tray.handle_event(menu_id) {
                    Some(TrayEvent::Open) => {
                        audio_system.refresh_devices();
                        match &mut window {
                            None => window = Some(EguiWindow::new(event_loop)),
                            Some(window) => {
                                window.focus();
                            }
                        }
                    }
                    Some(TrayEvent::Quit) => {
                        *control_flow = ControlFlow::Exit;
                    }
                    Some(TrayEvent::Profile(id)) => {
                        let _span = tracing::info_span!("profile_change", id).entered();
                        if let Some(device) = &device {
                            let headset = config.get_headset(device.name());
                            if id as u32 != headset.selected_profile_index {
                                let len = headset.profiles.len();
                                if id < len {
                                    headset.selected_profile_index = id as u32;
                                    submit_profile_change(&mut debouncer);
                                    debouncer.submit_all([Action::SaveConfig, Action::UpdateTray]);
                                } else {
                                    tracing::warn!(len, "Profile id out of range")
                                }
                            } else {
                                tracing::trace!("Profile already selected");
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::NewEvents(_) | Event::LoopDestroyed => {
                while let Some(action) = debouncer.next() {
                    let _span = tracing::info_span!("debouncer_event", ?action).entered();
                    tracing::trace!("Processing event");
                    match action {
                        Action::RefreshDeviceList => device_manager
                            .refresh()
                            .unwrap_or_else(|err| tracing::warn!("Failed to refresh devices: {}", err)),
                        Action::SwitchDevice => {
                            if config.preferred_device != device.as_ref().map(|d| d.name().to_string()) {
                                device = device_manager.find_preferred_device(&config.preferred_device);
                                submit_full_change(&mut debouncer);
                                debouncer.submit_all([Action::UpdateTray, Action::UpdateTrayTooltip]);
                            } else {
                                tracing::debug!("Preferred device is already active")
                            }
                        }
                        Action::UpdateSystemAudio => {
                            if let Some(device) = &device {
                                let headset = config.get_headset(device.name());
                                audio_system.apply(&headset.os_audio, device.is_connected())
                            }
                        }
                        Action::SaveConfig => {
                            config
                                .save()
                                .unwrap_or_else(|err| tracing::warn!("Could not save config: {:?}", err));
                        }
                        Action::UpdateTray => update_tray(&mut tray, &mut config, device.as_ref().map(|d| d.name())),
                        Action::UpdateTrayTooltip => update_tray_tooltip(&mut tray, &device),
                        action => {
                            if let Some(device) = &device {
                                let headset = config.get_headset(device.name());
                                apply_config_to_device(action, device.as_ref(), headset)
                            }
                        }
                    }
                }
                if let Some(device) = &mut device {
                    if next_device_poll <= Instant::now() {
                        let _span = tracing::info_span!("device_poll").entered();
                        let (last_connected, last_battery) = (device.is_connected(), device.get_battery_status());
                        next_device_poll = Instant::now()
                            + device
                                .poll()
                                .map_err(|err| tracing::warn!("Failed to poll device: {:?}", err))
                                .unwrap_or(Duration::from_secs(10));

                        if last_connected != device.is_connected() {
                            let mut msg = match device.is_connected() {
                                true => "Connected",
                                false => "Disconnected"
                            }
                            .to_string();
                            let battery = [last_battery, device.get_battery_status()]
                                .into_iter()
                                .filter_map(|b| match b {
                                    Some(BatteryLevel::Level(l)) => Some(l),
                                    _ => None
                                })
                                .min();
                            if let Some(level) = battery {
                                msg = format!("{} (Battery: {}%)", msg, level);
                            }
                            notification::notify(device.name(), &msg, Duration::from_secs(2))
                                .unwrap_or_else(|err| tracing::warn!("Can not create notification: {:?}", err));
                            debouncer.submit(Action::UpdateSystemAudio);
                            debouncer.force(Action::UpdateSystemAudio);
                        }
                        if last_battery != device.get_battery_status() {
                            debouncer.submit(Action::UpdateTrayTooltip);
                        }
                    }
                }
            }
            _ => ()
        }
        if !matches!(*control_flow, ControlFlow::ExitWithCode(_)) {
            let next_window_update = window.as_ref().and_then(|w| w.next_repaint());
            let next_update = [device.as_ref().map(|_| next_device_poll), next_window_update, debouncer.next_action()]
                .into_iter()
                .flatten()
                .min();
            *control_flow = match next_update {
                Some(next_update) => match next_update <= Instant::now() {
                    true => ControlFlow::Poll,
                    false => ControlFlow::WaitUntil(next_update)
                },
                None => ControlFlow::Wait
            };
        }
    });
    Ok(())
}

#[instrument(skip_all)]
fn submit_profile_change(debouncer: &mut Debouncer) {
    let actions = [
        Action::UpdateSideTone,
        Action::UpdateEqualizer,
        Action::UpdateMicrophoneVolume,
        Action::UpdateVolumeLimit
    ];
    debouncer.submit_all(actions);
    debouncer.force_all(actions);
}

#[instrument(skip_all)]
fn submit_full_change(debouncer: &mut Debouncer) {
    submit_profile_change(debouncer);
    let actions = [
        Action::UpdateMicrophoneLight,
        Action::UpdateInactiveTime,
        Action::UpdateBluetoothCall,
        Action::UpdateAutoBluetooth,
        Action::UpdateSystemAudio
    ];
    debouncer.submit_all(actions);
    debouncer.force_all(actions);
}

#[instrument(skip_all, fields(name = %device.name()))]
fn apply_config_to_device(action: Action, device: &dyn Device, headset: &mut HeadsetConfig) {
    if device.is_connected() {
        match action {
            Action::UpdateSideTone => {
                if let Some(sidetone) = device.get_side_tone() {
                    let _span = tracing::info_span!("sidetone").entered();
                    sidetone
                        .set_level(headset.selected_profile().side_tone)
                        .unwrap_or_else(|err| tracing::warn!("Can not apply side tone: {:?}", err));
                }
            }
            Action::UpdateEqualizer => {
                if let Some(equalizer) = device.get_equalizer() {
                    let _span = tracing::info_span!("equalizer").entered();
                    let levels = match headset.selected_profile().equalizer.clone() {
                        EqualizerConfig::Preset(i) => equalizer
                            .presets()
                            .get(i as usize)
                            .expect("Unknown preset")
                            .1
                            .to_vec(),
                        EqualizerConfig::Custom(levels) => levels
                    };
                    equalizer
                        .set_levels(&levels)
                        .unwrap_or_else(|err| tracing::warn!("Could not apply equalizer: {:?}", err));
                }
            }
            Action::UpdateMicrophoneVolume => {
                if let Some(mic_volume) = device.get_mic_volume() {
                    let _span = tracing::info_span!("mic_volume").entered();
                    mic_volume
                        .set_level(headset.selected_profile().microphone_volume)
                        .unwrap_or_else(|err| tracing::warn!("Could not apply microphone volume: {:?}", err));
                }
            }
            Action::UpdateVolumeLimit => {
                if let Some(volume_limiter) = device.get_volume_limiter() {
                    let _span = tracing::info_span!("volume_limiter").entered();
                    volume_limiter
                        .set_enabled(headset.selected_profile().volume_limiter)
                        .unwrap_or_else(|err| tracing::warn!("Could not apply volume limited: {:?}", err));
                }
            }
            Action::UpdateInactiveTime => {
                if let Some(inactive_time) = device.get_inactive_time() {
                    let _span = tracing::info_span!("inactive time").entered();
                    inactive_time
                        .set_inactive_time(headset.inactive_time)
                        .unwrap_or_else(|err| tracing::warn!("Could not apply inactive time: {:?}", err));
                }
            }
            Action::UpdateMicrophoneLight => {
                if let Some(mic_light) = device.get_mic_light() {
                    let _span = tracing::info_span!("mic_light").entered();
                    mic_light
                        .set_light_strength(headset.mic_light)
                        .unwrap_or_else(|err| tracing::warn!("Could not apply microphone light: {:?}", err));
                }
            }
            Action::UpdateBluetoothCall => {
                if let Some(bluetooth_config) = device.get_bluetooth_config() {
                    let _span = tracing::info_span!("bluetooth").entered();
                    bluetooth_config
                        .set_auto_enabled(headset.auto_enable_bluetooth)
                        .unwrap_or_else(|err| tracing::warn!("Could not set bluetooth auto enabled: {:?}", err));
                }
            }
            Action::UpdateAutoBluetooth => {
                if let Some(bluetooth_config) = device.get_bluetooth_config() {
                    let _span = tracing::info_span!("bluetooth").entered();
                    bluetooth_config
                        .set_call_action(headset.bluetooth_call)
                        .unwrap_or_else(|err| tracing::warn!("Could not set call action: {:?}", err));
                }
            }
            _ => tracing::warn!("{:?} is not related to the device", action)
        }
    }
}

#[instrument(skip_all)]
pub fn update_tray(tray: &mut AppTray, config: &mut Config, device_name: Option<&str>) {
    match device_name {
        None => {
            tray.build_menu(0, |_| ("", false));
        }
        Some(device_name) => {
            let headset = config.get_headset(device_name);
            let selected = headset.selected_profile_index as usize;
            let profiles = &headset.profiles;
            tray.build_menu(profiles.len(), |id| (profiles[id].name.as_str(), id == selected));
        }
    }
}

#[instrument(skip_all)]
pub fn update_tray_tooltip(tray: &mut AppTray, device: &Option<BoxedDevice>) {
    match device {
        None => {
            tray.set_tooltip("No Device");
        }
        Some(device) => {
            let name = device.name().to_string();
            let tooltip = match device.is_connected() {
                true => match device.get_battery_status() {
                    Some(BatteryLevel::Charging) => format!("{name}\nBattery: Charging"),
                    Some(BatteryLevel::Level(level)) => format!("{name}\nBattery: {level}%"),
                    _ => format!("{name}\nConnected")
                },
                false => format!("{name}\nDisconnected")
            };
            tray.set_tooltip(&tooltip);
        }
    }
    tracing::trace!("Updated tooltip");
}
*/