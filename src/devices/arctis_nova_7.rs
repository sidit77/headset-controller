use std::time::{Duration, Instant};
use anyhow::{ensure, Result};
use hidapi::{DeviceInfo, HidApi, HidDevice};
use crate::devices::{BatteryLevel, BoxedDevice, ChatMix, Device, DeviceSupport};

const STEELSERIES: u16 = 0x1038;

const ARCTIS_NOVA_7 : u16 = 0x2202;
const ARCTIS_NOVA_7X: u16 = 0x2206;
const ARCTIS_NOVA_7P: u16 = 0x220a;

const SUPPORTED_VENDORS: &[u16] = &[STEELSERIES];
const SUPPORTED_PRODUCTS: &[u16] = &[ARCTIS_NOVA_7, ARCTIS_NOVA_7X, ARCTIS_NOVA_7P];
const REQUIRED_INTERFACE: i32 = 3;

const STATUS_BUF_SIZE: usize = 8;
const READ_TIMEOUT: i32 = 500;

const HEADSET_OFFLINE: u8 = 0x00;
const HEADSET_CHARGING: u8 = 0x01;

const BATTERY_MAX: u8 =  0x04;
const BATTERY_MIN: u8 =  0x00;

#[derive(Debug)]
pub struct ArcticsNova7 {
    device: HidDevice,
    id: u32,
    name: String,
    last_chat_mix_adjustment: Option<Instant>,
    connected: bool,
    battery: BatteryLevel,
    chat_mix: ChatMix
}

impl ArcticsNova7 {
    pub const SUPPORT: DeviceSupport = DeviceSupport {
        is_supported: Self::is_supported,
        open: Self::open,
    };
    fn is_supported(device_info: &DeviceInfo) -> bool {
        SUPPORTED_VENDORS.contains(&device_info.vendor_id())
            && SUPPORTED_PRODUCTS.contains(&device_info.product_id())
            && REQUIRED_INTERFACE == device_info.interface_number()
    }
    fn open(device_info: &DeviceInfo, api: &HidApi) -> Result<BoxedDevice> {
        ensure!(Self::is_supported(device_info));
        let device = device_info.open_device(api)?;
        let id = ((device_info.vendor_id() as u32) << 16) | (device_info.product_id() as u32);
        let manufacturer = device_info
            .manufacturer_string()
            .unwrap_or("SteelSeries");
        let product = device_info
            .product_string()
            .unwrap_or("Arctis Nova 7");
        Ok(Box::new(ArcticsNova7 {
            device,
            id,
            name: format!("{} {}", manufacturer, product),
            last_chat_mix_adjustment: None,
            connected: false,
            battery: BatteryLevel::Unknown,
            chat_mix: ChatMix::default(),
        }))
    }

}

impl Device for ArcticsNova7 {

    fn get_device_id(&self) -> u32 {
        self.id
    }

    fn get_name(&self) -> &str {
        &self.name
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn poll(&mut self) -> Result<Duration> {
        let mut report = [0u8; STATUS_BUF_SIZE];
        self.device.write(&[0x00, 0xb0])?;
        ensure!(self.device.read_timeout(&mut report, READ_TIMEOUT)? == STATUS_BUF_SIZE);

        let prev_chat_mix = self.chat_mix;
        self.chat_mix = ChatMix {
            game: report[4],
            chat: report[5],
        };
        match report[3] {
            HEADSET_OFFLINE => {
                self.connected = false;
                self.battery = BatteryLevel::Unknown;
                self.chat_mix = ChatMix::default();
            },
            HEADSET_CHARGING => {
                self.connected = true;
                self.battery = BatteryLevel::Charging;
            },
            _ =>  {
                self.connected = true;
                self.battery = BatteryLevel::Level({
                    let level = report[2].min(BATTERY_MAX).max(BATTERY_MIN);
                    (level - BATTERY_MIN) * (100 / (BATTERY_MAX - BATTERY_MIN))
                });
            }
        }
        if self.chat_mix != prev_chat_mix {
            if self.last_chat_mix_adjustment.is_none() {
                log::trace!("Increase polling rate");
            }
            self.last_chat_mix_adjustment = Some(Instant::now());
        }
        if self.last_chat_mix_adjustment.map(|i| i.elapsed() > Duration::from_secs(1)).unwrap_or(false) {
            self.last_chat_mix_adjustment = None;
            log::trace!("Decrease polling rate");
        }

        Ok(match self.connected {
            true => match self.last_chat_mix_adjustment.is_some() {
                true => Duration::from_millis(250),
                false => Duration::from_millis(1000)
            }
            false => Duration::from_secs(4)
        })
    }

    fn get_battery_status(&self) -> Option<BatteryLevel> {
        //assert!(self.connected);
        Some(self.battery)
    }

    fn get_chat_mix(&self) -> Option<ChatMix> {
        //assert!(self.connected);
        Some(self.chat_mix)
    }
}