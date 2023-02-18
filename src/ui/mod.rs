use egui::{Ui, Widget};
use crate::audio::AudioDevice;
use crate::config::{EqualizerConfig, OutputSwitch};
use crate::devices::Equalizer;

pub fn equalizer(ui: &mut Ui, conf: &mut EqualizerConfig, equalizer: &dyn Equalizer) -> bool {
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
        }
    });
    if dirty {
        *conf = if current_index == custom_index {
            EqualizerConfig::Custom(levels)
        } else {
            EqualizerConfig::Preset(current_index as u32)
        };
    }
    dirty
}

pub fn audio_output_switch_selector(ui: &mut Ui, switch: &mut OutputSwitch,
                                    audio_devices: &[AudioDevice],
                                    default_device: impl FnOnce() -> Option<AudioDevice>) -> bool {
    let mut dirty = false;
    let mut enabled = *switch != OutputSwitch::Disabled;
    if ui.checkbox(&mut enabled, "Automatic Output Switching").changed() {
        if enabled {
            let default_audio = default_device()
                .or(audio_devices.first().cloned())
                .map(|d| d.name().to_string())
                .expect("No device");
            *switch = OutputSwitch::Enabled {
                on_connect: default_audio.clone(),
                on_disconnect: default_audio.clone(),
            };
        } else {
            *switch = OutputSwitch::Disabled;
        }
        dirty |= true;
    }
    if let OutputSwitch::Enabled { on_connect, on_disconnect } = switch {
        dirty |= audio_device_selector(ui, "On Connect", on_connect, &audio_devices);
        dirty |= audio_device_selector(ui, "On Disconnect", on_disconnect, &audio_devices);
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