use std::sync::Arc;
use std::time::Duration;
use async_hid::{AccessMode, HidResult};
use crossbeam_utils::atomic::AtomicCell;
use static_assertions::const_assert;
use async_hid::Device as HidDevice;
use either::Either;

use flume::{Receiver, RecvError, unbounded};
use hc_foundation::{Task, Timer};

use crate::devices::*;
use crate::util::{AtomicCellExt, select, SenderExt, VecExt};

const VID_STEELSERIES: u16 = 0x1038;

const PID_ARCTIS_NOVA_7: u16 = 0x2202;
const PID_ARCTIS_NOVA_7X: u16 = 0x2206;
const PID_ARCTIS_NOVA_7P: u16 = 0x220a;

const USAGE_ID: u16 = 0x1;
const NOTIFICATION_USAGE_PAGE: u16 = 0xFF00;
const CONFIGURATION_USAGE_PAGE: u16 = 0xFFC0;

pub const ARCTIS_NOVA_7_INTERFACES: &[Interface] = &[
    Interface::new(NOTIFICATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7),
    Interface::new(CONFIGURATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7)
];
pub const ARCTIS_NOVA_7X_INTERFACES: &[Interface] = &[
    Interface::new(NOTIFICATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7X),
    Interface::new(CONFIGURATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7X)
];
pub const ARCTIS_NOVA_7P_INTERFACES: &[Interface] = &[
    Interface::new(NOTIFICATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7P),
    Interface::new(CONFIGURATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, PID_ARCTIS_NOVA_7P)
];

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Subtype {
    Pc,
    Xbox,
    Playstation
}

impl Subtype {
    const fn pid(self) -> u16 {
        match self {
            Subtype::Pc => PID_ARCTIS_NOVA_7,
            Subtype::Xbox => PID_ARCTIS_NOVA_7X,
            Subtype::Playstation => PID_ARCTIS_NOVA_7P,
        }
    }
}

impl From<Subtype> for SupportedDevice {
    fn from(value: Subtype) -> Self {
        match value {
            Subtype::Pc => SupportedDevice::ArctisNova7,
            Subtype::Xbox => SupportedDevice::ArctisNova7X,
            Subtype::Playstation => SupportedDevice::ArctisNova7P
        }
    }
}

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

const_assert!(AtomicCell::<State>::is_lock_free());
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
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
    _update_task: Task<()>,
    _config_task: Task<()>,
    subtype: Subtype,
    config_channel: Sender<ConfigAction>,
    state: Arc<AtomicCell<State>>
}

