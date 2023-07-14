use std::collections::HashMap;
use std::time::{Duration, Instant};

use color_eyre::eyre::eyre;
use tokio::task::JoinHandle;
use tracing::instrument;
use async_hid::{Device as HidDevice, DeviceInfo};
use tokio::spawn;
use tokio::sync::mpsc::UnboundedSender;

use crate::config::CallAction;
use crate::devices::{BatteryLevel, BluetoothConfig, BoxedDevice, BoxedDeviceFuture, ChatMix, Device, DeviceResult as Result, DeviceResult, DeviceUpdate, Equalizer, InactiveTime, Interface, InterfaceMap, MicrophoneLight, MicrophoneVolume, SideTone, SupportedDevice, UpdateChannel, VolumeLimiter};

const VID_STEELSERIES: u16 = 0x1038;

const PID_ARCTIS_NOVA_7: u16 = 0x2202;
const PID_ARCTIS_NOVA_7X: u16 = 0x2206;
const PID_ARCTIS_NOVA_7P: u16 = 0x220a;

const USAGE_ID: u16 = 0x1;
const NOTIFICATION_USAGE_PAGE: u16 = 0xFF00;
const CONFIGURATION_USAGE_PAGE: u16 = 0xFFC0;

const STATUS_BUF_SIZE: usize = 8;
const READ_TIMEOUT: i32 = 500;

const HEADSET_OFFLINE: u8 = 0x00;
const HEADSET_CHARGING: u8 = 0x01;

const BATTERY_MAX: u8 = 0x04;
const BATTERY_MIN: u8 = 0x00;

pub const ARCTIS_NOVA_7X: SupportedDevice = SupportedDevice {
    name: "Steelseries Arctis Nova 7X",
    required_interfaces: &[
        Interface::new(NOTIFICATION_USAGE_PAGE , USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7X),
        Interface::new(CONFIGURATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7X)
    ],
    open: ArctisNova7::open_xbox,
};

pub struct ArctisNova7 {
    update_task: JoinHandle<()>,
    config_channel: HidDevice,
    name: &'static str,
    connected: bool
}

impl ArctisNova7 {

    async fn open(name: &'static str, pid: u16, update_channel: UpdateChannel, interfaces: &InterfaceMap) -> DeviceResult<BoxedDevice> {
        let notification_channel = interfaces
            .get(&Interface::new(NOTIFICATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, pid))
            .unwrap()
            .open()
            .await?;

        let update_task = {
            let update_channel = update_channel.clone();
            spawn(async move {
                let mut buf = [0u8; 8];
                loop {
                    notification_channel.read_input_report(&mut buf)
                        .await
                        .unwrap_or_else(|err| {
                            println!("notification task: {}", err);
                            0
                        });
                    update_channel.send(DeviceUpdate::ConnectionStatusChanged).unwrap();
                }
            })
        };

        let config_channel = interfaces
            .get(&Interface::new(CONFIGURATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, pid))
            .unwrap()
            .open()
            .await?;

        Ok(Box::new(Self {
            update_task,
            config_channel,
            name,
            connected: false,
        }))
    }

    pub fn open_xbox(update_channel: UpdateChannel, interfaces: &InterfaceMap) -> BoxedDeviceFuture {
        Box::pin(Self::open(ARCTIS_NOVA_7X.name, PID_ARCTIS_NOVA_7X, update_channel, interfaces))
    }

}

impl Drop for ArctisNova7 {
    fn drop(&mut self) {
        self.update_task.abort();
        println!("Drop");
    }
}

impl Device for ArctisNova7 {

