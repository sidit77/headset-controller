#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod framework;
mod util;
mod config;
mod debouncer;
mod devices;
mod ui;

use color_eyre::Result;
use std::ops::{DerefMut, Not};
use std::sync::Arc;
use async_executor::LocalExecutor;
use either::Either;
use flume::{Receiver, Sender};
use futures_lite::{StreamExt, FutureExt};
use tracing::level_filters::LevelFilter;
use tracing_error::ErrorLayer;
use tracing_subscriber::filter::{Targets};
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIconBuilder};
use parking_lot::Mutex;
use tracing::instrument;
use crate::config::{CLOSE_IMMEDIATELY, Config, EqualizerConfig, HeadsetConfig, PRINT_UDEV_RULES, START_QUIET};
use crate::debouncer::{Action, ActionProxy, ActionReceiver, ActionSender};
use crate::devices::{BoxedDevice, Device, DeviceList, generate_udev_rules};
use crate::framework::{AsyncGuiWindow, Gui};
use crate::util::{select, WorkerThread};

struct ShowWindow;

struct SharedState {
    config: Config,
    device: Option<BoxedDevice>,
    device_list: DeviceList
}

fn main() -> Result<()> {
    if *PRINT_UDEV_RULES { return Ok(println!("{}", generate_udev_rules()?)); }
    color_eyre::install()?;
    //let logfile = Mutex::new(log_file());
    tracing_subscriber::registry()
        .with(ErrorLayer::default())
        .with(Targets::new()
            .with_target("async_io", LevelFilter::DEBUG)
            .with_target("polling", LevelFilter::DEBUG)
            .with_default(LevelFilter::TRACE))
        .with(layer().without_time())
        //.with(layer().with_ansi(false).with_writer(logfile))
        .init();

    let span = tracing::info_span!("init").entered();

    let shared_state = Arc::new(Mutex::new(SharedState {
        config: Config::load()?,
        device: None,
        device_list: DeviceList::empty()
    }));

    span.exit();

    let (window_sender, window_receiver) = flume::unbounded::<ShowWindow>();
    if START_QUIET.not() {
        let _ = window_sender.send(ShowWindow);
    }

    let (event_sender, event_receiver) = debouncer::debouncer();

    let executor = LocalExecutor::new();
    let worker = executor.spawn({
        let shared_state = shared_state.clone();
        WorkerThread::spawn(move || {
            let result = async_io::block_on(worker_thread(shared_state, event_receiver));
            tracing::trace!("async-io helper thread is shutting down");
            result
        })
    });
    let window = executor.spawn(manage_window(shared_state.clone(), window_receiver, event_sender));
    let tray = executor.spawn(manage_tray(window_sender));

    framework::block_on(executor.run(async move {
        window.or(tray).or(worker).await
    }))
}

