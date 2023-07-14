use std::sync::Arc;

use async_hid::Device as HidDevice;
use crossbeam_utils::atomic::AtomicCell;
use tokio::spawn;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use tracing::instrument;

use crate::devices::{BatteryLevel, BoxedDevice, BoxedDeviceFuture, ChatMix, ConfigAction, Device, DeviceResult, DeviceStrings, DeviceUpdate, Interface, InterfaceMap, SideTone, SupportedDevice, UpdateChannel};
use crate::util::{AtomicCellExt, SenderExt};

const VID_STEELSERIES: u16 = 0x1038;

const PID_ARCTIS_NOVA_7: u16 = 0x2202;
const PID_ARCTIS_NOVA_7X: u16 = 0x2206;
const PID_ARCTIS_NOVA_7P: u16 = 0x220a;

const USAGE_ID: u16 = 0x1;
const NOTIFICATION_USAGE_PAGE: u16 = 0xFF00;
const CONFIGURATION_USAGE_PAGE: u16 = 0xFFC0;

pub const ARCTIS_NOVA_7: SupportedDevice = SupportedDevice {
    strings: DeviceStrings::new("Steelseries Arctis Nova 7", "Steelseries", "Arctis Nova 7"),
    required_interfaces: &[
        Interface::new(NOTIFICATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7),
        Interface::new(CONFIGURATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7)
    ],
    open: ArctisNova7::open_pc
};

pub const ARCTIS_NOVA_7X: SupportedDevice = SupportedDevice {
    strings: DeviceStrings::new("Steelseries Arctis Nova 7X", "Steelseries", "Arctis Nova 7X"),
    required_interfaces: &[
        Interface::new(NOTIFICATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7X),
        Interface::new(CONFIGURATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7X)
    ],
    open: ArctisNova7::open_xbox
};

pub const ARCTIS_NOVA_7P: SupportedDevice = SupportedDevice {
    strings: DeviceStrings::new("Steelseries Arctis Nova 7P", "Steelseries", "Arctis Nova 7P"),
    required_interfaces: &[
        Interface::new(NOTIFICATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7P),
        Interface::new(CONFIGURATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7P)
    ],
    open: ArctisNova7::open_playstation
};

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
enum PowerState {
    #[default]
    Offline,
    Charging,
    Discharging
}

impl PowerState {
    fn from_u8(byte: u8) -> Self {
        match byte {
            0x0 => Self::Offline,
            0x1 => Self::Charging,
            0x3 => Self::Discharging,
            _ => Self::default()
        }
    }
}

#[derive(Default, Copy, Clone, Eq, PartialEq)]
#[repr(align(8))] //So that AtomicCell<State> becomes lock-free
struct State {
    power_state: PowerState,
    battery: u8,
    chat_mix: ChatMix
}

impl State {
    fn is_connected(self) -> bool {
        self.power_state != PowerState::Offline
    }
    fn battery(self) -> BatteryLevel {
        match self.power_state {
            PowerState::Offline => BatteryLevel::Unknown,
            PowerState::Charging => BatteryLevel::Charging,
            PowerState::Discharging => BatteryLevel::Level(self.battery)
        }
    }
}

pub struct ArctisNova7 {
    pub strings: DeviceStrings,
    update_task: JoinHandle<()>,
    config_task: JoinHandle<()>,
    config_channel: UnboundedSender<ConfigAction>,
    state: Arc<AtomicCell<State>>
}

impl ArctisNova7 {
    async fn open(strings: DeviceStrings, pid: u16, update_channel: UpdateChannel, interfaces: &InterfaceMap) -> DeviceResult<BoxedDevice> {
        debug_assert!(AtomicCell::<State>::is_lock_free());
        let config_interface = interfaces
            .get(&Interface::new(CONFIGURATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, pid))
            .expect("Failed to find interface in map")
            .open()
            .await?;

        let state = Arc::new(AtomicCell::new(load_state(&config_interface).await?));

        //TODO open as read-only
        let notification_interface = interfaces
            .get(&Interface::new(NOTIFICATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, pid))
            .expect("Failed to find interface in map")
            .open()
            .await?;

        let (config_channel, command_receiver) = unbounded_channel();
        let config_task = spawn(configuration_handler(config_interface, update_channel.clone(), command_receiver));
        let update_task = spawn(update_handler(notification_interface, update_channel.clone(), state.clone()));

        Ok(Box::new(Self {
            update_task,
            config_task,
            config_channel,
            strings,
            state
        }))
    }

    pub fn open_xbox(update_channel: UpdateChannel, interfaces: &InterfaceMap) -> BoxedDeviceFuture {
        Box::pin(Self::open(ARCTIS_NOVA_7X.strings, PID_ARCTIS_NOVA_7X, update_channel, interfaces))
    }

    pub fn open_playstation(update_channel: UpdateChannel, interfaces: &InterfaceMap) -> BoxedDeviceFuture {
        Box::pin(Self::open(ARCTIS_NOVA_7P.strings, PID_ARCTIS_NOVA_7P, update_channel, interfaces))
    }

    pub fn open_pc(update_channel: UpdateChannel, interfaces: &InterfaceMap) -> BoxedDeviceFuture {
        Box::pin(Self::open(ARCTIS_NOVA_7.strings, PID_ARCTIS_NOVA_7, update_channel, interfaces))
    }

