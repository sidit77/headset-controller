use std::time::Duration;
use hidapi::HidApi;
use once_cell::sync::Lazy;
use tracing::instrument;
use crate::config::CallAction;
use crate::devices::{BatteryLevel, BluetoothConfig, BoxedDevice, ChatMix, Device, DeviceResult, Equalizer, InactiveTime, Info, MicrophoneLight, MicrophoneVolume, SideTone, SupportedDevice, VolumeLimiter};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DummyDevice;

impl DummyDevice {
    pub fn new() -> Box<dyn SupportedDevice> {
        Box::new(DummyDevice)
    }
}

impl SupportedDevice for DummyDevice {
    fn get_info(&self) -> &Info {
        static INFO: Lazy<Info> = Lazy::new(|| Info {
            manufacturer: "DummyCorp".to_string(),
            product: "DummyDevice".to_string(),
            name: "DummyDevice".to_string()
        });
        &INFO
    }

    fn open(&self, _: &HidApi) -> DeviceResult<BoxedDevice> {
        Ok(Box::new(DummyDevice))
    }
}

impl Device for DummyDevice {
    fn get_info(&self) -> &Info {
        SupportedDevice::get_info(self)
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn poll(&mut self) -> DeviceResult<Duration> {
        Ok(Duration::from_secs(2))
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
    fn set_level(&self, level: u8) -> DeviceResult<()> {
        tracing::info!("Updated sidetone");
        Ok(())
    }
}

impl MicrophoneVolume for DummyDevice {
    fn levels(&self) -> u8 {
        12
    }

    #[instrument(skip(self))]
    fn set_level(&self, level: u8) -> DeviceResult<()> {
        tracing::info!("Updated microphone volume");
        Ok(())
    }
}

impl MicrophoneLight for DummyDevice {
    fn levels(&self) -> u8 {
        2
    }

    #[instrument(skip(self))]
    fn set_light_strength(&self, level: u8) -> DeviceResult<()> {
        tracing::info!("Updated microphone light");
        Ok(())
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
        &[
            ("Default", &[8; 13])
        ]
    }

    #[instrument(skip(self))]
    fn set_levels(&self, levels: &[u8]) -> DeviceResult<()> {
        tracing::info!("Updated equalizer");
        Ok(())
    }
}

impl VolumeLimiter for DummyDevice {

    #[instrument(skip(self))]
    fn set_enabled(&self, enabled: bool) -> DeviceResult<()> {
        tracing::info!("Updated volume limiter");
        Ok(())
    }
}

impl BluetoothConfig for DummyDevice {

    #[instrument(skip(self))]
    fn set_call_action(&self, action: CallAction) -> DeviceResult<()> {
        tracing::info!("Updated call action");
        Ok(())
    }

    #[instrument(skip(self))]
    fn set_auto_enabled(&self, enabled: bool) -> DeviceResult<()> {
        tracing::info!("Updated auto enable");
        Ok(())
    }
}

impl InactiveTime for DummyDevice {

    #[instrument(skip(self))]
    fn set_inactive_time(&self, minutes: u8) -> DeviceResult<()> {
        tracing::info!("Updated inactive time");
        Ok(())
    }
}