mod arctis_nova_7;
mod dummy;

use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display, Formatter, Write};
use std::future::Future;
use std::pin::Pin;

use async_hid::{DeviceInfo, HidError};
use color_eyre::eyre::Error as EyreError;
use futures_lite::stream::StreamExt;
use tao::event_loop::EventLoopProxy;
use tracing::instrument;

use crate::config::{CallAction, DUMMY_DEVICE as DUMMY_DEVICE_ENABLED};
use crate::devices::arctis_nova_7::{ARCTIS_NOVA_7, ARCTIS_NOVA_7P, ARCTIS_NOVA_7X};
use crate::devices::dummy::DUMMY_DEVICE;

pub const SUPPORTED_DEVICES: &[SupportedDevice] = &[ARCTIS_NOVA_7, ARCTIS_NOVA_7X, ARCTIS_NOVA_7P];

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u16)]
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
            BatteryLevel::Level(level) => write!(f, "{}%", level)
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
        Self { game: 100, chat: 100 }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Interface {
    pub product_id: u16,
    pub vendor_id: u16,
    pub usage_id: u16,
    pub usage_page: u16
}

impl Interface {
    pub const fn new(usage_page: u16, usage_id: u16, vendor_id: u16, product_id: u16) -> Self {
        Self {
            product_id,
            vendor_id,
            usage_id,
            usage_page
        }
    }
}

impl From<&DeviceInfo> for Interface {
    fn from(value: &DeviceInfo) -> Self {
        Interface::new(value.usage_page, value.usage_id, value.vendor_id, value.product_id)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct DeviceStrings {
    pub name: &'static str,
    pub manufacturer: &'static str,
    pub product: &'static str
}

impl DeviceStrings {
    pub const fn new(name: &'static str, manufacturer: &'static str, product: &'static str) -> Self {
        Self { name, manufacturer, product }
    }
}

pub type InterfaceMap = HashMap<Interface, DeviceInfo>;
pub type UpdateChannel = EventLoopProxy<DeviceUpdate>;
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct SupportedDevice {
    pub strings: DeviceStrings,
    required_interfaces: &'static [Interface],
    open: fn(channel: UpdateChannel, interfaces: &InterfaceMap) -> BoxedDeviceFuture
}

impl Display for SupportedDevice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl SupportedDevice {
    pub const fn name(&self) -> &'static str {
        self.strings.name
    }
}

#[derive(Debug)]
pub enum DeviceUpdate {
    ConnectionChanged,
    ChatMixChanged,
    BatteryLevel,
    DeviceError(HidError)
}

#[derive(Debug, Clone, Default)]
pub struct DeviceManager {
    interfaces: InterfaceMap,
    devices: Vec<SupportedDevice>
}

impl DeviceManager {
    pub async fn new() -> DeviceResult<Self> {
        let mut result = Self::default();
        result.refresh().await?;
        Ok(result)
    }

    #[instrument(skip_all)]
    pub async fn refresh(&mut self) -> DeviceResult<()> {
        self.interfaces.clear();
        DeviceInfo::enumerate()
            .await?
            .map(|dev| (Interface::from(&dev), dev))
            .for_each(|(info, dev)| _ = self.interfaces.insert(info, dev))
            .await;

        self.devices.clear();
        self.devices.extend(
            SUPPORTED_DEVICES
                .iter()
                .chain(DUMMY_DEVICE_ENABLED.then_some(&DUMMY_DEVICE))
                .filter(|dev| {
                    dev.required_interfaces
                        .iter()
                        .all(|i| self.interfaces.contains_key(i))
                })
                .inspect(|dev| tracing::trace!("Found {}", dev.strings.name))
        );

        Ok(())
    }

    pub fn supported_devices(&self) -> &Vec<SupportedDevice> {
        &self.devices
    }

    pub async fn open(&self, supported: &SupportedDevice, update_channel: UpdateChannel) -> DeviceResult<BoxedDevice> {
        tracing::trace!("Attempting to open {}", supported.strings.name);
        let dev = (supported.open)(update_channel, &self.interfaces).await?;

        Ok(dev)
    }