#[instrument(skip_all)]
async fn worker_thread(shared_state: Arc<Mutex<SharedState>>, mut event_receiver: ActionReceiver) -> Result<()> {
    let executor = LocalExecutor::new();

    let (update_sender, update_receiver) = flume::unbounded();

    let _event_printer = executor.spawn(async move {
        update_receiver
            .into_stream()
            .for_each(|event| println!("DeviceUpdate: {:?}", event))
            .await;
        //redraw?
        //match event {
        //    DeviceUpdate::ConnectionChanged | DeviceUpdate::BatteryLevel => action_sender.submit(Action::UpdateDeviceStatus),
        //    DeviceUpdate::DeviceError(err) => tracing::error!("The device return an error: {}", err),
        //    DeviceUpdate::ChatMixChanged => {}
        //}
    });

    event_receiver.submit_all([Action::UpdateSystemAudio, Action::UpdateTrayTooltip, Action::UpdateTray, Action::RefreshDeviceList, Action::SwitchDevice]);
    executor.run(async {
        let mut last_connected = false;
        let mut last_battery = Default::default();
        while let Some(action) = event_receiver.next().await {
            let _span = tracing::info_span!("debouncer_event", ?action).entered();
            tracing::trace!("Processing event");
            match action {
                Action::UpdateDeviceStatus => {
                    let state = shared_state.lock();
                    let device = &state.device;
                    if let Some(device) = device.as_ref() {
                        let current_connection = device.is_connected();
                        let current_battery = device.get_battery_status();
                        if current_connection != last_connected {
                            //TODO reenable
                            //let msg = build_notification_text(current_connection, &[current_battery, last_battery]);
                            //notification::notify(device.name(), &msg, Duration::from_secs(2))
                            //    .unwrap_or_else(|err| tracing::warn!("Can not create notification: {:?}", err));
                            event_receiver.submit_all([Action::UpdateSystemAudio, Action::UpdateTrayTooltip]);
                            event_receiver.force(Action::UpdateSystemAudio);
                            last_connected = current_connection;
                        }
                        if last_battery != current_battery {
                            event_receiver.submit(Action::UpdateTrayTooltip);
                            last_battery = current_battery;
                        }
                    }
                }
                Action::RefreshDeviceList => {
                    shared_state.lock().device = None;
                    let list = DeviceList::new()
                        .await
                        .unwrap_or_else(|err| {
                            tracing::warn!("Failed to refresh devices: {}", err);
                            DeviceList::empty()
                        });
                    shared_state.lock().device_list = list;
                },
                Action::SwitchDevice => {
                    let (preferred_device, current_device) = {
                        let state = shared_state.lock();
                        let preferred_device = state.config.preferred_device.clone();
                        let current_device = state.device.as_ref().map(|d| d.name().to_string());
                        (preferred_device, current_device)
                    };

                    if preferred_device != current_device {
                        let list = shared_state.lock().device_list.clone();
                        let device = list
                            .find_preferred_device(&preferred_device, &executor, update_sender.clone())
                            .await;
                        shared_state.lock().device = device;
                        event_receiver.submit_all([Action::UpdateTray, Action::UpdateTrayTooltip]);
                    } else {
                        tracing::debug!("Preferred device is already active")
                    }
                }
                Action::UpdateSystemAudio => {
                    //TODO REIMPLEMENT
                    //let device = device.lock();
                    //if let Some(device) = device.as_ref() {
                    //    let mut config = config.lock();
                    //    let headset = config.get_headset(device.name());
                    //    audio_system.apply(&headset.os_audio, device.is_connected())
                    //}
                }
                Action::SaveConfig => {
                    shared_state
                        .lock()
                        .config
                        .save()
                        .unwrap_or_else(|err| tracing::warn!("Could not save config: {:?}", err));
                }
                Action::UpdateTray => {
                    //TODO REIMPLEMENT
                    //let mut config = config.lock();
                    //update_tray(&mut tray, &mut config, device.lock().as_ref().map(|d| d.name()))
                },
                Action::UpdateTrayTooltip => {
                    //TODO REIMPLEMENT
                    //update_tray_tooltip(&mut tray, &device.lock())
                },
                action => {
                    let mut state = shared_state.lock();
                    let state = state.deref_mut();
                    if let Some(device) = state.device.as_ref() {
                        let headset = state.config.get_headset(device.name());
                        apply_config_to_device(action, device.as_ref(), headset)
                    }
                }
            }
        }

        //event_receiver
        //    .for_each(|event| println!("Got event: {:?}", event)).await;
        Ok(())
    }).await
}

#[instrument(skip_all)]
async fn manage_window(shared_state: Arc<Mutex<SharedState>>, receiver: Receiver<ShowWindow>, event_sender: ActionProxy) -> Result<()> {
    receiver
        .stream()
        .then(|_| async {
            let mut event_sender = event_sender.clone();
            let shared_state = shared_state.clone();
            let window = AsyncGuiWindow::new(Gui::new(move |ctx: &egui::Context | {
                let mut state = shared_state.lock();
                let state = state.deref_mut();
                match state.device.as_ref() {
                    Some(device) => ui::config_ui(
                        ctx,
                        &mut event_sender,
                        &mut state.config,
                        device.as_ref(),
                        &state.device_list
                    ),
                    None => ui::no_device_ui(ctx, &mut event_sender)
                }
            })).await;
            while let Either::Right(Ok(ShowWindow)) = select(window.close_requested(), receiver.recv_async()).await {
                window.focus();
            }
            Ok(())
        })
        .take(CLOSE_IMMEDIATELY.then_some(1).unwrap_or(usize::MAX))
        .try_collect()
        .await
}

