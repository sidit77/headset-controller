mod platform;

use std::fmt::{Debug, Formatter};
use hc_foundation::Result;

#[repr(transparent)]
pub struct AudioManager(platform::AudioManager);

#[derive(Clone, Eq, PartialEq)]
#[repr(transparent)]
pub struct AudioDevice(platform::AudioDevice);

pub struct AudioRedirection;

impl AudioManager {

    pub const fn switching_supported() -> bool {
        platform::AudioManager::switching_supported()
    }

    pub fn new() -> Result<Self> {
        platform::AudioManager::new()
            .map(Self)
    }

    pub fn devices(&self) -> impl Iterator<Item = AudioDevice> {
        self.0
            .devices()
            .map(AudioDevice)
    }

    pub fn find_device_by_name(&self, name: &str) -> Option<AudioDevice> {
        self.devices()
            .find(|dev| dev.name() == name)
    }

    pub fn get_default_device(&self) -> Option<AudioDevice> {
        self.0
            .get_default_device()
            .map(AudioDevice)
    }

    pub fn set_default_device(&self, device: &AudioDevice) -> Result<()> {
        assert!(Self::switching_supported());
        self.0
            .set_default_device(&device.0)
    }
}

impl Debug for AudioManager {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioManager").finish()
    }
}

impl AudioDevice {

    pub fn name(&self) -> &str {
        self.0.name()
    }

}

impl Debug for AudioDevice {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioDevice")
            .field("name", &self.name())
            .finish()
    }
}

impl AudioRedirection {

    pub const fn is_supported() -> bool {
        false
    }

    pub fn new(_src: &AudioDevice, _dst: &AudioDevice) -> Result<Self> {
        unimplemented!()
    }

}

impl Debug for AudioRedirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioRedirection").finish()
    }
}