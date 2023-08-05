use egui::*;
use tracing::instrument;

use crate::audio::{AudioDevice, AudioSystem};
use crate::config::{CallAction, HeadsetConfig, OsAudio};
use crate::debouncer::{Action, Debouncer};
use crate::devices::Device;
use crate::ui::ResponseExt;

#[instrument(skip_all)]
pub fn headset_section(
    ui: &mut Ui, debouncer: &mut Debouncer, auto_update: bool, headset: &mut HeadsetConfig, device: &dyn Device, audio_system: &mut AudioSystem
) {
    if device.get_inactive_time().is_some() {
        ui.horizontal(|ui| {
            DragValue::new(&mut headset.inactive_time)
                .clamp_range(5..=120)
                .ui(ui)
                .submit(debouncer, auto_update, Action::UpdateInactiveTime);
            ui.label("Inactive Time");
        });
        ui.add_space(10.0);
    }

    if let Some(mic_light) = device.get_mic_light() {
        Slider::new(&mut headset.mic_light, 0..=(mic_light.levels() - 1))
            .text("Microphone Light")
            .ui(ui)
            .submit(debouncer, auto_update, Action::UpdateMicrophoneLight);
        ui.add_space(10.0);
    }

    if device.get_bluetooth_config().is_some() {
        Checkbox::new(&mut headset.auto_enable_bluetooth, "Auto Enable Bluetooth")
            .ui(ui)
            .submit(debouncer, auto_update, Action::UpdateAutoBluetooth);
        ui.add_space(10.0);
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
        ui.add_space(10.0);
    }

    if audio_system.is_running() {
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
}

fn get_name(switch: &OsAudio) -> &str {
    match switch {
        OsAudio::Disabled => "Disabled",
        OsAudio::ChangeDefault { .. } => "Change Default Device",
        OsAudio::RouteAudio { .. } => "Route Audio When Disconnected"
    }
}

fn audio_output_switch_selector(ui: &mut Ui, switch: &mut OsAudio, audio_system: &mut AudioSystem) -> bool {
    let mut dirty = false;
    let resp = ComboBox::from_label("Audio Action")
        .selected_text(get_name(switch))
        .width(250.0)
        .show_ui(ui, |ui| {
            let default_device = audio_system
                .default_device()
                .or_else(|| audio_system.devices().first())
                .map(|d| d.name().to_string())
                .unwrap_or_else(|| String::from("<None>"));
            let options = [
                OsAudio::Disabled,
                OsAudio::ChangeDefault {
                    on_connect: default_device.clone(),
                    on_disconnect: default_device.clone()
                },
                OsAudio::RouteAudio {
                    src: default_device.clone(),
                    dst: default_device
                }
            ];
            for option in options {
                let current = std::mem::discriminant(switch) == std::mem::discriminant(&option);
                if ui.selectable_label(current, get_name(&option)).clicked() && !current {
                    *switch = option;
                    dirty = true;
                }
            }
        });
    if resp.response.clicked() {
        audio_system.refresh_devices();
    }
    if let OsAudio::ChangeDefault { on_connect, on_disconnect } = switch {
        dirty |= audio_device_selector(ui, "On Connect", on_connect, audio_system.devices());
        dirty |= audio_device_selector(ui, "On Disconnect", on_disconnect, audio_system.devices());
    }
    if let OsAudio::RouteAudio { src, dst } = switch {
        dirty |= audio_device_selector(ui, "From", src, audio_system.devices());
        dirty |= audio_device_selector(ui, "To", dst, audio_system.devices());
    }
    dirty
}

fn audio_device_selector(ui: &mut Ui, label: &str, selected: &mut String, audio_devices: &[AudioDevice]) -> bool {
    let mut changed = false;
    ComboBox::from_label(label)
        .width(300.0)
        .selected_text(selected.as_str())
        .show_ui(ui, |ui| {
            for dev in audio_devices {
                let current = dev.name() == selected;
                if ui.selectable_label(current, dev.name()).clicked() && !current {
                    *selected = dev.name().to_string();
                    changed = true;
                }
            }
        });
    changed
}
