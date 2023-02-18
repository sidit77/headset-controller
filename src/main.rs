#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod renderer;
mod devices;
mod util;
mod audio;
mod config;
mod ui;
mod notification;

use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::Result;
use egui::panel::Side;
use egui::{Align, Layout, RichText, TextStyle, Visuals, Widget};
use glow::Context;
use log::LevelFilter;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget};
use tao::menu::{ContextMenu, MenuItemAttributes};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::system_tray::SystemTrayBuilder;
use tao::window::Icon;
use crate::audio::AudioManager;
use crate::config::{Config, EqualizerConfig, OutputSwitch, Profile};
use crate::devices::{BatteryLevel, Device};
use crate::renderer::{create_display, GlutinWindowContext};
use crate::renderer::egui_glow_tao::EguiGlow;
use crate::ui::{audio_output_switch_selector};
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

    let icon = Icon::from_rgba(vec![0xff; 32 * 32 * 4], 32, 32).unwrap();
    let mut tray_menu = ContextMenu::new();
    let open_item = tray_menu.add_item(MenuItemAttributes::new("Open"));
    let quit_item = tray_menu.add_item(MenuItemAttributes::new("Quit"));
    let mut tray = SystemTrayBuilder::new(icon, Some(tray_menu))
        .with_tooltip("Not Connected")
        .build(&event_loop)
        .expect("Can not build system tray");


    let mut window: Option<EguiWindow> = Some(EguiWindow::new(&event_loop));

    let mut delete_buffer: Vec<usize> = Vec::new();
    let mut next_device_poll = Instant::now();
    let mut last_config_edit: Option<Instant> = None;
    event_loop.run_return(move |event, event_loop, control_flow| {
        if let Some(update) = last_config_edit {
            if update.elapsed() > Duration::from_secs(10) {
                config.save().log_ok("Could not save config");
                last_config_edit = None;
            }
        }
        if next_device_poll <= Instant::now() {
            let (last_connected, last_battery) = (device.is_connected(), device.get_battery_status());
            next_device_poll = Instant::now() + device.poll().unwrap();

            if last_connected != device.is_connected() {
                notification::notify(&device.get_info().name, match device.is_connected() {
                    true => "Connected",
                    false => "Disconnected"
                }, Duration::from_secs(2)).log_ok("Can not create notification");
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
        let next_update = window
            .as_ref()
            .and_then(|w|w.next_repaint)
            .map(|t| t.min(next_device_poll))
            .unwrap_or(next_device_poll);
        *control_flow = match next_update <= Instant::now() {
            true => ControlFlow::Poll,
            false => ControlFlow::WaitUntil(next_update)
        };
        if window.as_mut().map(|w| w.handle_events(&event, |egui_ctx| {
            let mut dirty = false;
            let headset = config.get_headset(&device.get_info().name);
            egui::SidePanel::new(Side::Left, "Profiles")
                .resizable(true)
                .width_range(175.0..=400.0)
                .show(egui_ctx, |ui| {
                    ui.style_mut().text_styles.get_mut(&TextStyle::Body).unwrap().size = 14.0;
                    ui.label(RichText::from(&device.get_info().manufacturer)
                        .heading()
                        .size(30.0));
                    ui.label(RichText::from(&device.get_info().product)
                        .heading()
                        .size(20.0));
                    ui.separator();
                    if device.is_connected() {
                        if let Some(battery) = device.get_battery_status() {
                            ui.label(format!("Battery: {}", battery));
                        }
                        ui.add_space(10.0);
                        if let Some(mix) = device.get_chat_mix() {
                            ui.label("Chat Mix:");
                            egui::ProgressBar::new(mix.chat as f32 / 100.0)
                                .text("Chat")
                                .ui(ui);
                            egui::ProgressBar::new(mix.game as f32 / 100.0)
                                .text("Game")
                                .ui(ui);
                        }
                    } else {
                        ui.label("Not Connected");
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.heading("Profiles");
                        let resp = ui.with_layout(Layout::right_to_left(Align::Center), |ui|
                            ui.selectable_label(false, RichText::from("+").heading())).inner;
                        if resp.clicked() {
                            headset.profiles.push(Profile::new(String::from("New Profile")));
                            dirty |= true;
                        }
                    });
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            let old_profile_index = headset.selected_profile_index;
                            delete_buffer.clear();
                            let profile_count = headset.profiles.len();
                            for (i, profile) in headset.profiles.iter_mut().enumerate() {
                                let resp = ui.with_layout(Layout::default().with_cross_justify(true), |ui|
                                    ui.selectable_label(i as u32 == headset.selected_profile_index, &profile.name)).inner;
                                let resp = resp.context_menu(|ui| {
                                    ui.text_edit_singleline(&mut profile.name);
                                    ui.add_space(4.0);
                                    if ui.add_enabled(profile_count > 1, egui::Button::new("Delete")).clicked() {
                                        delete_buffer.push(i);
                                        ui.close_menu();
                                    }
                                });
                                if resp.clicked() {
                                    headset.selected_profile_index = i as u32;
                                }
                            }
                            for i in delete_buffer.iter().rev() {
                                headset.profiles.remove(*i);
                                dirty |= true;
                            }
                            headset.selected_profile_index -= delete_buffer
                                .iter()
                                .filter(|i| **i as u32 <= headset.selected_profile_index)
                                .count()
                                .min(headset.selected_profile_index as usize) as u32;
                            if headset.selected_profile_index != old_profile_index {
                                apply_profile(headset.selected_profile(), device.as_ref());
                                dirty |= true;
                            }
                        });
                });
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.style_mut().text_styles.get_mut(&TextStyle::Heading).unwrap().size = 25.0;
                ui.style_mut().text_styles.get_mut(&TextStyle::Body).unwrap().size = 14.0;
                ui.style_mut().text_styles.get_mut(&TextStyle::Button).unwrap().size = 14.0;
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.heading("Profile");
                        {
                            let profile = headset.selected_profile();
                            if let Some(equalizer) = device.get_equalizer() {
                                if ui::equalizer(ui, &mut profile.equalizer, equalizer) {
                                    let levels = match &profile.equalizer {
                                        EqualizerConfig::Preset(i) => equalizer.presets()[*i as usize].1,
                                        EqualizerConfig::Custom(levels) => &levels
                                    };
                                    equalizer.set_levels(&levels)
                                        .log_ok("Could not set equalizer");
                                    dirty |= true;
                                }
                                ui.add_space(10.0);
                            }
                            if let Some(side_tone) = device.get_side_tone() {
                                let resp = egui::Slider::new(&mut profile.side_tone, 0..=(side_tone.levels() - 1))
                                    .text("Side Tone Level")
                                    .ui(ui)
                                    .on_hover_text("This setting controls how much of your voice is played back over the headset when you speak.\nSet to 0 to turn off.");
                                if resp.changed() {
                                    side_tone.set_level(profile.side_tone)
                                        .log_ok("Could not set sidetone");
                                    dirty |= true;
                                }
                                ui.add_space(10.0);
                            }
                            if let Some(mic_volume) = device.get_mic_volume() {
                                let resp = egui::Slider::new(&mut profile.microphone_volume, 0..=(mic_volume.levels() - 1))
                                    .text("Microphone Level")
                                    .ui(ui);
                                if resp.changed() {
                                    mic_volume.set_level(profile.microphone_volume)
                                        .log_ok("Could not set mic level");
                                    dirty |= true;
                                }
                                ui.add_space(10.0);
                            }
                            if let Some(volume_limiter) = device.get_volume_limiter() {
                                let resp = egui::Checkbox::new(&mut profile.volume_limiter, "Limit Volume")
                                    .ui(ui);
                                if resp.changed() {
                                    volume_limiter.set_enabled(profile.volume_limiter)
                                        .log_ok("Could not set volume limit");
                                    dirty |= true;
                                }
                                ui.add_space(10.0);
                            }
                        }
                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(10.0);
                        ui.heading("Headset");
                        ui.add_space(7.0);
                        {
                            let switch = &mut headset.switch_output;
                            if audio_output_switch_selector(ui, switch, &audio_devices, || audio_manager.get_default_device()) {
                                dirty |= true;
                                if device.is_connected() {
                                    apply_audio_switch(true, switch, &audio_manager);
                                }
                            }
                            ui.add_space(10.0);
                        }
                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(10.0);
                        ui.heading("Application");

                    });
                //ui.spinner();
            });
            if dirty {
                last_config_edit = Some(Instant::now());
            }
        })).unwrap_or(false) {
            config.save().log_ok("Could not save config");
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
            }
            _ => (),
        }
    });
    Ok(())
}

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
        let (gl_window, gl) = create_display(&event_loop);
        let gl = Arc::new(gl);
        let egui_glow = EguiGlow::new(&event_loop, gl.clone(), None);
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

                let event_response = self.egui_glow.on_event(&event);
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