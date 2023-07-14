#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
/*
mod devices;
mod config;
mod util;

use color_eyre::Result;
use tokio::runtime::Builder;
use tokio::sync::mpsc::unbounded_channel;
use crate::devices::DeviceManager;



fn main() -> Result<()> {
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let (sender, mut receiver) = unbounded_channel();

    runtime.block_on(async {
        let manager = DeviceManager::new().await?;
        let dev = manager
            .supported_devices()
            .first()
            .unwrap();

        let dev = manager.open(dev, sender.clone()).await?;
        println!("{:?}", dev.is_connected());
        println!("{:?}", dev.get_battery_status());
        println!("{:?}", dev.get_chat_mix());
        while let Some(_) = receiver.recv().await {
            println!("{:?}", dev.is_connected());
            println!("{:?}", dev.get_battery_status());
            println!("{:?}", dev.get_chat_mix());
        }
        Ok(())
    })
}
*/

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
use tokio::runtime::Builder;
use tracing::instrument;
use tracing_error::ErrorLayer;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::audio::AudioSystem;
use crate::config::{log_file, Config, EqualizerConfig, HeadsetConfig, CLOSE_IMMEDIATELY, START_QUIET};
use crate::debouncer::{Action, Debouncer};
use crate::devices::{BatteryLevel, BoxedDevice, Device, DeviceManager, DeviceUpdate};
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
    let runtime = Builder::new_multi_thread().enable_time().build()?;

    let span = tracing::info_span!("init").entered();

    let mut config = Config::load()?;

    let mut event_loop = EventLoop::with_user_event();
    let event_loop_proxy = event_loop.create_proxy();

    let mut audio_system = AudioSystem::new();

    let mut device_manager = runtime.block_on(DeviceManager::new())?;
    let mut device = runtime.block_on(async {
        device_manager
            .find_preferred_device(&config.preferred_device, event_loop_proxy.clone())
            .await
    });

    let mut tray = AppTray::new(&event_loop);

    let mut window: Option<EguiWindow> = START_QUIET.not().then(|| EguiWindow::new(&event_loop));

    let mut debouncer = Debouncer::new();
    let mut last_connected = false;
    let mut last_battery = Default::default();
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
                        Action::UpdateDeviceStatus => {
                            if let Some(device) = &device {
                                let current_connection = device.is_connected();
                                let current_battery = device.get_battery_status();
                                if current_connection != last_connected {
                                    let msg = build_notification_text(current_connection, &[current_battery, last_battery]);
                                    notification::notify(device.name(), &msg, Duration::from_secs(2))
                                        .unwrap_or_else(|err| tracing::warn!("Can not create notification: {:?}", err));
                                    debouncer.submit_all([Action::UpdateSystemAudio, Action::UpdateTrayTooltip]);
                                    debouncer.force(Action::UpdateSystemAudio);
                                    last_connected = current_connection;
                                }
                                if last_battery != current_battery {
                                    debouncer.submit(Action::UpdateTrayTooltip);
                                    last_battery = current_battery;
                                }
                            }
                        }
                        Action::RefreshDeviceList => runtime.block_on(async {
                            device_manager
                                .refresh()
                                .await
                                .unwrap_or_else(|err| tracing::warn!("Failed to refresh devices: {}", err))
                        }),
                        Action::SwitchDevice => {
                            if config.preferred_device != device.as_ref().map(|d| d.name().to_string()) {
                                device = runtime.block_on(async {
                                    device_manager
                                        .find_preferred_device(&config.preferred_device, event_loop_proxy.clone())
                                        .await
                                });
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
            }
            Event::UserEvent(event) => match event {
                DeviceUpdate::ConnectionChanged | DeviceUpdate::BatteryLevel => debouncer.submit(Action::UpdateDeviceStatus),
                DeviceUpdate::DeviceError(err) => tracing::error!("The device return an error: {}", err),
                DeviceUpdate::ChatMixChanged => {}
            },
            _ => ()
        }
        if !matches!(*control_flow, ControlFlow::ExitWithCode(_)) {
            let next_window_update = window.as_ref().and_then(|w| w.next_repaint());
            let next_update = [next_window_update, debouncer.next_action()]
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
                    sidetone.set_level(headset.selected_profile().side_tone);
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
                    equalizer.set_levels(&levels);
                }
            }
            Action::UpdateMicrophoneVolume => {
                if let Some(mic_volume) = device.get_mic_volume() {
                    let _span = tracing::info_span!("mic_volume").entered();
                    mic_volume.set_level(headset.selected_profile().microphone_volume);
                }
            }
            Action::UpdateVolumeLimit => {
                if let Some(volume_limiter) = device.get_volume_limiter() {
                    let _span = tracing::info_span!("volume_limiter").entered();
                    volume_limiter.set_enabled(headset.selected_profile().volume_limiter);
                }
            }
            Action::UpdateInactiveTime => {
                if let Some(inactive_time) = device.get_inactive_time() {
                    let _span = tracing::info_span!("inactive time").entered();
                    inactive_time.set_inactive_time(headset.inactive_time);
                }
            }
            Action::UpdateMicrophoneLight => {
                if let Some(mic_light) = device.get_mic_light() {
                    let _span = tracing::info_span!("mic_light").entered();
                    mic_light.set_light_strength(headset.mic_light);
                }
            }
            Action::UpdateBluetoothCall => {
                if let Some(bluetooth_config) = device.get_bluetooth_config() {
                    let _span = tracing::info_span!("bluetooth").entered();
                    bluetooth_config.set_auto_enabled(headset.auto_enable_bluetooth);
                }
            }
            Action::UpdateAutoBluetooth => {
                if let Some(bluetooth_config) = device.get_bluetooth_config() {
                    let _span = tracing::info_span!("bluetooth").entered();
                    bluetooth_config.set_call_action(headset.bluetooth_call);
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

fn build_notification_text(connected: bool, battery_levels: &[Option<BatteryLevel>]) -> String {
    let msg = match connected {
        true => "Connected",
        false => "Disconnected"
    };
    battery_levels
        .iter()
        .filter_map(|b| match b {
            Some(BatteryLevel::Level(l)) => Some(*l),
            _ => None
        })
        .min()
        .map(|level| format!("{} (Battery: {}%)", msg, level))
        .unwrap_or_else(|| msg.to_string())
}