    #[instrument(skip(self, update_channel))]
    pub async fn find_preferred_device(&self, preference: &Option<String>, update_channel: UpdateChannel) -> Option<BoxedDevice> {
        let device_iter = preference
            .iter()
            .flat_map(|pref| {
                self.devices
                    .iter()
                    .filter(move |dev| dev.strings.name == pref)
            })
            .chain(self.devices.iter());
        for device in device_iter {
            match self.open(device, update_channel.clone()).await {
                Ok(dev) => return Some(dev),
                Err(err) => tracing::error!("Failed to open device: {:?}", err)
            }
        }
        None
    }
}

pub fn generate_udev_rules() -> DeviceResult<String> {
    let mut rules = String::new();

    writeln!(rules, r#"ACTION!="add|change", GOTO="headsets_end""#)?;
    writeln!(rules, "")?;

    for device in SUPPORTED_DEVICES {
        writeln!(rules, "# {}", device.strings.name)?;
        let codes: HashSet<_> = device
            .required_interfaces
            .iter()
            .map(|i| (i.vendor_id, i.product_id))
            .collect();
        for (vid, pid) in codes {
            writeln!(rules, r#"KERNEL=="hidraw*", ATTRS{{idVendor}}=="{vid:04x}", ATTRS{{idProduct}}=="{pid:04x}", TAG+="uaccess""#)?;
        }
        writeln!(rules, "")?;
    }

    writeln!(rules, r#"LABEL="headsets_end""#)?;
    Ok(rules)
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
enum ConfigAction {
    SetSideTone(u8),
    EnableVolumeLimiter(bool),
    SetMicrophoneVolume(u8),
    SetEqualizerLevels(Vec<u8>),
    SetBluetoothCallAction(CallAction),
    EnableAutoBluetoothActivation(bool),
    SetMicrophoneLightStrength(u8),
    SetInactiveTime(u8)
}

pub type DeviceResult<T> = Result<T, EyreError>;
pub type BoxedDevice = Box<dyn Device>;
pub type BoxedDeviceFuture<'a> = Pin<Box<dyn Future<Output = DeviceResult<BoxedDevice>> + 'a>>;

pub trait Device {
    fn strings(&self) -> DeviceStrings;
    fn is_connected(&self) -> bool;

    fn name(&self) -> &'static str {
        self.strings().name
    }
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

pub trait SideTone {
    fn levels(&self) -> u8;
    fn set_level(&self, level: u8);
}

pub trait VolumeLimiter {
    fn set_enabled(&self, enabled: bool);
}

pub trait MicrophoneVolume {
    fn levels(&self) -> u8;
    fn set_level(&self, level: u8);
}

pub trait Equalizer {
    fn bands(&self) -> u8;
    fn base_level(&self) -> u8;
    fn variance(&self) -> u8;
    fn presets(&self) -> &[(&str, &[u8])];
    fn set_levels(&self, levels: &[u8]);
}

pub trait BluetoothConfig {
    fn set_call_action(&self, action: CallAction);
    fn set_auto_enabled(&self, enabled: bool);
}

pub trait MicrophoneLight {
    fn levels(&self) -> u8;
    fn set_light_strength(&self, level: u8);
}

pub trait InactiveTime {
    fn set_inactive_time(&self, minutes: u8);
}

/*
#[derive(Debug)]
pub enum DeviceError {
    Hid(HidError),
    Other(EyreError)
}


impl From<HidError> for DeviceError {
    fn from(value: HidError) -> Self {
        Self::Hid(value)
    }
}

impl From<EyreError> for DeviceError {
    fn from(value: EyreError) -> Self {
        Self::Other(value)
    }
}

impl Display for DeviceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceError::Hid(err) => Display::fmt(err, f),
            DeviceError::Other(err) => Display::fmt(err, f)
        }
    }
}

impl Error for DeviceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DeviceError::Hid(err) => Some(err),
            DeviceError::Other(err) => err.source()
        }
    }
}
*/
