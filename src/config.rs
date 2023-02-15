use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use anyhow::Result;
use directories_next::BaseDirs;

#[derive(Default, Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum OutputSwitch {
    #[default]
    Disabled,
    Enabled {
        on_connect: String,
        on_disconnect: String
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub side_tone: Option<u8>
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct HeadsetConfig {
    pub switch_output: OutputSwitch,
    selected_profile: String,
    profiles: HashMap<String, Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    headsets: HashMap<String, HeadsetConfig>
}

impl Default for Config {
    fn default() -> Self {
        Self {
            headsets: HashMap::new(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        let dirs = BaseDirs::new().expect("can not get directories");
        let config_dir = dirs.config_dir();
        config_dir.join("ArctisController.ron")
    }

    pub fn load() -> Result<Self> {
        let config: Self = match Self::path().exists() {
            true => {
                let file = std::fs::read_to_string(Self::path())?;
                let conf = ron::from_str(&file)?;
                conf
            },
            false => {
                let conf = Self::default();
                conf.save()?;
                conf
            }
        };
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        Ok(std::fs::write(Self::path(), ron::to_string(self)?)?)
    }

    pub fn get_headset(&mut self, name: &str) -> &mut HeadsetConfig {
        if !self.headsets.contains_key(name) {
            self.headsets.insert(String::from(name), HeadsetConfig::default());
        }
        self.headsets.get_mut(name)
            .expect("Key should always exist")
    }

}

impl HeadsetConfig {

    pub fn profile(&mut self) -> &mut Profile {
        if self.profiles.is_empty() {
            self.profiles.insert(String::from("Default"), Profile::default());
        }
        if !self.profiles.contains_key(&self.selected_profile){
            self.selected_profile = self.profiles
                .iter()
                .map(|(k, _)| k.clone())
                .next()
                .expect("At least the default profile should always exist")
        }
        self.profiles.get_mut(&self.selected_profile)
            .expect("Should always be valid")
    }

}