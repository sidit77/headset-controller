mod arctis_nova_7;

use std::fmt::{Display, Formatter};
use std::time::Duration;
use color_eyre::{Result};
use color_eyre::eyre::eyre;
use hidapi::{DeviceInfo, HidApi};
use crate::config::CallAction;
use crate::util::PeekExt;
use crate::devices::arctis_nova_7::ArcticsNova7;

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
pub enum BatteryLevel {
    #[default]
    Unknown,
    Charging,
    Level(u8)
}

impl Display for BatteryLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            BatteryLevel::Unknown => write!(f, "Error"),
            BatteryLevel::Charging => write!(f, "Charging"),
            BatteryLevel::Level(level) => write!(f, "{}%", level),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ChatMix {
    pub game: u8,
    pub chat: u8
}

impl Default for ChatMix {
    fn default() -> Self {
        Self {
            game: 100,
            chat: 100,
        }
    }
}


pub trait SideTone {
    fn levels(&self) -> u8;
    fn set_level(&self, level: u8) -> Result<()>;
}

pub trait VolumeLimiter {
    fn set_enabled(&self, enabled: bool) -> Result<()>;
}

pub trait MicrophoneVolume {
    fn levels(&self) -> u8;
    fn set_level(&self, level: u8) -> Result<()>;
}

pub trait Equalizer {
    fn bands(&self) -> u8;
    fn base_level(&self) -> u8;
    fn variance(&self) -> u8;
    fn presets(&self) -> &[(&str, &[u8])];
    fn set_levels(&self, levels: &[u8]) -> Result<()>;
}

pub trait BluetoothConfig {
    fn set_call_action(&self, action: CallAction) -> Result<()>;
    fn set_auto_enabled(&self, enabled: bool) -> Result<()>;
}

pub trait MicrophoneLight {
    fn levels(&self) -> u8;
    fn set_light_strength(&self, level: u8) -> Result<()>;
}

pub trait InactiveTime {
    fn set_inactive_time(&self, minutes: u8) -> Result<()>;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Info {
    pub manufacturer: String,
    pub product: String,
    pub name: String,
    pub id: u32
}

impl Display for Info {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

pub trait Device {

    fn get_info(&self) -> &Info;
    fn is_connected(&self) -> bool;
    fn poll(&mut self) -> Result<Duration>;

    fn get_battery_status(&self) -> Option<BatteryLevel> {
        None
    }
    fn get_chat_mix(&self) -> Option<ChatMix> {
        None
    }
    fn get_side_tone(&self) -> Option<&dyn SideTone> {
        None
    }
    fn get_mic_volume(&self) -> Option<&dyn MicrophoneVolume> {
        None
    }
    fn get_volume_limiter(&self) -> Option<&dyn VolumeLimiter> {
        None
    }
    fn get_equalizer(&self) -> Option<&dyn Equalizer> {
        None
    }
    fn get_bluetooth_config(&self) -> Option<&dyn BluetoothConfig> {
        None
    }
    fn get_inactive_time(&self) -> Option<&dyn InactiveTime> {
        None
    }
    fn get_mic_light(&self) -> Option<&dyn MicrophoneLight> {
        None
    }
}

pub type BoxedDevice = Box<dyn Device>;

#[derive(Copy, Clone)]
pub struct DeviceSupport {
    is_supported: fn(device_info: &DeviceInfo) -> bool,
    open: fn(device_info: &DeviceInfo, api: &HidApi) -> Result<BoxedDevice>
}

const SUPPORTED_DEVICES: &[DeviceSupport] = &[
    ArcticsNova7::SUPPORT
];

pub fn find_device() -> Result<Box<dyn Device>> {
    let api = HidApi::new()?;
    api
        .device_list()
        .filter_map(|info|
            SUPPORTED_DEVICES
                .iter()
                .find(|supp| (supp.is_supported)(info))
                .zip(Some(info)))
        .inspect(|(_, info) | {
            tracing::info!("Found {} {}", info.manufacturer_string().unwrap_or(""), info.product_string().unwrap_or(""));
        })
        .collect::<Vec<_>>()
        .first()
        .peek(|(_, info)| tracing::info!("Selected {} {}", info.manufacturer_string().unwrap_or(""), info.product_string().unwrap_or("")))
        .map(|(support, info)|(support.open)(info, &api))
        .ok_or_else(|| eyre!("No supported device found!"))?
}