#[instrument(skip_all)]
async fn manage_tray(window_sender: Sender<ShowWindow>) -> Result<()> {
    let menu_events: Receiver<MenuEvent> = {
        let (sender, receiver) = flume::unbounded();
        MenuEvent::set_event_handler(Some(move |event| sender.send(event).unwrap()));
        receiver
    };

    let _tray_icon = TrayIconBuilder::new()
        .with_icon(Icon::from_rgba(vec![255,255,255,255], 1, 1).unwrap())
        .with_menu(Box::new(Menu::with_items(&[
            &MenuItem::with_id("open", "Open", true, None),
            &MenuItem::with_id("quit", "Quit", true, None)
        ]).unwrap()))
        .build()?;

    while let Ok(event) = menu_events.recv_async().await {
        match event.id {
            id if id == "open" => {
                let _ = window_sender.send_async(ShowWindow).await;
            },
            id if id == "quit" => {
                break;
            }
            _ => {}
        }
    }
    Ok(())
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
            Action::UpdateAutoBluetooth => {
                if let Some(bluetooth_config) = device.get_bluetooth_config() {
                    let _span = tracing::info_span!("bluetooth").entered();
                    bluetooth_config.set_auto_enabled(headset.auto_enable_bluetooth);
                }
            }
            Action::UpdateBluetoothCall => {
                if let Some(bluetooth_config) = device.get_bluetooth_config() {
                    let _span = tracing::info_span!("bluetooth").entered();
                    bluetooth_config.set_call_action(headset.bluetooth_call);
                }
            }
            _ => tracing::warn!("{:?} is not related to the device", action)
        }
    }
}


/*

fn main() -> Result<()> {
    if *PRINT_UDEV_RULES { return Ok(println!("{}", generate_udev_rules()?)); }
    color_eyre::install()?;
    //let logfile = Mutex::new(log_file());
    tracing_subscriber::registry()
        .with(ErrorLayer::default())
        .with(Targets::new().with_default(LevelFilter::TRACE))
        .with(layer().without_time())
        //.with(layer().with_ansi(false).with_writer(logfile))
        .init();


    let span = tracing::info_span!("init").entered();

    let config = Arc::new(Mutex::new(Config::load()?));

    let mut event_loop = EventLoop::new();

    let mut audio_system = AudioSystem::new();

    let device_manager = Arc::new(Mutex::new(DeviceList::empty()));
    let device: Arc<Mutex<Option<BoxedDevice>>> = Arc::new(Mutex::new(None));

    //let tray = AppTray::new(&event_loop);

    let mut window: Option<EguiWindow> = START_QUIET.not().then(|| EguiWindow::new(&event_loop));

    let (mut action_sender, action_receiver) = debouncer();
    let mut debouncer = Debouncer::new();

    action_sender.submit_all([Action::UpdateSystemAudio, Action::UpdateTrayTooltip, Action::UpdateTray]);

    span.exit();

    let runtime = {
        let device_manager = device_manager.clone();
        let device = device.clone();
        let config = config.clone();
        std::thread::spawn(move || {
            Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to start runtime")
                .block_on(action_handler(action_receiver, device_manager, device, config))
        })
    };

    event_loop.run_return(move |event, event_loop, control_flow| {
        if window
            .as_mut()
            .map(|w| {
                w.handle_events(&event, |egui_ctx| {
                    let device = device.lock();
                    let mut config = config.lock();
                    match device.as_ref() {
                        Some(device) => ui::config_ui(
                            egui_ctx,
                            &mut action_sender,
                            &mut config,
                            device.as_ref(),
                            device_manager.as_ref(),
                            &mut audio_system
                        ),
                        None => ui::no_device_ui(egui_ctx, &mut action_sender)
                    }
                })
            })
            .unwrap_or(false)
        {
            action_sender.force(Action::SaveConfig);
            window.take();
            if *CLOSE_IMMEDIATELY {
                *control_flow = ControlFlow::Exit;
            }
        }

        match event {
            /*
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
                        let device = device.lock();
                        if let Some(device) = device.as_ref() {
                            let mut config = config.lock();
                            let headset = config.get_headset(device.name());
                            if id as u32 != headset.selected_profile_index {
                                let len = headset.profiles.len();
                                if id < len {
                                    headset.selected_profile_index = id as u32;
                                    submit_profile_change(&action_sender);
                                    action_sender.submit_all([Action::SaveConfig, Action::UpdateTray]);
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
             */
            _ => ()
        }
        if !matches!(*control_flow, ControlFlow::ExitWithCode(_)) {
            let next_update = window.as_ref().and_then(|w| w.next_repaint());
            *control_flow = match next_update {
                Some(next_update) => match next_update <= Instant::now() {
                    true => ControlFlow::Poll,
                    false => ControlFlow::WaitUntil(next_update)
                },
                None => ControlFlow::Wait
            };
        }
    });

    runtime.join().unwrap();
    Ok(())
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
*/