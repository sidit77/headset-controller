#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod renderer;
mod devices;
mod util;
mod audio;
mod config;
mod ui;
mod notification;
mod debouncer;


use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::Result;
use egui::{Visuals};
use glow::Context;
use log::LevelFilter;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget};
use tao::menu::{ContextMenu, MenuItemAttributes};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::system_tray::SystemTrayBuilder;
use crate::audio::AudioSystem;
use crate::config::{Config, EqualizerConfig, HeadsetConfig};
use crate::debouncer::{Action, Debouncer};
use crate::devices::{BatteryLevel, Device};
use crate::renderer::{create_display, GlutinWindowContext};
use crate::renderer::egui_glow_tao::EguiGlow;
use crate::util::LogResultExt;

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        .format_timestamp(None)
        .parse_default_env()
        .init();

    let mut config = Config::load()?;

    let mut audio_system = AudioSystem::new();

    let mut device = devices::find_device().unwrap();

    let mut event_loop = EventLoop::new();

    let mut tray_menu = ContextMenu::new();
    let open_item = tray_menu.add_item(MenuItemAttributes::new("Open"));
    let quit_item = tray_menu.add_item(MenuItemAttributes::new("Quit"));
    let mut tray = SystemTrayBuilder::new(ui::WINDOW_ICON.clone(), Some(tray_menu))
        .with_tooltip("Not Connected")
        .build(&event_loop)
        .expect("Can not build system tray");

    let mut window: Option<EguiWindow> = match std::env::args().any(|arg| arg.eq("--quiet")) {
        true => None,
        false => Some(EguiWindow::new(&event_loop))
    };

    let mut next_device_poll = Instant::now();
    let mut debouncer = Debouncer::new();
    debouncer.submit(Action::UpdateSystemAudio);
    event_loop.run_return(move |event, event_loop, control_flow| {
        if window.as_mut().map(|w| w.handle_events(&event, |egui_ctx| {
            ui::config_ui(egui_ctx, &mut debouncer, &mut config, device.as_ref(), &mut audio_system);
        })).unwrap_or(false) {
            debouncer.force(Action::SaveConfig);
            window.take();
            if cfg!(debug_assertions) {
                *control_flow = ControlFlow::Exit;
            }
        }

        match event {
            Event::MenuEvent { menu_id, ..} => {
                if menu_id == open_item.clone().id() {
                    audio_system.refresh_devices();
                    match &mut window {
                        None => window = Some(EguiWindow::new(event_loop)),
                        Some(window) => {
                            window.gl_window.window().set_focus();
                        }
                    }
                }
                if menu_id == quit_item.clone().id() {
                    *control_flow = ControlFlow::Exit;
                }
            },
            Event::NewEvents(_) | Event::LoopDestroyed => {
                for action in &mut debouncer {
                    log::trace!("Activated action: {:?}", action);
                    match action {
                        Action::UpdateSystemAudio => {
                            let headset = config.get_headset(&device.get_info().name);
                            audio_system.apply(&headset.os_audio, device.is_connected())
                        },
                        Action::SaveConfig => {
                            config.save()
                                .log_ok("Could not save config");
                        },
                        action => {
                            let headset = config.get_headset(&device.get_info().name);
                            apply_config_to_device(action, device.as_ref(), headset)
                        }
                    }

                }

                if next_device_poll <= Instant::now() {
                    let (last_connected, last_battery) = (device.is_connected(), device.get_battery_status());
                    next_device_poll = Instant::now() + device.poll().unwrap();

                    if last_connected != device.is_connected() {
                        notification::notify(&device.get_info().name, match device.is_connected() {
                            true => "Connected",
                            false => "Disconnected"
                        }, Duration::from_secs(2))
                            .log_ok("Can not create notification");
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
            _ => (),
        }
        if !matches!(*control_flow, ControlFlow::ExitWithCode(_)) {
            let next_window_update = window
                .as_ref()
                .and_then(|w|w.next_repaint);
            let next_update = [Some(next_device_poll), next_window_update, debouncer.next_action()]
                .into_iter()
                .flatten()
                .min()
                .unwrap();
            *control_flow = match next_update <= Instant::now() {
                true => ControlFlow::Poll,
                false => ControlFlow::WaitUntil(next_update)
            };
        }
    });
    Ok(())
}

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

fn apply_config_to_device(action: Action, device: &dyn Device, headset: &mut HeadsetConfig) {
    if device.is_connected() {
        match action {
            Action::UpdateSideTone => if let Some(sidetone) = device.get_side_tone() {
                sidetone.set_level(headset.selected_profile().side_tone)
                    .log_ok("Can not apply side tone");
            }
            Action::UpdateEqualizer => if let Some(equalizer) = device.get_equalizer() {
                let levels = match headset.selected_profile().equalizer.clone() {
                    EqualizerConfig::Preset(i) => equalizer
                        .presets().get(i as usize)
                        .expect("Unknown preset").1
                        .to_vec(),
                    EqualizerConfig::Custom(levels) => levels,
                };
                equalizer.set_levels(&levels)
                    .log_ok("Could not apply equalizer");
            }
            Action::UpdateMicrophoneVolume => if let Some(mic_volume) = device.get_mic_volume() {
                mic_volume.set_level(headset.selected_profile().microphone_volume)
                    .log_ok("Could not apply microphone volume");
            }
            Action::UpdateVolumeLimit => if let Some(volume_limiter) = device.get_volume_limiter() {
                volume_limiter.set_enabled(headset.selected_profile().volume_limiter)
                    .log_ok("Could not apply volume limited");
            }
            Action::UpdateInactiveTime => if let Some(inactive_time) = device.get_inactive_time() {
                inactive_time.set_inactive_time(headset.inactive_time)
                    .log_ok("Could not apply inactive time");
            }
            Action::UpdateMicrophoneLight => if let Some(mic_light) = device.get_mic_light() {
                mic_light.set_light_strength(headset.mic_light)
                    .log_ok("Could not apply microphone light");
            }
            Action::UpdateBluetoothCall => if let Some(bluetooth_config) = device.get_bluetooth_config() {
                bluetooth_config.set_auto_enabled(headset.auto_enable_bluetooth)
                    .log_ok("Could not set bluetooth auto enabled");
            }
            Action::UpdateAutoBluetooth => if let Some(bluetooth_config) = device.get_bluetooth_config() {
                bluetooth_config.set_call_action(headset.bluetooth_call)
                    .log_ok("Could not set call action");
            }
            Action::SaveConfig | Action::UpdateSystemAudio => log::warn!("{:?} is not related to the device", action)
        }
    }
}

/*
fn apply_profile(profile: &Profile, device: &dyn Device) {
    if let Some(equalizer) = device.get_equalizer() {
        let levels = match &profile.equalizer {
            EqualizerConfig::Preset(i) => equalizer.presets()[*i as usize].1,
            EqualizerConfig::Custom(levels) => &levels
        };
        equalizer.set_levels(&levels)
            .log_ok("Could not set equalizer");
    }
    if let Some(side_tone) = device.get_side_tone() {
        side_tone.set_level(profile.side_tone)
            .log_ok("Could not set sidetone");
    }
    if let Some(mic_volume) = device.get_mic_volume() {
        mic_volume.set_level(profile.microphone_volume)
            .log_ok("Could not set mic level");
    }
    if let Some(volume_limiter) = device.get_volume_limiter() {
        volume_limiter.set_enabled(profile.volume_limiter)
            .log_ok("Could not set volume limit");
    }
}
*/


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
            next_repaint: Some(Instant::now()),
        }
    }

    fn redraw(&mut self, gui: impl FnMut(&egui::Context)) {
        let repaint_after = self.egui_glow.run(self.gl_window.window(), gui);
        self.next_repaint = Instant::now().checked_add(repaint_after);
        {
            let clear_color = [0.1, 0.1, 0.1];
            unsafe {
                use glow::HasContext as _;
                self.gl.clear_color(clear_color[0], clear_color[1], clear_color[2], 1.0);
                self.gl.clear(glow::COLOR_BUFFER_BIT);
            }
            self.egui_glow.paint(self.gl_window.window());
            self.gl_window.swap_buffers().unwrap();
        }
    }

    fn handle_events(&mut self, event: &Event<()>, gui: impl FnMut(&egui::Context)) -> bool{
        if self.next_repaint.map(|t| Instant::now().checked_duration_since(t)).is_some() {
            self.gl_window.window().request_redraw();
        }
        match event {
            Event::RedrawEventsCleared if cfg!(windows) => self.redraw(gui),
            Event::RedrawRequested(_) if !cfg!(windows) => self.redraw(gui),
            Event::WindowEvent { event, .. } => {
                match &event {
                    WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                        return true;
                    },
                    WindowEvent::Resized(physical_size) => {
                        self.gl_window.resize(*physical_size);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        self.gl_window.resize(**new_inner_size);
                    },
                    _ => {}
                }

                let event_response = self.egui_glow.on_event(event);
                if event_response.repaint {
                    self.gl_window.window().request_redraw();
                }
            }
            _ => (),
        }
        false
    }
}

impl Drop for EguiWindow {
    fn drop(&mut self) {
        self.egui_glow.destroy();
    }
}