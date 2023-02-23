use egui::{Align, Button, Context, Layout, RichText, SidePanel, Slider, TextStyle, Ui, Widget};
use egui::panel::Side;
use once_cell::sync::Lazy;
use tao::window::Icon;
use crate::audio::{AudioDevice, AudioManager};
use crate::config::{Config, EqualizerConfig, OutputSwitch, Profile};
use crate::debouncer::{Action, Debouncer};
use crate::devices::{Device, Equalizer};
use crate::{apply_audio_switch, submit_profile_change};

#[cfg(windows)]
pub static WINDOW_ICON: Lazy<Icon> = Lazy::new(|| {
    use tao::platform::windows::IconExtWindows;
    Icon::from_resource(32512, None).unwrap()
});

#[cfg(not(windows))]
pub static WINDOW_ICON: Lazy<Icon> = Lazy::new(|| {
    let mut decoder = png::Decoder::new(include_bytes!("../../resources/icon.png").as_slice());
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    Icon::from_rgba(buf, info.width, info.height).unwrap()
});

pub fn config_ui(ctx: &Context, debouncer: &mut Debouncer, config: &mut Config, device: &dyn Device, audio_devices: &[AudioDevice], audio_manager: &AudioManager) {
    SidePanel::new(Side::Left, "Profiles")
        .resizable(true)
        .width_range(175.0..=400.0)
        .show(ctx, |ui| {
            let headset = config.get_headset(&device.get_info().name);
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
                    debouncer.submit(Action::SaveConfig);
                }
            });
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    let old_profile_index = headset.selected_profile_index;
                    let mut deleted = None;
                    let profile_count = headset.profiles.len();
                    for (i, profile) in headset.profiles.iter_mut().enumerate() {
                        let resp = ui.with_layout(Layout::default().with_cross_justify(true), |ui|
                            ui.selectable_label(i as u32 == headset.selected_profile_index, &profile.name)).inner;
                        let resp = resp.context_menu(|ui| {
                            ui.text_edit_singleline(&mut profile.name);
                            ui.add_space(4.0);
                            if ui.add_enabled(profile_count > 1, Button::new("Delete")).clicked() {
                                deleted = Some(i);
                                ui.close_menu();
                            }
                        });
                        if resp.clicked() {
                            headset.selected_profile_index = i as u32;
                        }
                    }
                    if let Some(i) = deleted {
                        headset.profiles.remove(i);
                        debouncer.submit(Action::SaveConfig);
                        if i as u32 <= headset.selected_profile_index && headset.selected_profile_index > 0{
                            headset.selected_profile_index -= 1;
                        }
                    }
                    if headset.selected_profile_index != old_profile_index {
                        submit_profile_change(debouncer);
                        debouncer.submit(Action::SaveConfig);
                    }
                });
        });
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.style_mut().text_styles.get_mut(&TextStyle::Heading).unwrap().size = 25.0;
        ui.style_mut().text_styles.get_mut(&TextStyle::Body).unwrap().size = 14.0;
        ui.style_mut().text_styles.get_mut(&TextStyle::Button).unwrap().size = 14.0;
        egui::ScrollArea::both()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let auto_update = config.auto_apply_changes;
                let headset = config.get_headset(&device.get_info().name);
                ui.heading("Profile");
                {
                    let profile = headset.selected_profile();
                    if let Some(equalizer) = device.get_equalizer() {
                        equalizer_ui(ui, debouncer, auto_update, &mut profile.equalizer, equalizer);
                        ui.add_space(10.0);
                    }
                    if let Some(side_tone) = device.get_side_tone() {
                        let resp = Slider::new(&mut profile.side_tone, 0..=(side_tone.levels() - 1))
                            .text("Side Tone Level")
                            .ui(ui)
                            .on_hover_text("This setting controls how much of your voice is played back over the headset when you speak.\nSet to 0 to turn off.");
                        if resp.changed() {
                            debouncer.submit(Action::SaveConfig);
                            if auto_update {
                                debouncer.submit(Action::UpdateSideTone);
                            }
                        }
                        if resp.drag_released() {
                            debouncer.force(Action::UpdateSideTone);
                        }
                        ui.add_space(10.0);
                    }
                    if let Some(mic_volume) = device.get_mic_volume() {
                        let resp = egui::Slider::new(&mut profile.microphone_volume, 0..=(mic_volume.levels() - 1))
                            .text("Microphone Level")
                            .ui(ui);
                        if resp.changed() {
                            debouncer.submit(Action::SaveConfig);
                            if auto_update {
                                debouncer.submit(Action::UpdateMicrophoneVolume);
                            }
                        }
                        if resp.drag_released() {
                            debouncer.force(Action::UpdateMicrophoneVolume);
                        }
                        ui.add_space(10.0);
                    }
                    if device.get_volume_limiter().is_some() {
                        let resp = egui::Checkbox::new(&mut profile.volume_limiter, "Limit Volume")
                            .ui(ui);
                        if resp.changed() {
                            debouncer.submit(Action::SaveConfig);
                            if auto_update {
                                debouncer.submit(Action::UpdateVolumeLimit);
                            }
                            debouncer.force(Action::UpdateVolumeLimit);
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
                    if audio_output_switch_selector(ui, switch, audio_devices, || audio_manager.get_default_device()) {
                        debouncer.submit(Action::SaveConfig);
                        if device.is_connected() {
                            apply_audio_switch(true, switch, audio_manager);
                        }
                    }
                    ui.add_space(10.0);
                }
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                ui.heading("Application");
                ui.add_space(7.0);
                if ui.checkbox(&mut config.auto_apply_changes, "Auto Apply Changes").changed() {
                    debouncer.submit(Action::SaveConfig);
                }
                ui.with_layout(Layout::default().with_main_align(Align::Center), |ui| {
                    if ui.add_sized([200.0, 20.0], Button::new("Apply Now")).clicked(){
                        submit_profile_change(debouncer);
                    }
                });
                ui.add_space(10.0);
                ui.checkbox(&mut false, "Run On Startup");
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                ui.heading("Information");
                ui.add_space(7.0);
                ui.label(concat!("Version: ", env!("CARGO_PKG_VERSION")));
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    ui.label("Repository: ");
                    ui.hyperlink("https://github.com/sidit77/headset-controller");
                });
                ui.add_space(7.0);
                ui.label(format!("Config Location: {}", Config::path().display()));
                ui.add_space(12.0);
            });
        //ui.spinner();
    });
}

