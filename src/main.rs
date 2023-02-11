use std::fmt::Display;
use std::thread;
use std::time::Duration;
use anyhow::{anyhow, ensure, Result};
use hidapi::HidApi;
use log::LevelFilter;

const STEELSERIES: u16 = 0x1038;

const ARCTIS_NOVA_7 : u16 = 0x2202;
const ARCTIS_NOVA_7X: u16 = 0x2206;
const ARCTIS_NOVA_7P: u16 = 0x220a;

const SUPPORTED_VENDORS: &[u16] = &[STEELSERIES];
const SUPPORTED_PRODUCTS: &[u16] = &[ARCTIS_NOVA_7, ARCTIS_NOVA_7X, ARCTIS_NOVA_7P];

const STATUS_BUF_SIZE: usize = 8;
const READ_TIMEOUT: i32 = 500;

const HEADSET_OFFLINE: u8 = 0x00;
const HEADSET_CHARGING: u8 = 0x01;

const BATTERY_MAX: u8 =  0x04;
const BATTERY_MIN: u8 =  0x00;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum BatteryStatus {
    Charging,
    Level(u8)
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ChatMix {
    game: u8,
    chat: u8
}

impl Default for ChatMix {
    fn default() -> Self {
        Self {
            game: 100,
            chat: 100,
        }
    }
}

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
pub enum HeadsetStatus {
    #[default]
    Disconnected,
    Connected {
        battery: BatteryStatus,
        chat_mix: ChatMix
    }
}

impl From<[u8; STATUS_BUF_SIZE]> for HeadsetStatus {
    fn from(report: [u8; STATUS_BUF_SIZE]) -> Self {
        let chat_mix = ChatMix {
            game: report[4],
            chat: report[5],
        };
        match report[3] {
            HEADSET_OFFLINE => HeadsetStatus::Disconnected,
            HEADSET_CHARGING => HeadsetStatus::Connected {
                battery: BatteryStatus::Charging,
                chat_mix,
            },
            _ => HeadsetStatus::Connected {
                battery: BatteryStatus::Level({
                    let level = report[2].min(BATTERY_MAX).max(BATTERY_MIN);
                    (level - BATTERY_MIN) * (100 / (BATTERY_MAX - BATTERY_MIN))
                }),
                chat_mix,
            }
        }
    }
}

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        .format_timestamp(None)
        .parse_default_env()
        .init();

    let api = HidApi::new()?;
    let device = api
        .device_list()
        .filter(|info| SUPPORTED_VENDORS.contains(&info.vendor_id()))
        .filter(|info| SUPPORTED_PRODUCTS.contains(&info.product_id()))
        .inspect(|info | {
            log::info!("Found {} {}", info.manufacturer_string().unwrap_or(""), info.product_string().unwrap_or(""));
        })
        .next()
        .peek(|info| log::info!("Selected {} {}", info.manufacturer_string().unwrap_or(""), info.product_string().unwrap_or("")))
        .ok_or_else(|| anyhow!("No supported device found!"))?
        .open_device(&api)?;

    let mut previous_status = None;

    let mut buf = [0u8; STATUS_BUF_SIZE];
    loop {
        device.write(&[0x00, 0xb0])?;
        ensure!(device.read_timeout(&mut buf, READ_TIMEOUT)? == STATUS_BUF_SIZE);
        let current_status = HeadsetStatus::from(buf);
        if previous_status.map(|s| s != current_status).unwrap_or(true) {
            log::info!("{:#?}", current_status);
            previous_status = Some(current_status);
        }
        thread::sleep(Duration::from_millis(500));
    }
}

pub trait LogResultExt<T> {
    fn log_ok(self, msg: &str) -> Option<T>;
}

impl<T, E: Display> LogResultExt<T> for std::result::Result<T, E> {
    fn log_ok(self, msg: &str) -> Option<T> {
        match self {
            Ok(val) => Some(val),
            Err(err) => {
                log::warn!("{}: {}", msg, err);
                None
            }
        }
    }
}

pub trait PeekExt<T> {
    fn peek(self, func: impl FnOnce(&T)) -> Self;
}

impl<T> PeekExt<T> for Option<T> {
    fn peek(self, func: impl FnOnce(&T)) -> Self {
        if let Some(inner) = self.as_ref() {
            func(inner);
        }
        self
    }
}