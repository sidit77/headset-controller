mod arctis_nova_7;
//mod dummy;

use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use async_hid::DeviceInfo;

use color_eyre::eyre::Error as EyreError;
use tracing::instrument;

use crate::config::{CallAction};
use crate::devices::arctis_nova_7::{ARCTIS_NOVA_7X};
//use crate::devices::dummy::DummyDevice;

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

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Interface {
    pub product_id: u16,
    pub vendor_id: u16,
    pub usage_id: u16,
    pub usage_page: u16
}

impl Interface {
    pub const fn new(usage_page: u16, usage_id: u16, vendor_id: u16, product_id: u16) -> Self {
        Self { product_id, vendor_id, usage_id, usage_page }
    }
}

impl From<&DeviceInfo> for Interface {
    fn from(value: &DeviceInfo) -> Self {
        Interface::new(value.usage_page, value.usage_id, value.vendor_id, value.product_id)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct SupportedDevice {
    name: &'static str,
    required_interfaces: &'static [Interface],
    open: fn(interfaces: &HashMap<Interface, DeviceInfo>) -> BoxedDeviceFuture
}

impl Display for SupportedDevice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
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

    fn name(&self) -> &str {
        &self.get_info().name
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
pub type BoxedDevice = Box<dyn Device>;
pub type BoxedDeviceFuture<'a> = Pin<Box<dyn Future<Output=DeviceResult<BoxedDevice>> + 'a>>;

pub const SUPPORTED_DEVICES: &[SupportedDevice] = &[ARCTIS_NOVA_7X];

#[derive(Debug, Clone, Default)]
pub struct DeviceManager {
    interfaces: HashMap<Interface, DeviceInfo>,
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
        self.interfaces.extend(DeviceInfo::enumerate()
            .await?
            .into_iter()
            .map(|dev| (Interface::from(&dev), dev)));

        self.devices.clear();
        self.devices.extend(SUPPORTED_DEVICES
            .iter()
            .filter(|dev| dev
                .required_interfaces
                .iter()
                .all(|i| self.interfaces.contains_key(i)))
            .inspect(|dev| println!("Found {}", dev.name)));

        Ok(())
    }

    pub fn supported_devices(&self) -> &Vec<SupportedDevice> {
        &self.devices
    }

    pub async fn open(&self, supported: &SupportedDevice) -> DeviceResult<BoxedDevice> {
        println!("Opening {}", supported.name);
        let dev = (supported.open)(&self.interfaces).await?;

        Ok(dev)
    }
}

/*
pub struct DeviceManager {
    api: HidApi,
    devices: Vec<BoxedSupportedDevice>
}

impl DeviceManager {
    pub fn new() -> DeviceResult<Self> {
        let api = HidApi::new()?;
        let mut result = Self { api, devices: Vec::new() };
        result.find_supported_devices();
        Ok(result)
    }

    #[instrument(skip_all)]
    fn find_supported_devices(&mut self) {
        self.devices.clear();
        self.api
            .device_list()
            .flat_map(|info| SUPPORTED_DEVICES.iter().filter_map(|check| check(info)))
            .chain(DUMMY_DEVICE
                .then::<Box<dyn SupportedDevice>, _>(|| Box::new(DummyDevice)))
            .for_each(|dev| {
                tracing::debug!("Found {}", dev.name());
                self.devices.push(dev);
            });
    }

    pub fn refresh(&mut self) -> DeviceResult<()> {
        self.api.refresh_devices()?;
        self.find_supported_devices();
        Ok(())
    }

    pub fn supported_devices(&self) -> &Vec<BoxedSupportedDevice> {
        &self.devices
    }

    pub fn open(&self, supported: &dyn SupportedDevice) -> DeviceResult<BoxedDevice> {
        supported.open(&self.api)
    }

    #[instrument(skip(self))]
    pub fn find_preferred_device(&self, preference: &Option<String>) -> Option<BoxedDevice> {
        preference
            .iter()
            .flat_map(|pref| self.devices.iter().filter(move |dev| dev.name() == pref))
            .chain(self.devices.iter())
            .filter_map(|dev| {
                self.open(dev.as_ref())
                    .map_err(|err| tracing::error!("Failed to open device: {:?}", err))
                    .ok()
            })
            .next()
    }
}
*/

pub type DeviceResult<T> = Result<T, EyreError>;

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
