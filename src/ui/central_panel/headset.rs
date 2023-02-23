use egui::*;
use crate::apply_audio_switch;
use crate::audio::{AudioDevice, AudioManager};
use crate::config::{HeadsetConfig, OutputSwitch};
use crate::debouncer::{Action, Debouncer};
use crate::devices::Device;

pub fn headset_section(ui: &mut Ui, debouncer: &mut Debouncer, headset: &mut HeadsetConfig, device: &dyn Device, audio_devices: &[AudioDevice], audio_manager: &AudioManager) {
    let switch = &mut headset.switch_output;
    if audio_output_switch_selector(ui, switch, audio_devices, || audio_manager.get_default_device()) {
        debouncer.submit(Action::SaveConfig);
        if device.is_connected() {
            apply_audio_switch(true, switch, audio_manager);
        }
    }
    ui.add_space(10.0);
}

fn audio_output_switch_selector(ui: &mut Ui, switch: &mut OutputSwitch,
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

fn audio_device_selector(ui: &mut Ui, label: &str, selected: &mut String, audio_devices: &[AudioDevice]) -> bool {
    let (mut index, mut changed) = audio_devices
        .iter()
        .position(|d| d.name() == selected)
        .map(|i| (i, false))
        .unwrap_or((0, true));
    changed |= ComboBox::from_label(label)
        .width(300.0)
        .show_index(ui, &mut index, audio_devices.len(), |i| audio_devices[i].name().to_string())
        .changed();
    if changed {
        *selected = audio_devices[index].name().to_string();
    }
    changed
}