impl ArctisNova7 {
    pub async fn open(subtype: Subtype, executor: &LocalExecutor<'_>, update_channel: UpdateChannel, interfaces: &InterfaceMap) -> DeviceResult<ArctisNova7> {
        let config_interface = interfaces
            .get(&Interface::new(CONFIGURATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, subtype.pid()))
            .expect("Failed to find interface in map")
            .open(AccessMode::ReadWrite)
            .await?;

        let state = Arc::new(AtomicCell::new(load_state(&config_interface).await?));

        let notification_interface = interfaces
            .get(&Interface::new(NOTIFICATION_USAGE_PAGE, USAGE_ID, VID_STEELSERIES, subtype.pid()))
            .expect("Failed to find interface in map")
            .open(AccessMode::Read)
            .await?;

        let (config_channel, command_receiver) = unbounded();
        let config_task = executor.spawn(configuration_handler(config_interface, update_channel.clone(), command_receiver));
        let update_task = executor.spawn(update_handler(notification_interface, update_channel.clone(), state.clone()));

        Ok(Self {
            _update_task: update_task,
            _config_task: config_task,
            subtype,
            config_channel,
            state
        })
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
    let buffer = &buffer[..size];

    state.power_state = PowerState::from_u8(buffer[3]);
    state.battery = (state.power_state == PowerState::Discharging)
        .then(|| normalize_battery_level(buffer[2]))
        .unwrap_or_default();
    state.chat_mix = (state.power_state != PowerState::Offline)
        .then_some(ChatMix {
            game: buffer[4],
            chat: buffer[5]
        })
        .unwrap_or_default();

    Ok(state)
}

#[instrument(skip_all)]
async fn configuration_handler(config_interface: HidDevice, events: UpdateChannel, config_requests: Receiver<ConfigAction>) {
    let mut config_interface = MaybeHidDevice::from(config_interface);
    loop {
        let timer = config_interface
            .is_connected()
            .then_some(Duration::from_secs(20))
            .map_or_else(Timer::never, Timer::after);

        match select(config_requests.recv_async(), timer).await {
            Either::Left(Ok(request)) => {
                tracing::debug!("Attempting apply config request: {:?}", request);
                let data = match request {
                    ConfigAction::SetSideTone(level) => vec![0x00, 0x39, level],
                    ConfigAction::SetMicrophoneVolume(level) => vec![0x00, 0x37, level],
                    ConfigAction::EnableVolumeLimiter(enabled) => vec![0x00, 0x3a, u8::from(enabled)],
                    ConfigAction::SetEqualizerLevels(mut levels) => {
                        levels.prepend([0x00, 0x33]);
                        levels
                    }
                    ConfigAction::SetBluetoothCallAction(action) => {
                        let v = match action {
                            CallAction::Nothing => 0x00,
                            CallAction::ReduceVolume => 0x01,
                            CallAction::Mute => 0x02
                        };
                        vec![0x00, 0xb3, v]
                    }
                    ConfigAction::EnableAutoBluetoothActivation(enabled) => vec![0x00, 0xb2, u8::from(enabled)],
                    ConfigAction::SetMicrophoneLightStrength(level) => vec![0x00, 0xae, level],
                    ConfigAction::SetInactiveTime(minutes) => vec![0x00, 0xa3, minutes]
                };
                match config_interface.connected(AccessMode::Write).await {
                    Ok(device) => device
                        .write_output_report(&data)
                        .await
                        .unwrap_or_else(|err| events.send_log(DeviceUpdate::DeviceError(err))),
                    Err(err) => events.send_log(DeviceUpdate::DeviceError(err))
                }
            },
            Either::Left(Err(RecvError::Disconnected)) => break,
            Either::Right(_) => config_interface.disconnect(),
        }
    }
    tracing::warn!("Request channel close unexpectedly");
}

#[instrument(skip_all)]
async fn update_handler(notification_interface: HidDevice, events: UpdateChannel, state: Arc<AtomicCell<State>>) {
    let mut buf = [0u8; STATUS_BUF_SIZE];
    loop {
        match notification_interface.read_input_report(&mut buf).await {
            Ok(size) => {
                let buf = &buf[..size];
                //debug_assert_eq!(size, buf.len());
                if let Some(update) = parse_status_update(buf) {
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

impl Device for ArctisNova7 {
    fn name(&self) -> &'static str {
        SupportedDevice::from(self.subtype).name()
    }

    fn product_name(&self) -> &'static str {
        match self.subtype {
            Subtype::Pc => "Arctis Nova 7",
            Subtype::Xbox => "Arctis Nova 7X",
            Subtype::Playstation => "Arctis Nova 7P",
        }
    }

    fn manufacturer_name(&self) -> &'static str {
        "Steelseries"
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

impl SideTone for ArctisNova7 {
    fn levels(&self) -> u8 {
        4
    }

    fn set_level(&self, level: u8) {
        assert!(level < SideTone::levels(self));
        self.request_config_action(ConfigAction::SetSideTone(level));
    }
}

impl MicrophoneVolume for ArctisNova7 {
    fn levels(&self) -> u8 {
        8
    }

    fn set_level(&self, level: u8) {
        assert!(level < MicrophoneVolume::levels(self));
        self.request_config_action(ConfigAction::SetMicrophoneVolume(level))
    }
}

impl VolumeLimiter for ArctisNova7 {
    fn set_enabled(&self, enabled: bool) {
        self.request_config_action(ConfigAction::EnableVolumeLimiter(enabled));
    }
}

impl Equalizer for ArctisNova7 {
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

    fn set_levels(&self, levels: &[u8]) {
        assert_eq!(levels.len(), Equalizer::bands(self) as usize);
        assert!(
            levels
                .iter()
                .all(|i| *i >= self.base_level() - self.variance() && *i <= self.base_level() + self.variance())
        );
        self.request_config_action(ConfigAction::SetEqualizerLevels(levels.to_vec()));
    }
}

impl BluetoothConfig for ArctisNova7 {
    fn set_call_action(&self, action: CallAction) {
        self.request_config_action(ConfigAction::SetBluetoothCallAction(action));
    }

    fn set_auto_enabled(&self, enabled: bool) {
        self.request_config_action(ConfigAction::EnableAutoBluetoothActivation(enabled));
    }
}

impl MicrophoneLight for ArctisNova7 {
    fn levels(&self) -> u8 {
        4
    }

    fn set_light_strength(&self, level: u8) {
        assert!(level < MicrophoneLight::levels(self));
        self.request_config_action(ConfigAction::SetMicrophoneLightStrength(level));
    }
}

impl InactiveTime for ArctisNova7 {
    fn set_inactive_time(&self, minutes: u8) {
        assert!(minutes > 0);
        //This should be correct, but I'm honestly to scared to test it
        //self.request_config_action(ConfigAction::SetInactiveTime(minutes));
        let _ = ConfigAction::SetInactiveTime(minutes);
    }
}

enum MaybeHidDevice {
    Connected(HidDevice),
    Disconnected(DeviceInfo)
}

impl From<HidDevice> for MaybeHidDevice {
    fn from(value: HidDevice) -> Self {
        Self::Connected(value)
    }
}

impl MaybeHidDevice {
    fn is_connected(&self) -> bool {
        matches!(self, MaybeHidDevice::Connected(_))
    }

    fn disconnect(&mut self) {
        if let MaybeHidDevice::Connected(device) = self {
            let info = device.info().clone();
            *self = MaybeHidDevice::Disconnected(info);
            tracing::debug!("Disconnecting from the device");
        }
    }

    async fn connected(&mut self, mode: AccessMode) -> HidResult<&HidDevice> {
        match self {
            MaybeHidDevice::Connected(device) => Ok(device),
            MaybeHidDevice::Disconnected(info) => {
                tracing::debug!("Reconnecting to the device");
                let device = info.open(mode).await?;
                *self = MaybeHidDevice::Connected(device);
                match self {
                    MaybeHidDevice::Connected(device) => Ok(device),
                    MaybeHidDevice::Disconnected(_) => unreachable!()
                }
            }
        }
    }
}
