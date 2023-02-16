use egui::Ui;
use crate::audio::AudioDevice;
use crate::config::OutputSwitch;

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