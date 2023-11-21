mod com;

use std::iter::FusedIterator;
#[cfg(feature = "switching-windows")]
use com_policy_config::{IPolicyConfig, PolicyConfigClient};
use widestring::U16CString;
use windows::core::{HRESULT, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::ERROR_NOT_FOUND;
use windows::Win32::Media::Audio::{DEVICE_STATE_ACTIVE, eConsole, eRender, IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, STGM_READ};
use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
use windows::Win32::System::Variant::VT_LPWSTR;
use hc_foundation::Result;
use crate::platform::windows::com::{ComPtr, initialize_com};

#[derive(Debug, Clone)]
pub struct AudioManager {
    enumerator: IMMDeviceEnumerator,
    #[cfg(feature = "switching-windows")]
    policy_config: IPolicyConfig
}

impl AudioManager {

    pub const fn switching_supported() -> bool {
        true
    }

    pub fn new() -> Result<Self> {
        unsafe {
            initialize_com();
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            #[cfg(feature = "switching-windows")]
            let policy_config: IPolicyConfig = CoCreateInstance(&PolicyConfigClient, None, CLSCTX_ALL)?;

            Ok(Self {
                enumerator,
                #[cfg(feature = "switching-windows")]
                policy_config
            })
        }
    }

    pub fn devices(&self) -> impl Iterator<Item = AudioDevice> {
        unsafe {
            let device_collection = self
                .enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .expect("Unexpected error");
            let count = device_collection.GetCount().expect("Unexpected error");
            AudioDeviceIterator {
                device_collection,
                count,
                index: 0
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

    #[cfg(feature = "switching-windows")]
    pub fn set_default_device(&self, device: &AudioDevice) -> Result<()> {
        unsafe {
            self.policy_config
                .SetDefaultEndpoint(device.id(), eConsole)?;
            Ok(())
        }
    }

    #[cfg(not(feature = "switching-windows"))]
    pub fn set_default_device(&self, _device: &AudioDevice) -> Result<()> {
        unimplemented!()
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
                let item = self
                    .device_collection
                    .Item(self.index)
                    .expect("Unexpected error");
                self.index += 1;
                Some(AudioDevice::new(item))
            } else {
                None
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.count - self.index) as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for AudioDeviceIterator {}
impl FusedIterator for AudioDeviceIterator {}

#[derive(Debug, Clone)]
pub struct AudioDevice {
    #[allow(dead_code)]
    device: IMMDevice,
    name: String,
    id: U16CString
}

impl AudioDevice {
    fn new(device: IMMDevice) -> Self {
        unsafe {
            let id = {
                let ptr = ComPtr::from_ptr(device.GetId().expect("Unexpected error").0);
                U16CString::from_ptr_str(ptr.ptr())
            };
            let name = {
                let property_store = device
                    .OpenPropertyStore(STGM_READ)
                    .expect("Unexpected error");
                let mut prop = property_store
                    .GetValue(&PKEY_Device_FriendlyName)
                    .expect("Unexpected error");
                let dynamic_type = &prop.Anonymous.Anonymous;
                assert_eq!(dynamic_type.vt, VT_LPWSTR);
                let name: PWSTR = dynamic_type.Anonymous.pwszVal;
                let result = String::from_utf16_lossy(name.as_wide());
                PropVariantClear(&mut prop).expect("Unexpected error");
                result
            };
            Self { device, name, id }
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    #[allow(dead_code)]
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