    fn request_config_action(&self, action: ConfigAction) {
        self.config_channel
            .send(action)
            .unwrap_or_else(|_| tracing::warn!("config channel close unexpectedly"))
    }
}

const STATUS_BUF_SIZE: usize = 8;

#[instrument(skip_all)]
async fn load_state(config_interface: &HidDevice) -> DeviceResult<State> {
    let mut state = State::default();
    config_interface.write_output_report(&[0x0, 0xb0]).await?;
    let mut buffer = [0u8; STATUS_BUF_SIZE];
    //TODO add a timeout
    let size = config_interface.read_input_report(&mut buffer).await?;
    debug_assert_eq!(size, buffer.len());

    state.power_state = PowerState::from_u8(buffer[4]);
    state.battery = (state.power_state == PowerState::Discharging)
        .then(|| normalize_battery_level(buffer[3]))
        .unwrap_or_default();
    state.chat_mix = (state.power_state != PowerState::Offline)
        .then_some(ChatMix {
            game: buffer[5],
            chat: buffer[6]
        })
        .unwrap_or_default();

    Ok(state)
}

#[instrument(skip_all)]
async fn configuration_handler(config_interface: HidDevice, events: UpdateChannel, mut config_requests: UnboundedReceiver<ConfigAction>) {
    while let Some(request) = config_requests.recv().await {
        tracing::debug!("Attempting apply config request: {:?}", request);
        let result = match request {
            ConfigAction::SetSideTone(level) => {
                config_interface.write_output_report(&[0x00, 0x39, level]).await
            }
        };
        result.unwrap_or_else(|err| events.send_log(DeviceUpdate::DeviceError(err)));
    }
    tracing::warn!("Request channel close unexpectedly");
}

#[instrument(skip_all)]
async fn update_handler(notification_interface: HidDevice, events: UpdateChannel, state: Arc<AtomicCell<State>>) {
    let mut buf = [0u8; STATUS_BUF_SIZE];
    loop {
        match notification_interface.read_input_report(&mut buf).await {
            Ok(size) => {
                debug_assert_eq!(size, buf.len());
                if let Some(update) = parse_status_update(&buf[1..]) {
                    state.update(|state| match update {
                        StatusUpdate::PowerState(ps) => state.power_state = ps,
                        StatusUpdate::Battery(level) => state.battery = level,
                        StatusUpdate::ChatMix(mix) => state.chat_mix = mix
                    });
                    events.send_log(DeviceUpdate::from(update));
                }
            }
            Err(err) => events.send_log(DeviceUpdate::DeviceError(err))
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum StatusUpdate {
    PowerState(PowerState),
    Battery(u8),
    ChatMix(ChatMix)
}

impl From<StatusUpdate> for DeviceUpdate {
    fn from(value: StatusUpdate) -> Self {
        //This mapping is not fully correct but it's good enough
        match value {
            StatusUpdate::PowerState(_) => Self::ConnectionChanged,
            StatusUpdate::Battery(_) => Self::BatteryLevel,
            StatusUpdate::ChatMix(_) => Self::ChatMixChanged
        }
    }
}

fn parse_status_update(data: &[u8]) -> Option<StatusUpdate> {
    const POWER_STATE_CHANGED: u8 = 0xbb;
    const BATTERY_LEVEL_CHANGED: u8 = 0xb7;
    const CHAT_MIX_CHANGED: u8 = 0x45;
    match data[0] {
        CHAT_MIX_CHANGED => Some(StatusUpdate::ChatMix(ChatMix {
            game: data[1],
            chat: data[2]
        })),
        POWER_STATE_CHANGED => Some(StatusUpdate::PowerState(PowerState::from_u8(data[1]))),
        BATTERY_LEVEL_CHANGED => Some(StatusUpdate::Battery(normalize_battery_level(data[1]))),
        _ => None
    }
}

fn normalize_battery_level(byte: u8) -> u8 {
    const BATTERY_MAX: u8 = 0x04;
    const BATTERY_MIN: u8 = 0x00;
    let level = byte.clamp(BATTERY_MIN, BATTERY_MAX);
    (level - BATTERY_MIN) * (100 / (BATTERY_MAX - BATTERY_MIN))
}

impl Drop for ArctisNova7 {
    fn drop(&mut self) {
        tracing::trace!("Stopping background tasks for {}", self.name());
        self.update_task.abort();
        self.config_task.abort();
    }
}

impl Device for ArctisNova7 {
    fn strings(&self) -> DeviceStrings {
        self.strings
    }

    fn is_connected(&self) -> bool {
        self.state.load().is_connected()
    }

    fn get_battery_status(&self) -> Option<BatteryLevel> {
        Some(self.state.load().battery())
    }

    fn get_chat_mix(&self) -> Option<ChatMix> {
        Some(self.state.load().chat_mix)
    }

    fn get_side_tone(&self) -> Option<&dyn SideTone> {
        Some(self)
    }
}

impl SideTone for ArctisNova7 {
    fn levels(&self) -> u8 {
        4
    }

    fn set_level(&self, level: u8) -> DeviceResult<()> {
        assert!(level < SideTone::levels(self));
        self.request_config_action(ConfigAction::SetSideTone(level));
        Ok(())
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