    fn name(&self) -> &str {
        self.name
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

}


/*
pub struct ArcticsNova7 {
    device: HidDevice,
    name: Info,
    last_chat_mix_adjustment: Option<Instant>,
    connected: bool,
    battery: BatteryLevel,
    chat_mix: ChatMix
}

impl From<(HidDevice, Info)> for ArcticsNova7 {
    fn from((device, info): (HidDevice, Info)) -> Self {
        Self {
            device,
            name: info,
            last_chat_mix_adjustment: None,
            connected: false,
            battery: BatteryLevel::Unknown,
            chat_mix: Default::default()
        }
    }
}

impl ArcticsNova7 {
    pub const SUPPORT: CheckSupport = |info| {
        let supported = SUPPORTED_VENDORS.contains(&info.vendor_id())
            && SUPPORTED_PRODUCTS.contains(&info.product_id())
            && REQUIRED_INTERFACE == info.interface_number();
        if supported {
            Some(Box::new(GenericHidDevice::<ArcticsNova7>::new(info, "SteelSeries", "Arctis Nova 7")))
        } else {
            None
        }
    };
}

impl Device for ArcticsNova7 {
    fn get_info(&self) -> &Info {
        &self.name
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    #[instrument(skip(self))]
    fn poll(&mut self) -> Result<Duration> {
        let mut report = [0u8; STATUS_BUF_SIZE];
        self.device.write(&[0x00, 0xb0])?;
        if self.device.read_timeout(&mut report, READ_TIMEOUT)? != STATUS_BUF_SIZE {
            return Err(eyre!("Cannot read enough bytes"));
        }

        let prev_chat_mix = self.chat_mix;
        self.chat_mix = ChatMix {
            game: report[4],
            chat: report[5]
        };
        match report[3] {
            HEADSET_OFFLINE => {
                self.connected = false;
                self.battery = BatteryLevel::Unknown;
                self.chat_mix = ChatMix::default();
            }
            HEADSET_CHARGING => {
                self.connected = true;
                self.battery = BatteryLevel::Charging;
            }
            _ => {
                self.connected = true;
                self.battery = BatteryLevel::Level({
                    let level = report[2].clamp(BATTERY_MIN, BATTERY_MAX);
                    (level - BATTERY_MIN) * (100 / (BATTERY_MAX - BATTERY_MIN))
                });
            }
        }
        if self.chat_mix != prev_chat_mix {
            if self.last_chat_mix_adjustment.is_none() {
                tracing::trace!("Increase polling rate");
            }
            self.last_chat_mix_adjustment = Some(Instant::now());
        }
        if self
            .last_chat_mix_adjustment
            .map(|i| i.elapsed() > Duration::from_secs(1))
            .unwrap_or(false)
        {
            self.last_chat_mix_adjustment = None;
            tracing::trace!("Decrease polling rate");
        }

        Ok(match self.connected {
            true => match self.last_chat_mix_adjustment.is_some() {
                true => Duration::from_millis(250),
                false => Duration::from_millis(1000)
            },
            false => Duration::from_secs(4)
        })
    }

    fn get_battery_status(&self) -> Option<BatteryLevel> {
        Some(self.battery)
    }
    fn get_chat_mix(&self) -> Option<ChatMix> {
        Some(self.chat_mix)
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

impl SideTone for ArcticsNova7 {
    fn levels(&self) -> u8 {
        4
    }

    #[instrument(skip(self))]
    fn set_level(&self, level: u8) -> Result<()> {
        tracing::debug!("Attempting to write new value to device!");
        assert!(level < SideTone::levels(self));
        self.device.write(&[0x00, 0x39, level])?;
        Ok(())
    }
}

impl MicrophoneVolume for ArcticsNova7 {
    fn levels(&self) -> u8 {
        8
    }

    #[instrument(skip(self))]
    fn set_level(&self, level: u8) -> Result<()> {
        tracing::debug!("Attempting to write new value to device!");
        assert!(level < MicrophoneVolume::levels(self));
        self.device.write(&[0x00, 0x37, level])?;
        Ok(())
    }
}

impl VolumeLimiter for ArcticsNova7 {
    #[instrument(skip(self))]
    fn set_enabled(&self, enabled: bool) -> Result<()> {
        tracing::debug!("Attempting to write new value to device!");
        self.device.write(&[0x00, 0x3a, u8::from(enabled)])?;
        Ok(())
    }
}

impl Equalizer for ArcticsNova7 {
    fn bands(&self) -> u8 {
        10
    }

    fn base_level(&self) -> u8 {
        0x14
    }

    fn variance(&self) -> u8 {
        0x14
    }

    fn presets(&self) -> &[(&str, &[u8])] {
        &[
            ("Flat", &[0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14, 0x14]),
            ("Bass", &[0x1b, 0x1f, 0x1c, 0x16, 0x11, 0x11, 0x12, 0x12, 0x12, 0x12]),
            ("Focus", &[0x0a, 0x0d, 0x12, 0x0d, 0x0f, 0x1c, 0x20, 0x1b, 0x0d, 0x14]),
            ("Smiley", &[0x1a, 0x1b, 0x17, 0x11, 0x0c, 0x0c, 0x0f, 0x17, 0x1a, 0x1c])
        ]
    }

    #[instrument(skip(self))]
    fn set_levels(&self, levels: &[u8]) -> Result<()> {
        tracing::debug!("Attempting to write new value to device!");
        assert_eq!(levels.len(), Equalizer::bands(self) as usize);
        assert!(
            levels
                .iter()
                .all(|i| *i >= self.base_level() - self.variance() && *i <= self.base_level() + self.variance())
        );
        let mut msg = [0u8; 13];
        msg[1] = 0x33;
        msg[2..12].copy_from_slice(levels);
        self.device.write(&msg)?;
        Ok(())
    }
}

impl BluetoothConfig for ArcticsNova7 {
    #[instrument(skip(self))]
    fn set_call_action(&self, action: CallAction) -> Result<()> {
        tracing::debug!("Attempting to write new value to device!");
        let v = match action {
            CallAction::Nothing => 0x00,
            CallAction::ReduceVolume => 0x01,
            CallAction::Mute => 0x02
        };
        self.device.write(&[0x00, 0xb3, v])?;
        Ok(())
    }

    #[instrument(skip(self))]
    fn set_auto_enabled(&self, enabled: bool) -> Result<()> {
        tracing::debug!("Attempting to write new value to device!");
        self.device.write(&[0x00, 0xb2, u8::from(enabled)])?;
        Ok(())
    }
}

impl MicrophoneLight for ArcticsNova7 {
    fn levels(&self) -> u8 {
        4
    }

    #[instrument(skip(self))]
    fn set_light_strength(&self, level: u8) -> Result<()> {
        assert!(level < MicrophoneLight::levels(self));
        tracing::debug!("Attempting to write new value to device!");
        self.device.write(&[0x00, 0xae, level])?;
        Ok(())
    }
}

impl InactiveTime for ArcticsNova7 {
    #[instrument(skip(self))]
    fn set_inactive_time(&self, minutes: u8) -> Result<()> {
        assert!(minutes > 0);
        tracing::debug!("Attempting to write new value to device!");
        //This should be correct, but I'm honestly to scared to test it
        //self.device.write(&[0x00, 0xa3, minutes])?;
        Ok(())
    }
}
*/