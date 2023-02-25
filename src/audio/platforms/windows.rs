use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{ptr, thread};
use std::ops::Deref;
use std::thread::JoinHandle;
use anyhow::{ensure, Result};
use com_policy_config::{IPolicyConfig, PolicyConfigClient};
use widestring::{U16CString};
use windows::core::{HRESULT, Interface, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, ERROR_NOT_FOUND};
use windows::Win32::Media::Audio::{AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_RATEADJUST, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE_ACTIVE, eConsole, eRender, IAudioCaptureClient, IAudioClient, IAudioRenderClient, IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize, STGM_READ, VT_LPWSTR};
use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
use windows::Win32::System::Threading::{CREATE_EVENT, CreateEventExW, EVENT_MODIFY_STATE, SYNCHRONIZATION_SYNCHRONIZE, WaitForSingleObject};

#[derive(Default)]
struct ComWrapper {
    _ptr: PhantomData<*mut ()>,
}

thread_local!(static COM_INITIALIZED: ComWrapper = {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .expect("Could not initialize COM");
        let thread = std::thread::current();
        log::trace!("Initialized COM on thread \"{}\"", thread.name().unwrap_or(""));
        ComWrapper::default()
    }
});

impl Drop for ComWrapper {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
            let thread = std::thread::current();
            log::trace!("Uninitialized COM on thread \"{}\"", thread.name().unwrap_or(""));
        }
    }
}

#[inline]
pub fn com_initialized() {
    COM_INITIALIZED.with(|_| {});
}

#[derive(Debug, Clone)]
pub struct AudioManager {
    enumerator: IMMDeviceEnumerator,
    policy_config: IPolicyConfig
}

impl AudioManager {

    pub fn new() -> Result<Self> {
        unsafe {
            com_initialized();

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
        let remaining = (self.count - self.index) as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for AudioDeviceIterator {}
impl FusedIterator for AudioDeviceIterator {}


#[derive(Debug, Clone)]
pub struct AudioDevice {
    device: IMMDevice,
    name: String,
    id: U16CString
}

impl AudioDevice {

    fn new(device: IMMDevice) -> Self {
        unsafe {
            let id = {
                let ptr = ComPtr(device.GetId()
                    .expect("Unexpected error").0);
                U16CString::from_ptr_str(ptr.ptr())
            };
            let name = {
                let property_store = device.OpenPropertyStore(STGM_READ)
                    .expect("Unexpected error");
                let mut prop = property_store.GetValue(&PKEY_Device_FriendlyName)
                    .expect("Unexpected error");
                let dynamic_type = &prop.Anonymous.Anonymous;
                assert_eq!(dynamic_type.vt, VT_LPWSTR);
                let name: PWSTR = dynamic_type.Anonymous.pwszVal;
                let result = String::from_utf16_lossy(name.as_wide());
                PropVariantClear(&mut prop)
                    .expect("Unexpected error");
                result
            };
            Self{
                device,
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

#[derive(Clone)]
struct ComObj<T: Interface>(T);
unsafe impl<T: Interface> Send for ComObj<T> {}
unsafe impl<T: Interface> Sync for ComObj<T> {}

impl<T: Interface> Deref for ComObj<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct ComPtr<T>(*mut T);

impl<T> ComPtr<T> {
    fn ptr(&self) -> *mut T {
        self.0
    }
}

impl<T> Drop for ComPtr<T> {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CoTaskMemFree(Some(self.0 as _));
            }
        }
    }
}

pub struct AudioLoopback {
    should_stop: Arc<AtomicBool>,
    audio_thread: JoinHandle<()>
}

impl AudioLoopback {

    pub fn new(src: &AudioDevice, dst: &AudioDevice) -> Result<Self> {
        Ok(unsafe {
            let src_audio_client = ComObj::<IAudioClient>(src.device.Activate(CLSCTX_ALL, None)?);
            let dst_audio_client = ComObj::<IAudioClient>(dst.device.Activate(CLSCTX_ALL, None)?);

            let format = ComPtr(src_audio_client.GetMixFormat()?);
            ensure!(!format.ptr().is_null());
            let bytes_per_frame = format.ptr().read_unaligned().nBlockAlign as u32;
            let sound_buffer_duration = 10000000;

            src_audio_client.Initialize(AUDCLNT_SHAREMODE_SHARED,
                                        AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
                                        sound_buffer_duration,
                                        0,
                                        format.ptr(),
                                        None)?;

            dst_audio_client.Initialize(AUDCLNT_SHAREMODE_SHARED,
                                        AUDCLNT_STREAMFLAGS_RATEADJUST | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                                        sound_buffer_duration,
                                        0,
                                        format.ptr(),
                                        None)?;

            let capture_client = ComObj::<IAudioCaptureClient>(src_audio_client.GetService()?);
            let render_client = ComObj::<IAudioRenderClient>(dst_audio_client.GetService()?);

            let buffer_event = CreateEventExW(None, None, CREATE_EVENT(0),
                                              (EVENT_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE).0)?;
            src_audio_client.SetEventHandle(buffer_event)?;
            
            let should_quit = Arc::new(AtomicBool::new(false));
            let should_quit2 = should_quit.clone();
            
            let audio_thread = thread::Builder::new()
                .name("loopback audio router".to_string())
                .spawn(move || {
                    com_initialized();

                    src_audio_client.Start().unwrap();
                    dst_audio_client.Start().unwrap();
                    while !should_quit.load(Ordering::Relaxed) {
                        WaitForSingleObject(buffer_event, 1000);
                        let mut packet_length  = capture_client.GetNextPacketSize().unwrap();
                        while packet_length != 0 {
                            let mut buffer = ptr::null_mut();
                            let mut flags = 0;
                            let mut frames_available = 0;
                            capture_client.GetBuffer(&mut buffer,
                                                     &mut frames_available,
                                                     &mut flags,
                                                     None,
                                                     None).unwrap();
                            let silence = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
                            {
                                let play_buffer = render_client.GetBuffer(frames_available).unwrap();
                                let buffer_len = (frames_available * bytes_per_frame) as usize;
                                if silence {
                                    ptr::write_bytes(buffer, 0, buffer_len)
                                } else {
                                    ptr::copy(buffer, play_buffer, buffer_len);
                                }

                                render_client.ReleaseBuffer(frames_available, 0).unwrap();
                            }

                            capture_client.ReleaseBuffer(frames_available).unwrap();
                            packet_length = capture_client.GetNextPacketSize().unwrap();
                        }
                    }
                    CloseHandle(buffer_event).ok().unwrap();
                    src_audio_client.Stop().unwrap();
                    dst_audio_client.Stop().unwrap();
            })?;
            AudioLoopback {
                should_stop: should_quit2,
                audio_thread,
            }
        })
    }

    pub fn stop(self) {
        self.should_stop.store(true, Ordering::Relaxed);
        self.audio_thread.join().unwrap();
    }

}