pub fn equalizer_ui(ui: &mut Ui, debouncer: &mut Debouncer, auto_update: bool, conf: &mut EqualizerConfig, equalizer: &dyn Equalizer) {
    let range = (equalizer.base_level() - equalizer.variance())..=(equalizer.base_level() + equalizer.variance());
    let mut presets = equalizer.presets().iter().map(|(s, _)| s.to_string()).collect::<Vec<_>>();
    let custom_index = presets.len();
    presets.push("Custom".to_string());
    let (mut current_index, mut levels) = match conf {
        EqualizerConfig::Preset(i) => (*i as usize, equalizer.presets()[*i as usize].1.to_vec()),
        EqualizerConfig::Custom(levels) => (custom_index, levels.clone())
    };
    let preset = egui::ComboBox::from_label("Equalizer")
        .show_index(ui, &mut current_index, presets.len(), |i| presets[i].clone());
    let mut dirty = preset.changed();
    ui.horizontal(|ui| {
        for i in levels.iter_mut() {
            let resp = egui::Slider::new(i, range.clone())
                .vertical()
                .ui(ui);
            if resp.changed() {
                dirty |= true;
                current_index = custom_index;
            }
            if resp.drag_released() {
                debouncer.force(Action::UpdateEqualizer);
            }
        }
    });
    if dirty {
        *conf = if current_index == custom_index {
            EqualizerConfig::Custom(levels)
        } else {
            EqualizerConfig::Preset(current_index as u32)
        };
        debouncer.submit(Action::SaveConfig);
        if auto_update {
            debouncer.submit(Action::UpdateEqualizer);
        }
    }
    if preset.changed() {
        debouncer.force(Action::UpdateEqualizer);
    }
}

pub fn audio_output_switch_selector(ui: &mut Ui, switch: &mut OutputSwitch,
                                    audio_devices: &[AudioDevice],
                                    default_device: impl FnOnce() -> Option<AudioDevice>) -> bool {
    let mut dirty = false;
    let mut enabled = *switch != OutputSwitch::Disabled;
    if ui.checkbox(&mut enabled, "Automatic Output Switching").changed() {
        if enabled {
            let default_audio = default_device()
                .or_else(||audio_devices.first().cloned())
                .map(|d| d.name().to_string())
                .expect("No device");
            *switch = OutputSwitch::Enabled {
                on_connect: default_audio.clone(),
                on_disconnect: default_audio,
            };
        } else {
            *switch = OutputSwitch::Disabled;
        }
        dirty |= true;
    }
    if let OutputSwitch::Enabled { on_connect, on_disconnect } = switch {
        dirty |= audio_device_selector(ui, "On Connect", on_connect, audio_devices);
        dirty |= audio_device_selector(ui, "On Disconnect", on_disconnect, audio_devices);
    }
    dirty
}

pub fn audio_device_selector(ui: &mut Ui, label: &str, selected: &mut String, audio_devices: &[AudioDevice]) -> bool {
    let (mut index, mut changed) = audio_devices
        .iter()
        .position(|d| d.name() == selected)
        .map(|i| (i, false))
        .unwrap_or((0, true));
    changed |= egui::ComboBox::from_label(label)
        .width(300.0)
        .show_index(ui, &mut index, audio_devices.len(), |i| audio_devices[i].name().to_string())
        .changed();
    if changed {
        *selected = audio_devices[index].name().to_string();
    }
    changed
}