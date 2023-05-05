mod arctis_nova_7;
mod dummy;

use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;
use std::time::Duration;

use color_eyre::eyre::Error as EyreError;
use hidapi::{DeviceInfo, HidApi, HidDevice, HidError};

use crate::config::CallAction;
use crate::devices::arctis_nova_7::ArcticsNova7;
use crate::devices::dummy::DummyDevice;

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

pub trait SideTone {
    fn levels(&self) -> u8;
    fn set_level(&self, level: u8) -> DeviceResult<()>;
}

pub trait VolumeLimiter {
    fn set_enabled(&self, enabled: bool) -> DeviceResult<()>;
}

pub trait MicrophoneVolume {
    fn levels(&self) -> u8;
    fn set_level(&self, level: u8) -> DeviceResult<()>;
}

pub trait Equalizer {
    fn bands(&self) -> u8;
    fn base_level(&self) -> u8;
    fn variance(&self) -> u8;
    fn presets(&self) -> &[(&str, &[u8])];
    fn set_levels(&self, levels: &[u8]) -> DeviceResult<()>;
}

pub trait BluetoothConfig {
    fn set_call_action(&self, action: CallAction) -> DeviceResult<()>;
    fn set_auto_enabled(&self, enabled: bool) -> DeviceResult<()>;
}

pub trait MicrophoneLight {
    fn levels(&self) -> u8;
    fn set_light_strength(&self, level: u8) -> DeviceResult<()>;
}

pub trait InactiveTime {
    fn set_inactive_time(&self, minutes: u8) -> DeviceResult<()>;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Info {
    pub manufacturer: String,
    pub product: String,
    pub name: String
}

impl Display for Info {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

pub trait Device {
    fn get_info(&self) -> &Info;
    fn is_connected(&self) -> bool;
    fn poll(&mut self) -> DeviceResult<Duration>;

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

pub trait SupportedDevice {
    fn get_info(&self) -> &Info;
    fn open(&self, api: &HidApi) -> DeviceResult<BoxedDevice>;
}

pub type CheckSupport = fn(info: &DeviceInfo) -> Option<Box<dyn SupportedDevice>>;

#[derive(Debug, Clone)]
pub struct GenericHidDevice<T> {
    device_info: DeviceInfo,
    info: Info,
    _marker: PhantomData<T>
}

impl<T> GenericHidDevice<T> {
    pub fn new(info: &DeviceInfo, fallback_manufacturer: &str, fallback_product: &str) -> Self {
        let manufacturer = info
            .manufacturer_string()
            .unwrap_or(fallback_manufacturer)
            .to_string();
        let product = info
            .product_string()
            .unwrap_or(fallback_product)
            .to_string();
        let name = format!("{} {}", manufacturer, product);
        Self {
            device_info: info.clone(),
            info: Info { manufacturer, product, name },
            _marker: Default::default()
        }
    }
}

impl<T: From<(HidDevice, Info)> + Device + 'static> SupportedDevice for GenericHidDevice<T> {
    fn get_info(&self) -> &Info {
        &self.info
    }

    fn open(&self, api: &HidApi) -> DeviceResult<BoxedDevice> {
        let device = self.device_info.open_device(api)?;
        Ok(Box::new(T::from((device, self.info.clone()))))
    }
}

const SUPPORTED_DEVICES: &[CheckSupport] = &[ArcticsNova7::SUPPORT];

fn get_dummy_device() -> Option<Box<dyn SupportedDevice>> {
    match cfg!(debug_assertions) {
        true => Some(Box::new(DummyDevice)),
        false => None
    }
}

pub struct DeviceManager(HidApi);

impl DeviceManager {
    pub fn new() -> DeviceResult<Self> {
        Ok(Self(HidApi::new()?))
    }

    pub fn supported_devices(&self) -> Vec<Box<dyn SupportedDevice>> {
        self.0
            .device_list()
            .flat_map(|info| SUPPORTED_DEVICES.iter().filter_map(|check| check(info)))
            .chain(get_dummy_device())
            .collect::<Vec<_>>()
    }

    pub fn open(&self, supported: &dyn SupportedDevice) -> DeviceResult<BoxedDevice> {
        supported.open(&self.0)
    }
}

pub type DeviceResult<T> = Result<T, DeviceError>;

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
