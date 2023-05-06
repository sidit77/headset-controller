use color_eyre::eyre::bail;
use color_eyre::Result;

#[derive(Debug, Clone)]
pub struct AudioManager;
impl AudioManager {
    pub fn new() -> Result<Self> {
        bail!("Not supported on this platform!")
    }

    pub fn devices(&self) -> impl Iterator<Item = AudioDevice> {
        std::iter::empty::<AudioDevice>()
    }

    pub fn get_default_device(&self) -> Option<AudioDevice> {
        None
    }

    pub fn set_default_device(&self, _: &AudioDevice) -> Result<()> {
        bail!("not supported!");
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AudioDevice;
impl AudioDevice {
    pub fn name(&self) -> &str {
        unimplemented!()
    }
}

pub struct AudioLoopback;

impl AudioLoopback {
    pub fn new(_: &AudioDevice, _: &AudioDevice) -> Result<Self> {
        bail!("not supported!")
    }
}
