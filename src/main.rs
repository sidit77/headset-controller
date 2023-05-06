#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod debouncer;
mod devices;
mod notification;
mod renderer;
mod tray;
mod ui;
mod util;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use color_eyre::Result;
use egui::Visuals;
use glow::Context;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget};
use tao::platform::run_return::EventLoopExtRunReturn;
use tracing::instrument;
use tracing_error::ErrorLayer;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::audio::AudioSystem;
use crate::config::{log_file, Config, EqualizerConfig, HeadsetConfig};
use crate::debouncer::{Action, Debouncer};
use crate::devices::{BatteryLevel, BoxedDevice, Device, DeviceManager, SupportedDevice};
use crate::renderer::egui_glow_tao::EguiGlow;
use crate::renderer::{create_display, GlutinWindowContext};
use crate::tray::{AppTray, TrayEvent};

fn main() -> Result<()> {
    color_eyre::install()?;
    let logfile = Mutex::new(log_file());
    tracing_subscriber::registry()
        .with(Targets::new().with_default(LevelFilter::TRACE))
        .with(layer().without_time())
        .with(layer().with_ansi(false).with_writer(logfile))
        .with(ErrorLayer::default())
        .init();

    let span = tracing::info_span!("init").entered();

    let mut config = Config::load()?;

    let mut audio_system = AudioSystem::new();

    let device_manager = DeviceManager::new()?;
    let (mut devices, mut device) = get_preferred_device(&device_manager, &config);

    let mut event_loop = EventLoop::new();

    let mut tray = AppTray::new(&event_loop);
    update_tray(&mut tray, &mut config, device.as_ref().map(|d| d.name()));

    let mut window: Option<EguiWindow> = match std::env::args().any(|arg| arg.eq("--quiet")) {
        true => None,
        false => Some(EguiWindow::new(&event_loop))
    };

    let mut next_device_poll = Instant::now();
    let mut debouncer = Debouncer::new();
    debouncer.submit(Action::UpdateSystemAudio);

    span.exit();
    event_loop.run_return(move |event, event_loop, control_flow| {
        if window
            .as_mut()
            .map(|w| {
                w.handle_events(&event, |egui_ctx| match &device {
                    Some(device) => ui::config_ui(egui_ctx, &mut debouncer, &mut config, device.as_ref(), &devices, &mut audio_system),
                    None => ui::no_device_ui(egui_ctx)
                })
            })
            .unwrap_or(false)
        {
            debouncer.force(Action::SaveConfig);
            window.take();
            if cfg!(debug_assertions) {
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
                                window.gl_window.window().set_focus();
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
                        Action::SwitchDevice => {
                            if config.preferred_device != device.as_ref().map(|d| d.name().to_string()) {
                                let (list, dev) = get_preferred_device(&device_manager, &config);
                                device = dev;
                                devices = list;
                                submit_full_change(&mut debouncer);
                                debouncer.submit(Action::UpdateTray);
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
                            tray.set_tooltip(&match device.get_battery_status() {
                                Some(BatteryLevel::Charging) => format!("{}\nBattery: Charging", device.get_info()),
                                Some(BatteryLevel::Level(level)) => format!("{}\nBattery: {}%", device.get_info(), level),
                                _ => format!("{}\nDisconnected", device.get_info())
                            });
                        }
                    }
                }
            }
            _ => ()
        }
        if !matches!(*control_flow, ControlFlow::ExitWithCode(_)) {
            let next_window_update = window.as_ref().and_then(|w| w.next_repaint);
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
        None => tray.build_menu(0, |_| ("", false)),
        Some(device_name) => {
            let headset = config.get_headset(device_name);
            let selected = headset.selected_profile_index as usize;
            let profiles = &headset.profiles;
            tray.build_menu(profiles.len(), |id| (profiles[id].name.as_str(), id == selected));
        }
    }
}

#[instrument(skip_all)]
fn get_preferred_device(device_manager: &DeviceManager, config: &Config) -> (Vec<Box<dyn SupportedDevice>>, Option<BoxedDevice>) {
    let devices = device_manager.supported_devices();
    for dev in &devices {
        tracing::info!("Found: {}", dev.name());
    }
    let device = config
        .preferred_device
        .iter()
        .flat_map(|pref| devices.iter().filter(move |dev| dev.name() == pref))
        .chain(devices.iter())
        .filter_map(|dev| {
            device_manager
                .open(dev.as_ref())
                .map_err(|err| tracing::error!("Failed to open device: {:?}", err))
                .ok()
        })
        .next();
    (devices, device)
}

struct EguiWindow {
    gl_window: GlutinWindowContext,
    gl: Arc<Context>,
    egui_glow: EguiGlow,
    next_repaint: Option<Instant>
}

impl EguiWindow {
    fn new(event_loop: &EventLoopWindowTarget<()>) -> Self {
        let (gl_window, gl) = create_display(event_loop);
        let gl = Arc::new(gl);
        let egui_glow = EguiGlow::new(event_loop, gl.clone(), None);
        egui_glow.egui_ctx.set_visuals(Visuals::light());
        gl_window.window().set_visible(true);

        Self {
            gl_window,
            gl,
            egui_glow,
            next_repaint: Some(Instant::now())
        }
    }

    fn redraw(&mut self, gui: impl FnMut(&egui::Context)) {
        let repaint_after = self.egui_glow.run(self.gl_window.window(), gui);
        self.next_repaint = Instant::now().checked_add(repaint_after);
        {
            let clear_color = [0.1, 0.1, 0.1];
            unsafe {
                use glow::HasContext as _;
                self.gl
                    .clear_color(clear_color[0], clear_color[1], clear_color[2], 1.0);
                self.gl.clear(glow::COLOR_BUFFER_BIT);
            }
            self.egui_glow.paint(self.gl_window.window());
            self.gl_window
                .swap_buffers()
                .expect("Failed to swap buffers");
        }
    }

    fn handle_events(&mut self, event: &Event<()>, gui: impl FnMut(&egui::Context)) -> bool {
        if self
            .next_repaint
            .map(|t| Instant::now().checked_duration_since(t))
            .is_some()
        {
            self.gl_window.window().request_redraw();
        }
        match event {
            Event::RedrawEventsCleared if cfg!(windows) => self.redraw(gui),
            Event::RedrawRequested(_) if !cfg!(windows) => self.redraw(gui),
            Event::WindowEvent { event, .. } => {
                match &event {
                    WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                        return true;
                    }
                    WindowEvent::Resized(physical_size) => {
                        self.gl_window.resize(*physical_size);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        self.gl_window.resize(**new_inner_size);
                    }
                    _ => {}
                }

                let event_response = self.egui_glow.on_event(event);
                if event_response.repaint {
                    self.gl_window.window().request_redraw();
                }
            }
            _ => ()
        }
        false
    }
}

impl Drop for EguiWindow {
    fn drop(&mut self) {
        self.egui_glow.destroy();
    }
}
