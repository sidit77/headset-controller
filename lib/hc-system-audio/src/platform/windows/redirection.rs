use std::ptr;
use std::thread::{Builder, JoinHandle};
use hc_foundation::{ensure, Result};
use windows::core::{GUID, w};
use windows::core::implement;
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, CloseHandle};
use windows::Win32::Media::Audio::*;
use windows::Win32::Media::Audio::AUDCLNT_BUFFERFLAGS_SILENT;
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl, IAudioEndpointVolume};
use windows::Win32::System::Com::CLSCTX_ALL;
use windows::Win32::System::Threading::*;
use super::AudioDevice;
use super::com::{ComObj, ComPtr, initialize_com};

#[implement(IAudioEndpointVolumeCallback)]
struct AudioEndpointVolumeCallback(ISimpleAudioVolume);

impl IAudioEndpointVolumeCallback_Impl for AudioEndpointVolumeCallback {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        unsafe {
            let notify = pnotify.read();
            self.0
                .SetMasterVolume(notify.fMasterVolume, &notify.guidEventContext)?;
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
            self.audio_volume
                .UnregisterControlChangeNotify(&self.callback)
                .unwrap_or_else(|err| tracing::warn!("Failed to unregister volume control handler: {}", err))
        }
    }
}

struct AudioThreadHandle {
    handle: HANDLE,
    _task_id: u32
}

impl Drop for AudioThreadHandle {
    fn drop(&mut self) {
        unsafe {
            AvRevertMmThreadCharacteristics(self.handle)
                .unwrap_or_else(|err| tracing::warn!("Could not revert to normal thread: {}", err))
        }
    }
}

fn mark_audio_thread() -> Result<AudioThreadHandle> {
    let mut task_id = 0;
    let handle = unsafe { AvSetMmThreadCharacteristicsW(w!("Audio"), &mut task_id)? };
    Ok(AudioThreadHandle { handle, _task_id: task_id })
}

pub struct AudioRedirection {
    stop_event: HANDLE,
    _volume_sync: VolumeSync,
    audio_thread: Option<JoinHandle<()>>
}

impl AudioRedirection {

    pub const fn is_supported() -> bool {
        true
    }

    pub fn new(src: &AudioDevice, dst: &AudioDevice) -> Result<Self> {
        Ok(unsafe {
            let src_audio_client = ComObj::<IAudioClient>::new(src.device.Activate(CLSCTX_ALL, None)?);
            let dst_audio_client = ComObj::<IAudioClient>::new(dst.device.Activate(CLSCTX_ALL, None)?);

            let format = ComPtr::from_ptr(src_audio_client.GetMixFormat()?);
            ensure!(!format.ptr().is_null(), "Could not retrieve current format");
            let bytes_per_frame = format.ptr().read_unaligned().nBlockAlign as u32;
            let sound_buffer_duration = 10000000;

            src_audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_NOPERSIST | AUDCLNT_SESSIONFLAGS_DISPLAY_HIDE,
                sound_buffer_duration,
                0,
                format.ptr(),
                None
            )?;

            dst_audio_client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_RATEADJUST | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                sound_buffer_duration,
                0,
                format.ptr(),
                None
            )?;

            let dst_audio_volume: ISimpleAudioVolume = src_audio_client.GetService()?;
            let src_volume: IAudioEndpointVolume = src.device.Activate(CLSCTX_ALL, None)?;
            let volume_sync = VolumeSync::new(src_volume, dst_audio_volume)?;

            let capture_client = ComObj::<IAudioCaptureClient>::new(src_audio_client.GetService()?);
            let render_client = ComObj::<IAudioRenderClient>::new(dst_audio_client.GetService()?);

            let stop_event = CreateEventExW(None, None, CREATE_EVENT(0), (EVENT_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE).0)?;
            let buffer_event = CreateEventExW(None, None, CREATE_EVENT(0), (EVENT_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE).0)?;
            src_audio_client.SetEventHandle(buffer_event)?;

            let audio_thread = Some(
                Builder::new()
                    .name("loopback audio router".to_string())
                    .spawn(move || {
                        initialize_com();
                        let _handle = mark_audio_thread().map_err(|err| tracing::warn!("Could not mark as audio thread: {:?}", err));

                        src_audio_client.Start().unwrap();
                        dst_audio_client.Start().unwrap();
                        loop {
                            let wait_result = WaitForMultipleObjects(&[buffer_event, stop_event], false, INFINITE);
                            match wait_result.0 - WAIT_OBJECT_0.0 {
                                0 => copy_data(&capture_client, &render_client, bytes_per_frame).unwrap(),
                                1 => break,
                                _ => unreachable!()
                            }
                        }
                        CloseHandle(buffer_event)
                            .unwrap_or_else(|err| tracing::warn!("Could not delete buffer event: {}", err));
                        src_audio_client.Stop().unwrap();
                        dst_audio_client.Stop().unwrap();
                    })?
            );
            AudioRedirection {
                stop_event,
                _volume_sync: volume_sync,
                audio_thread
            }
        })
    }

    pub fn stop(&self) {
        unsafe {
            SetEvent(self.stop_event)
                .unwrap_or_else(|err| tracing::warn!("Could not set stop event: {}", err));
        }
    }
}

impl Drop for AudioRedirection {
    fn drop(&mut self) {
        self.stop();
        if let Some(thread) = self.audio_thread.take() {
            thread.join().unwrap();
        }
        unsafe {
            CloseHandle(self.stop_event)
                .unwrap_or_else(|err| tracing::warn!("Could not delete stop event: {}", err));
        }
    }
}

unsafe fn copy_data(src: &IAudioCaptureClient, dst: &IAudioRenderClient, bytes_per_frame: u32) -> Result<()> {
    let mut packet_length = src.GetNextPacketSize()?;
    while packet_length != 0 {
        let mut buffer = ptr::null_mut();
        let mut flags = 0;
        let mut frames_available = 0;
        src.GetBuffer(&mut buffer, &mut frames_available, &mut flags, None, None)?;
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
