use std::future::ready;

use tracing::instrument;

use crate::config::CallAction;
use crate::devices::*;

pub const DUMMY_DEVICE: SupportedDevice = SupportedDevice {
    strings: DeviceStrings::new("DummyDevice", "DummyCorp", "DummyDevice"),
    required_interfaces: &[],
    open: create_dummy
};

fn create_dummy(_: UpdateChannel, _: &InterfaceMap) -> BoxedDeviceFuture {
    let dummy: BoxedDevice = Box::new(DummyDevice);
    Box::pin(ready(Ok(dummy)))
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DummyDevice;

impl Device for DummyDevice {
    fn strings(&self) -> DeviceStrings {
        DUMMY_DEVICE.strings
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn get_battery_status(&self) -> Option<BatteryLevel> {
        Some(BatteryLevel::Charging)
    }

    fn get_chat_mix(&self) -> Option<ChatMix> {
        Some(ChatMix::default())
    }

    fn get_side_tone(&self) -> Option<&dyn SideTone> {
        Some(self)
    }

    fn get_mic_volume(&self) -> Option<&dyn MicrophoneVolume> {
        Some(self)
    }

    fn get_volume_limiter(&self) -> Option<&dyn VolumeLimiter> {
        Some(self)
    }

    fn get_equalizer(&self) -> Option<&dyn Equalizer> {
        Some(self)
    }

    fn get_bluetooth_config(&self) -> Option<&dyn BluetoothConfig> {
        Some(self)
    }

    fn get_inactive_time(&self) -> Option<&dyn InactiveTime> {
        Some(self)
    }

    fn get_mic_light(&self) -> Option<&dyn MicrophoneLight> {
        Some(self)
    }
}

impl SideTone for DummyDevice {
    fn levels(&self) -> u8 {
        6
    }

    #[instrument(skip(self))]
    fn set_level(&self, level: u8) {
        tracing::info!("Updated sidetone");
    }
}

impl MicrophoneVolume for DummyDevice {
    fn levels(&self) -> u8 {
        12
    }

    #[instrument(skip(self))]
    fn set_level(&self, level: u8) {
        tracing::info!("Updated microphone volume");
    }
}

impl MicrophoneLight for DummyDevice {
    fn levels(&self) -> u8 {
        2
    }

    #[instrument(skip(self))]
    fn set_light_strength(&self, level: u8) {
        tracing::info!("Updated microphone light");
    }
}

impl Equalizer for DummyDevice {
    fn bands(&self) -> u8 {
        13
    }

    fn base_level(&self) -> u8 {
        8
    }

    fn variance(&self) -> u8 {
        3
    }

    fn presets(&self) -> &[(&str, &[u8])] {
        &[("Default", &[8; 13])]
    }

    #[instrument(skip(self))]
    fn set_levels(&self, levels: &[u8]) {
        tracing::info!("Updated equalizer");
    }
}

impl VolumeLimiter for DummyDevice {
    #[instrument(skip(self))]
    fn set_enabled(&self, enabled: bool) {
        tracing::info!("Updated volume limiter");
    }
}

impl BluetoothConfig for DummyDevice {
    #[instrument(skip(self))]
    fn set_call_action(&self, action: CallAction) {
        tracing::info!("Updated call action");
    }

    #[instrument(skip(self))]
    fn set_auto_enabled(&self, enabled: bool) {
        tracing::info!("Updated auto enable");
    }
}

impl InactiveTime for DummyDevice {
    #[instrument(skip(self))]
    fn set_inactive_time(&self, minutes: u8) {
        tracing::info!("Updated inactive time");
    }
}
