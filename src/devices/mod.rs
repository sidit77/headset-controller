mod arctis_nova_7;

use std::time::Duration;
use anyhow::{anyhow, Result};
use hidapi::{DeviceInfo, HidApi};
use crate::util::PeekExt;
use crate::devices::arctis_nova_7::ArcticsNova7;

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
pub enum BatteryLevel {
    #[default]
    Unknown,
    Charging,
    Level(u8)
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
    fn set_level(&self, level: u8);
}

pub trait Device {

    fn get_device_id(&self) -> u32;
    fn get_name(&self) -> &str;
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
            log::info!("Found {} {}", info.manufacturer_string().unwrap_or(""), info.product_string().unwrap_or(""));
        })
        .collect::<Vec<_>>()
        .first()
        .peek(|(_, info)| log::info!("Selected {} {}", info.manufacturer_string().unwrap_or(""), info.product_string().unwrap_or("")))
        .map(|(support, info)|(support.open)(info, &api))
        .ok_or_else(|| anyhow!("No supported device found!"))?
}