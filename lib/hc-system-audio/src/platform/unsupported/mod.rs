use std::iter::empty;
use hc_foundation::Result;

pub struct AudioManager;

#[derive(Clone, Eq, PartialEq)]
pub struct AudioDevice;

impl AudioManager {

    pub const fn switching_supported() -> bool {
        false
    }

    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub fn devices(&self) -> impl Iterator<Item = AudioDevice> {
        empty()
    }

    pub fn get_default_device(&self) -> Option<AudioDevice> {
        None
    }

    pub fn set_default_device(&self, _: &AudioDevice) -> Result<()> {
        unimplemented!()
    }
}

impl AudioDevice {

    pub fn name(&self) -> &str {
        unimplemented!()
    }

}