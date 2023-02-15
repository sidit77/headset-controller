use std::iter::FusedIterator;
use anyhow::{Result};
use com_policy_config::{IPolicyConfig, PolicyConfigClient};
use widestring::{U16CString};
use windows::core::{HRESULT, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::ERROR_NOT_FOUND;
use windows::Win32::Media::Audio::{DEVICE_STATE_ACTIVE, eConsole, eRender, IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize, STGM_READ, VT_LPWSTR};

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

            Ok(Self {
                enumerator,
                policy_config,
            })
        }
    }

    pub fn devices(&self) -> impl Iterator<Item=AudioDevice> {
        unsafe {
            let device_collection = self.enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .expect("Unexpected error");
            let count = device_collection.GetCount()
                .expect("Unexpected error");
            AudioDeviceIterator {
                device_collection,
                count,
                index: 0,
            }
        }
    }

    pub fn get_default_device(&self) -> Option<AudioDevice> {
        unsafe {
            match self.enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                Ok(dev) => Some(AudioDevice::new(dev)),
                Err(err) if err.code() == HRESULT::from(ERROR_NOT_FOUND) => None,
                Err(err) => Err(err).expect("Unexpected error")
            }
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
    type Item = AudioDevice;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if self.index < self.count {
                let item = self.device_collection.Item(self.index)
                    .expect("Unexpected error");
                self.index += 1;
                Some(AudioDevice::new(item))
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
    id: U16CString
}

impl AudioDevice {

    fn new(device: IMMDevice) -> Self {
        unsafe {
            let id = {
                let ptr = device.GetId()
                    .expect("Unexpected error");
                let id = U16CString::from_ptr_str(ptr.as_ptr());
                CoTaskMemFree(Some(ptr.as_ptr() as _));
                id
            };
            let name = {
                let property_store = device.OpenPropertyStore(STGM_READ)
                    .expect("Unexpected error");
                let prop = property_store.GetValue(&PKEY_Device_FriendlyName)
                    .expect("Unexpected error");
                let dynamic_type = prop.Anonymous.Anonymous;
                assert_eq!(dynamic_type.vt, VT_LPWSTR);
                let name: PWSTR = dynamic_type.Anonymous.pwszVal;
                String::from_utf16_lossy(name.as_wide())
            };
            Self{
                name,
                id,
            }
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