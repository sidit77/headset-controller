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
use crate::audio::AudioManager;
use crate::config::{Config, OutputSwitch};
use crate::debouncer::{Action, Debouncer};
use crate::devices::{BatteryLevel};
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

    let audio_manager = AudioManager::new()?;
    let audio_devices = audio_manager
        .devices()
        .collect::<Vec<_>>();

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
    event_loop.run_return(move |event, event_loop, control_flow| {
        if window.as_mut().map(|w| w.handle_events(&event, |egui_ctx| {
            ui::config_ui(egui_ctx, &mut debouncer, &mut config, device.as_ref(), &audio_devices, &audio_manager);
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
                    //match action {
                    //    Action::SaveConfig => log::info!("Saved config"),//config.save().log_ok("Could not save config"),
                    //    Action::UpdateSideTone => log::info!("changed sidetone"),
                    //    Action::UpdateEqualizer => log::info!("changed sidetone"),
                    //    Action::UpdateMicrophoneVolume => {}
                    //    Action::UpdateVolumeLimit => {}
                    //};
                    log::info!("{:?}", action);
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
                        let switch = &config.get_headset(&device.get_info().name).switch_output;
                        apply_audio_switch(device.is_connected(), switch, &audio_manager);
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
fn apply_audio_switch(connected: bool, switch: &OutputSwitch, audio_manager: &AudioManager) {
    if let OutputSwitch::Enabled { on_connect, on_disconnect} = switch {
        let target = match connected {
            true => on_connect,
            false => on_disconnect
        };
        if let Some(device) = audio_manager
            .devices()
            .find(|dev| dev.name() == target) {
            match audio_manager
                .get_default_device()
                .map(|dev|dev == device)
                .unwrap_or(false) {
                true => log::info!("Device \"{}\" is already active", device.name()),
                false => {
                    audio_manager.set_default_device(&device)
                        .log_ok("Could not change default audio device");
                }
            }
        }
    }
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