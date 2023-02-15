use std::iter::FusedIterator;
use anyhow::{Context, Result};
use com_policy_config::{IPolicyConfig, PolicyConfigClient};
use windows::core::PCWSTR;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{DEVICE_STATE_ACTIVE, eConsole, eRender, IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize, STGM_READ};
use crate::util::CopySlice;

#[derive(Debug, Clone)]
pub struct AudioManager {
    enumerator: IMMDeviceEnumerator,
    policy_config: IPolicyConfig
}

impl AudioManager {

    pub fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)?;

            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let policy_config: IPolicyConfig = CoCreateInstance(&PolicyConfigClient, None, CLSCTX_ALL)?;

            /*
            println!("All Devices:");
            let device_collection = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            for i in 0..device_collection.GetCount()? {
                let device = device_collection.Item(i)?;
                let property_store = device.OpenPropertyStore(STGM_READ)?;
                let name = property_store.GetValue(&PKEY_Device_FriendlyName)?;
                println!("  {}", name.Anonymous.Anonymous.Anonymous.pwszVal.display());
            }

            {
                let selected_device = device_collection.Item(0)?;
                let property_store = selected_device.OpenPropertyStore(STGM_READ)?;
                let name = property_store.GetValue(&PKEY_Device_FriendlyName)?;
                println!("Default Device: {}", name.Anonymous.Anonymous.Anonymous.pwszVal.display());

                let device_id = PCWSTR(selected_device.GetId()?.0);
                println!("{}", device_id.display());

                //let policy_config: IPolicyConfig = CoCreateInstance(&PolicyConfigClient, None, CLSCTX_ALL)?;
                //policy_config.SetDefaultEndpoint(&device_id, eConsole)?;
            }

            {
                let default_device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
                let property_store = default_device.OpenPropertyStore(STGM_READ)?;
                let name = property_store.GetValue(&PKEY_Device_FriendlyName)?;
                println!("Default Device: {}", name.Anonymous.Anonymous.Anonymous.pwszVal.display());
            }

            */
            Ok(Self {
                enumerator,
                policy_config,
            })
        }
    }

    pub fn devices(&self) -> Result<impl Iterator<Item=Result<AudioDevice>>> {
        unsafe {
            let device_collection = self.enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            let count = device_collection.GetCount()?;
            Ok(AudioDeviceIterator {
                device_collection,
                count,
                index: 0,
            })
        }
    }

    pub fn get_default_device(&self) -> Result<AudioDevice> {
        unsafe {
            let device = self.enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            Ok(AudioDevice::new(device)?)
        }
    }

    pub fn set_default_device(&self, device: &AudioDevice) -> Result<()> {
        unsafe {
            self.policy_config.SetDefaultEndpoint(device.id(), eConsole)?;
            Ok(())
        }
    }

}

impl Drop for AudioManager {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

#[derive(Debug, Clone)]
struct AudioDeviceIterator {
    device_collection: IMMDeviceCollection,
    count: u32,
    index: u32
}

impl Iterator for AudioDeviceIterator {
    type Item = Result<AudioDevice>;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if self.index < self.count {
                let item = self.device_collection.Item(self.index)
                    .context("Error retrieving item");
                self.index += 1;
                Some(item.and_then(AudioDevice::new))
            } else {
                None
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.count as usize, Some(self.count as usize))
    }
}

impl ExactSizeIterator for AudioDeviceIterator {}
impl FusedIterator for AudioDeviceIterator {}


#[derive(Debug, Clone)]
pub struct AudioDevice {
    name: String,
    id: Box<[u16]>
}

impl AudioDevice {

    fn new(device: IMMDevice) -> Result<Self> {
        unsafe {
            let id = {
                let ptr = device.GetId()?;
                let id = ptr.as_wide().cloned();
                CoTaskMemFree(Some(ptr.as_ptr() as _));
                id
            };
            let name = {
                let property_store = device.OpenPropertyStore(STGM_READ)?;
                let name = property_store.GetValue(&PKEY_Device_FriendlyName)?;
                name.Anonymous.Anonymous.Anonymous.pwszVal.to_string()?
            };
            Ok(Self{
                name,
                id,
            })
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn id(&self) -> PCWSTR {
        PCWSTR::from_raw(self.id.as_ptr())
    }

}

impl PartialEq for AudioDevice {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}
impl Eq for AudioDevice {}