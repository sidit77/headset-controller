use egui::*;

use crate::config::{EqualizerConfig, Profile};
use crate::debouncer::{Action, Debouncer};
use crate::devices::{Device, Equalizer};
use crate::ui::ResponseExt;

pub fn profile_section(ui: &mut Ui, debouncer: &mut Debouncer, auto_update: bool, profile: &mut Profile, device: &dyn Device) {
    if let Some(equalizer) = device.get_equalizer() {
        equalizer_ui(ui, debouncer, auto_update, &mut profile.equalizer, equalizer);
        ui.add_space(10.0);
    }
    if let Some(side_tone) = device.get_side_tone() {
        Slider::new(&mut profile.side_tone, 0..=(side_tone.levels() - 1))
            .text("Side Tone Level")
            .ui(ui)
            .on_hover_text("This setting controls how much of your voice is played back over the headset when you speak.\nSet to 0 to turn off.")
            .submit(debouncer, auto_update, Action::UpdateSideTone);
        ui.add_space(10.0);
    }
    if let Some(mic_volume) = device.get_mic_volume() {
        Slider::new(&mut profile.microphone_volume, 0..=(mic_volume.levels() - 1))
            .text("Microphone Level")
            .ui(ui)
            .submit(debouncer, auto_update, Action::UpdateMicrophoneVolume);
        ui.add_space(10.0);
    }
    if device.get_volume_limiter().is_some() {
        Checkbox::new(&mut profile.volume_limiter, "Limit Volume")
            .ui(ui)
            .submit(debouncer, auto_update, Action::UpdateVolumeLimit);
        ui.add_space(10.0);
    }
}

fn equalizer_ui(ui: &mut Ui, debouncer: &mut Debouncer, auto_update: bool, conf: &mut EqualizerConfig, equalizer: &dyn Equalizer) {
    let range = (equalizer.base_level() - equalizer.variance())..=(equalizer.base_level() + equalizer.variance());
    let mut presets = equalizer
        .presets()
        .iter()
        .map(|(s, _)| s.to_string())
        .collect::<Vec<_>>();
    let custom_index = presets.len();
    presets.push("Custom".to_string());
    let (mut current_index, mut levels) = match conf {
        EqualizerConfig::Preset(i) => (*i as usize, equalizer.presets()[*i as usize].1.to_vec()),
        EqualizerConfig::Custom(levels) => (custom_index, levels.clone())
    };
    let preset = ComboBox::from_label("Equalizer").show_index(ui, &mut current_index, presets.len(), |i| presets[i].clone());
    let mut dirty = preset.changed();
    ui.horizontal(|ui| {
        for i in levels.iter_mut() {
            let resp = Slider::new(i, range.clone()).vertical().ui(ui);
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
