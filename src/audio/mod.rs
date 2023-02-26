use anyhow::Result;

#[cfg(target_os = "windows")]
#[path = "platforms/windows.rs"]
mod platform;

#[cfg(not(target_os = "windows"))]
compile_error!("unsupported right now");

pub use platform::{AudioLoopback, AudioDevice, AudioManager};
use crate::config::OsAudio;
use crate::util::LogResultExt;

pub struct AudioSystem {
    manager: Result<AudioManager>,
    devices: Vec<AudioDevice>,
    default_device: Option<AudioDevice>,
    loopback: Option<AudioLoopback>
}

impl AudioSystem {

    pub fn new() -> Self {
        let manager = AudioManager::new();
        let mut result = Self {
            manager,
            devices: Vec::new(),
            default_device: None,
            loopback: None,
        };
        result.refresh_devices();
        result
    }

    //pub fn is_running(&self) -> bool {
    //    self.manager.is_ok()
    //}

    pub fn refresh_devices(&mut self) {
        if let Ok(manager) = &self.manager {
            self.devices.clear();
            self.devices.extend(manager.devices());
            self.default_device = manager.get_default_device();
        }
    }

    pub fn devices(&self) -> &Vec<AudioDevice> {
        &self.devices
    }

    pub fn default_device(&self) -> Option<&AudioDevice> {
        self.default_device.as_ref()
    }

    pub fn apply(&mut self, audio_config: &OsAudio, connected: bool) {
        self.refresh_devices();
        if let Ok(manager) = &self.manager {
            match audio_config {
                OsAudio::Disabled => {}
                OsAudio::ChangeDefault { on_connect, on_disconnect } => {
                    let target = match connected {
                        true => on_connect,
                        false => on_disconnect
                    };
                    if let Some(device) = self
                        .devices()
                        .iter()
                        .find(|dev| dev.name() == target) {
                        match self
                            .default_device()
                            .map_or(false, |dev|dev == device) {
                            true => log::info!("Device \"{}\" is already active", device.name()),
                            false => {
                                manager.set_default_device(device)
                                    .log_ok("Could not change default audio device");
                                self.default_device = manager.get_default_device();
                            }
                        }
                    }
                }
                OsAudio::RouteAudio { src, dst } => {
                    if connected {
                        self.loopback = None;
                    } else {
                        let src = self
                            .devices()
                            .iter()
                            .find(|dev| dev.name() == src);
                        let dst = self
                            .devices()
                            .iter()
                            .find(|dev| dev.name() == dst);
                        match (src, dst) {
                            (Some(src), Some(dst)) => {
                                self.loopback = AudioLoopback::new(src, dst)
                                    .log_ok("Could not start audio routing");
                            }
                            _ => log::warn!("Could not find both audio devices")
                        }
                    }
                }
            }
        }
    }

}