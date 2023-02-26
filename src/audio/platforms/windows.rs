use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::{ptr, thread};
use std::ops::Deref;
use std::thread::JoinHandle;
use anyhow::{ensure, Result};
use com_policy_config::{IPolicyConfig, PolicyConfigClient};
use widestring::{U16CString};
use windows::core::{GUID, HRESULT, implement, Interface, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, ERROR_NOT_FOUND, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SESSIONFLAGS_DISPLAY_HIDE, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_NOPERSIST, AUDCLNT_STREAMFLAGS_RATEADJUST, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE_ACTIVE, eConsole, eRender, IAudioCaptureClient, IAudioClient, IAudioRenderClient, IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator};
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointFormatControl_Impl, IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize, STGM_READ, VT_LPWSTR};
use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
use windows::Win32::System::Threading::{CREATE_EVENT, CreateEventExW, EVENT_MODIFY_STATE, SetEvent, SYNCHRONIZATION_SYNCHRONIZE, WaitForMultipleObjects};
use windows::Win32::System::WindowsProgramming::INFINITE;
use crate::util::LogResultExt;

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

#[implement(IAudioEndpointVolumeCallback)]
struct AudioEndpointVolumeCallback(ISimpleAudioVolume);

impl IAudioEndpointVolumeCallback_Impl for AudioEndpointVolumeCallback {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        unsafe {
            let notify = pnotify.read();
            self.0.SetMasterVolume(notify.fMasterVolume, &notify.guidEventContext)?;
            self.0.SetMute(notify.bMuted, &notify.guidEventContext)?;
        }
        Ok(())
    }
}

struct VolumeSync {
    callback: IAudioEndpointVolumeCallback,
    audio_volume: IAudioEndpointVolume
}

impl VolumeSync {
    fn new(src_volume: IAudioEndpointVolume, dst_volume: ISimpleAudioVolume) -> Result<Self> {
        unsafe {
            dst_volume.SetMasterVolume(src_volume.GetMasterVolumeLevelScalar()?, &GUID::default())?;
            dst_volume.SetMute(src_volume.GetMute()?, &GUID::default())?;
            let callback: IAudioEndpointVolumeCallback = AudioEndpointVolumeCallback(dst_volume).into();
            src_volume.RegisterControlChangeNotify(&callback)?;
            Ok(Self {
                callback,
                audio_volume: src_volume
            })
        }
    }
}

impl Drop for VolumeSync {
    fn drop(&mut self) {
        unsafe {
            self.audio_volume.UnregisterControlChangeNotify(&self.callback)
                .log_ok("Error removing volume control handler");
        }
    }
}

pub struct AudioLoopback {
    stop_event: HANDLE,
    volume_sync: VolumeSync,
    audio_thread: Option<JoinHandle<()>>
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
                                        AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_NOPERSIST | AUDCLNT_SESSIONFLAGS_DISPLAY_HIDE,
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

            let dst_audio_volume: ISimpleAudioVolume = src_audio_client.GetService()?;
            let src_volume: IAudioEndpointVolume = src.device.Activate(CLSCTX_ALL, None)?;
            let volume_sync = VolumeSync::new(src_volume, dst_audio_volume)?;

            let capture_client = ComObj::<IAudioCaptureClient>(src_audio_client.GetService()?);
            let render_client = ComObj::<IAudioRenderClient>(dst_audio_client.GetService()?);

            let stop_event = CreateEventExW(None, None, CREATE_EVENT(0),
                                              (EVENT_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE).0)?;
            let buffer_event = CreateEventExW(None, None, CREATE_EVENT(0),
                                              (EVENT_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE).0)?;
            src_audio_client.SetEventHandle(buffer_event)?;
            
            let audio_thread = Some(thread::Builder::new()
                .name("loopback audio router".to_string())
                .spawn(move || {
                    com_initialized();

                    src_audio_client.Start().unwrap();
                    dst_audio_client.Start().unwrap();
                    loop {
                        let wait_result = WaitForMultipleObjects(&[buffer_event, stop_event], false, INFINITE);
                        match wait_result.0 - WAIT_OBJECT_0.0 {
                            0 => copy_data(&capture_client, &render_client, bytes_per_frame).unwrap(),
                            1 => break,
                            _ => wait_result.ok().unwrap()
                        }
                    }
                    CloseHandle(buffer_event)
                        .ok()
                        .log_ok("Could not delete buffer event");
                    src_audio_client.Stop().unwrap();
                    dst_audio_client.Stop().unwrap();
            })?);
            AudioLoopback {
                stop_event,
                volume_sync,
                audio_thread,
            }
        })
    }

    pub fn stop(&self) {
        unsafe {
            SetEvent(self.stop_event)
                .ok()
                .log_ok("Could not set stop event");
        }
    }

}

impl Drop for AudioLoopback {
    fn drop(&mut self) {
        self.stop();
        if let Some(thread) = self.audio_thread.take() {
            thread.join().unwrap();
        }
        unsafe {
            CloseHandle(self.stop_event)
                .ok()
                .log_ok("Could not delete stop event");
        }
    }
}

unsafe fn copy_data(src: &IAudioCaptureClient, dst: &IAudioRenderClient, bytes_per_frame: u32) -> Result<()> {
    let mut packet_length  = src.GetNextPacketSize()?;
    while packet_length != 0 {
        let mut buffer = ptr::null_mut();
        let mut flags = 0;
        let mut frames_available = 0;
        src.GetBuffer(&mut buffer,
                      &mut frames_available,
                      &mut flags,
                      None,
                      None)?;
        let silence = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
        {
            let play_buffer = dst.GetBuffer(frames_available)?;
            let buffer_len = (frames_available * bytes_per_frame) as usize;
            if !silence {
                ptr::copy(buffer, play_buffer, buffer_len);
            }
            flags &= AUDCLNT_BUFFERFLAGS_SILENT.0 as u32;
            dst.ReleaseBuffer(frames_available, flags)?;
        }

        src.ReleaseBuffer(frames_available)?;
        packet_length = src.GetNextPacketSize()?;
    }
    Ok(())
}