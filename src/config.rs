use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use color_eyre::Result;
use directories_next::BaseDirs;
use once_cell::sync::Lazy;
use ron::ser::{to_string_pretty, PrettyConfig};
use serde::{Deserialize, Serialize};

use crate::util::EscapeStripper;

#[derive(Default, Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum OsAudio {
    #[default]
    Disabled,
    ChangeDefault {
        on_connect: String,
        on_disconnect: String
    },
    RouteAudio {
        src: String,
        dst: String
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum EqualizerConfig {
    Preset(u32),
    Custom(Vec<u8>)
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum CallAction {
    Nothing,
    ReduceVolume,
    Mute
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub side_tone: u8,
    pub volume_limiter: bool,
    pub microphone_volume: u8,
    pub equalizer: EqualizerConfig
}

impl Profile {
    pub(crate) fn new(name: String) -> Self {
        Self {
            name,
            side_tone: 0,
            volume_limiter: true,
            microphone_volume: 0,
            equalizer: EqualizerConfig::Preset(0)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadsetConfig {
    pub os_audio: OsAudio,
    pub mic_light: u8,
    pub bluetooth_call: CallAction,
    pub auto_enable_bluetooth: bool,
    pub inactive_time: u8,
    pub selected_profile_index: u32,
    pub profiles: Vec<Profile>
}

impl Default for HeadsetConfig {
    fn default() -> Self {
        Self {
            os_audio: Default::default(),
            mic_light: 0,
            bluetooth_call: CallAction::Nothing,
            auto_enable_bluetooth: false,
            inactive_time: 30,
            selected_profile_index: 0,
            profiles: vec![Profile::new(String::from("Default"))]
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    headsets: HashMap<String, HeadsetConfig>,
    pub auto_apply_changes: bool,
    pub preferred_device: Option<String>
}

impl Default for Config {
    fn default() -> Self {
        Self {
            headsets: HashMap::new(),
            auto_apply_changes: true,
            preferred_device: None
        }
    }
}

static BASE_PATH: Lazy<BaseDirs> = Lazy::new(|| BaseDirs::new().expect("can not get directories"));
static CONFIG_PATH: Lazy<PathBuf> = Lazy::new(|| BASE_PATH.config_dir().join("HeadsetController.ron"));
static LOG_PATH: Lazy<PathBuf> = Lazy::new(|| BASE_PATH.config_dir().join("HeadsetController.log"));

pub fn log_file() -> impl Write {
    let file = File::create(LOG_PATH.as_path()).expect("Can not open file");
    EscapeStripper::new(file)
}

impl Config {
    pub fn path() -> &'static Path {
        CONFIG_PATH.as_path()
    }

    pub fn load() -> Result<Self> {
        let config: Self = match Self::path().exists() {
            true => {
                let file = std::fs::read_to_string(Self::path())?;

                ron::from_str(&file)?
            }
            false => {
                let conf = Self::default();
                conf.save()?;
                conf
            }
        };
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let pretty = PrettyConfig::new();
        Ok(std::fs::write(Self::path(), to_string_pretty(self, pretty)?)?)
    }

    pub fn get_headset(&mut self, name: &str) -> &mut HeadsetConfig {
        if !self.headsets.contains_key(name) {
            self.headsets
                .insert(String::from(name), HeadsetConfig::default());
        }
        self.headsets
            .get_mut(name)
            .expect("Key should always exist")
    }
}

impl HeadsetConfig {
    pub fn selected_profile(&mut self) -> &mut Profile {
        if self.profiles.is_empty() {
            tracing::debug!("No profile creating a new one");
            self.profiles.push(Profile::new(String::from("Default")));
        }
        if self.selected_profile_index >= self.profiles.len() as u32 {
            tracing::debug!("profile index out of bounds");
            self.selected_profile_index = self.profiles.len() as u32 - 1;
        }
        &mut self.profiles[self.selected_profile_index as usize]
    }
}

pub static START_QUIET: Lazy<bool> = Lazy::new(|| std::env::args().any(|arg| arg.eq("--quiet")));
pub static CLOSE_IMMEDIATELY: Lazy<bool> = Lazy::new(|| std::env::args().any(|arg| arg.eq("--close-on-quit")));
pub static DUMMY_DEVICE: Lazy<bool> = Lazy::new(|| std::env::args().any(|arg| arg.eq("--dummy-device")));
pub static PRINT_UDEV_RULES: Lazy<bool> = Lazy::new(|| std::env::args().any(|arg| arg.eq("--print-udev-rules")));
