use egui::*;
use crate::audio::{AudioDevice, AudioSystem};
use crate::config::{CallAction, HeadsetConfig, OsAudio};
use crate::debouncer::{Action, Debouncer};
use crate::devices::Device;
use crate::ui::ResponseExt;

pub fn headset_section(ui: &mut Ui, debouncer: &mut Debouncer, auto_update: bool, headset: &mut HeadsetConfig, device: &dyn Device, audio_system: &mut AudioSystem) {
    if device.get_inactive_time().is_some() {
        ui.horizontal(|ui| {
            DragValue::new(&mut headset.inactive_time)
                .clamp_range(5..=120)
                .ui(ui)
                .submit(debouncer, auto_update, Action::UpdateInactiveTime);
            ui.label("Inactive Time");
        });
    }

    ui.add_space(10.0);
    if let Some(mic_light) = device.get_mic_light() {
        Slider::new(&mut headset.mic_light, 0..=(mic_light.levels() - 1))
            .text("Microphone Light")
            .ui(ui)
            .submit(debouncer, auto_update, Action::UpdateMicrophoneLight);
    }

    ui.add_space(10.0);
    if device.get_bluetooth_config().is_some() {
        Checkbox::new(&mut headset.auto_enable_bluetooth, "Auto Enable Bluetooth")
            .ui(ui)
            .submit(debouncer, auto_update, Action::UpdateAutoBluetooth);
        let actions = [
            (CallAction::Nothing, "Nothing"),
            (CallAction::ReduceVolume, "Reduce Volume"),
            (CallAction::Mute, "Mute")
        ];
        let mut current_index = actions
            .iter()
            .position(|(a, _)| *a == headset.bluetooth_call)
            .unwrap_or(0);
        ComboBox::from_label("Bluetooth Call Action")
            .width(120.0)
            .show_index(ui, &mut current_index, actions.len(), |i| actions[i].1.to_string())
            .submit(debouncer, auto_update, Action::UpdateBluetoothCall);
        headset.bluetooth_call = actions[current_index].0;
    }

    ui.add_space(10.0);
    let switch = &mut headset.os_audio;
    if audio_output_switch_selector(ui, switch, audio_system) {
        debouncer.submit(Action::SaveConfig);
        if auto_update {
            debouncer.submit(Action::UpdateSystemAudio);
            debouncer.force(Action::UpdateSystemAudio);
        }
    }
    ui.add_space(10.0);
}

fn audio_output_switch_selector(ui: &mut Ui, switch: &mut OsAudio, audio_system: &mut AudioSystem) -> bool {
    let mut dirty = false;
    let mut enabled = *switch != OsAudio::Disabled;
    if ui.checkbox(&mut enabled, "Automatic Output Switching").changed() {
        if enabled {
            audio_system.refresh_devices();
            let default_audio = audio_system
                .default_device()
                .or_else(||audio_system
                    .devices()
                    .first())
                .map(|d| d.name().to_string())
                .expect("No device");
            *switch = OsAudio::ChangeDefault {
                on_connect: default_audio.clone(),
                on_disconnect: default_audio,
            };
        } else {
            *switch = OsAudio::Disabled;
        }
        dirty |= true;
    }
    if let OsAudio::ChangeDefault { on_connect, on_disconnect } = switch {
        dirty |= audio_device_selector(ui, "On Connect", on_connect, audio_system.devices());
        dirty |= audio_device_selector(ui, "On Disconnect", on_disconnect, audio_system.devices());
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