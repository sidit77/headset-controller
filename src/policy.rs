#![allow(dead_code)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

use std::ffi::c_void;
use windows::core::{GUID, Interface, IUnknown, Vtable, Result, InParam, PCWSTR, HRESULT};
use windows::Devices::Custom::DeviceSharingMode;
use windows::interface_hierarchy;
use windows::Win32::Foundation::BOOL;
use windows::Win32::Media::Audio::{ERole, WAVEFORMATEX};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

pub const PolicyConfigClient: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IPolicyConfig(IUnknown);
interface_hierarchy!(IPolicyConfig, IUnknown);

impl IPolicyConfig {
    pub unsafe fn GetMixFormat(&self, device_name: impl Into<InParam<PCWSTR>>) -> Result<*mut WAVEFORMATEX> {
        let mut result__ = ::core::mem::MaybeUninit::zeroed();
        (Vtable::vtable(self).GetMixFormat)(Vtable::as_raw(self), device_name.into().abi(), result__.as_mut_ptr()).from_abi(result__)
    }

    pub unsafe fn GetDeviceFormat(&self, device_name: impl Into<InParam<PCWSTR>>, default: impl Into<BOOL>) -> Result<*mut WAVEFORMATEX> {
        let mut result__ = ::core::mem::MaybeUninit::zeroed();
        (Vtable::vtable(self).GetDeviceFormat)(Vtable::as_raw(self), device_name.into().abi(), default.into().0, result__.as_mut_ptr()).from_abi(result__)
    }

    pub unsafe fn ResetDeviceFormat(&self, device_name: impl Into<InParam<PCWSTR>>) -> Result<()> {
        (Vtable::vtable(self).ResetDeviceFormat)(Vtable::as_raw(self), device_name.into().abi()).ok()
    }

    pub unsafe fn SetDeviceFormat(&self, device_name: impl Into<InParam<PCWSTR>>, mut endpoint_format: WAVEFORMATEX, mut mix_format: WAVEFORMATEX) -> Result<()> {
        (Vtable::vtable(self).SetDeviceFormat)(Vtable::as_raw(self), device_name.into().abi(), &mut endpoint_format, &mut mix_format).ok()
    }

    pub unsafe fn GetProcessingPeriod(&self, device_name: impl Into<InParam<PCWSTR>>, default: impl Into<BOOL>, default_period: *mut i64, min_period: *mut i64) -> Result<()> {
        (Vtable::vtable(self).GetProcessingPeriod)(Vtable::as_raw(self), device_name.into().abi(), default.into().0, default_period, min_period).ok()
    }

    pub unsafe fn SetProcessingPeriod(&self, device_name: impl Into<InParam<PCWSTR>>, period: *mut i64) -> Result<()> {
        (Vtable::vtable(self).SetProcessingPeriod)(Vtable::as_raw(self), device_name.into().abi(), period).ok()
    }

    pub unsafe fn GetShareMode(&self, device_name: impl Into<InParam<PCWSTR>>) -> Result<DeviceSharingMode> {
        let mut result__ = ::core::mem::MaybeUninit::zeroed();
        (Vtable::vtable(self).GetShareMode)(Vtable::as_raw(self), device_name.into().abi(), result__.as_mut_ptr()).from_abi(result__)
    }

    pub unsafe fn SetShareMode(&self, device_name: impl Into<InParam<PCWSTR>>, mut mode: DeviceSharingMode) -> Result<()> {
        (Vtable::vtable(self).SetShareMode)(Vtable::as_raw(self), device_name.into().abi(), &mut mode).ok()
    }

    pub unsafe fn GetPropertyValue(&self, device_name: impl Into<InParam<PCWSTR>>, key: *const PROPERTYKEY) -> Result<PROPVARIANT> {
        let mut result__ = ::core::mem::MaybeUninit::zeroed();
        (Vtable::vtable(self).GetPropertyValue)(Vtable::as_raw(self), device_name.into().abi(), key, result__.as_mut_ptr()).from_abi(result__)
    }

    pub unsafe fn SetPropertyValue(&self, device_name: impl Into<InParam<PCWSTR>>, key: *const PROPERTYKEY, propvar: *mut PROPVARIANT) -> Result<()> {
        (Vtable::vtable(self).SetPropertyValue)(Vtable::as_raw(self), device_name.into().abi(), key, propvar).ok()
    }

    pub unsafe fn SetDefaultEndpoint(&self, device_name: impl Into<InParam<PCWSTR>>, role: ERole) -> Result<()> {
        (Vtable::vtable(self).SetDefaultEndpoint)(Vtable::as_raw(self), device_name.into().abi(), role).ok()
    }

    pub unsafe fn SetEndpointVisibility(&self, device_name: impl Into<InParam<PCWSTR>>, visible: impl Into<BOOL>) -> Result<()> {
        (Vtable::vtable(self).SetEndpointVisibility)(Vtable::as_raw(self), device_name.into().abi(), visible.into().0).ok()
    }
}



unsafe impl Vtable for IPolicyConfig { type Vtable = IPolicyConfig_Vtbl; }

unsafe impl Interface for IPolicyConfig {
    const IID: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
}

#[repr(C)]
#[doc(hidden)]
pub struct IPolicyConfig_Vtbl {
    pub base__: ::windows::core::IUnknown_Vtbl,
    pub GetMixFormat: unsafe extern "system" fn(this: *mut c_void, PCWSTR, *mut *mut WAVEFORMATEX) -> HRESULT,
    pub GetDeviceFormat: unsafe extern "system" fn(this: *mut c_void, PCWSTR, i32, *mut *mut WAVEFORMATEX) -> HRESULT,
    pub ResetDeviceFormat: unsafe extern "system" fn(this: *mut c_void, PCWSTR) -> HRESULT,
    pub SetDeviceFormat: unsafe extern "system" fn(this: *mut c_void, PCWSTR, *mut WAVEFORMATEX, *mut WAVEFORMATEX) -> HRESULT,
    pub GetProcessingPeriod: unsafe extern "system" fn(this: *mut c_void, PCWSTR, i32, *mut i64, *mut i64) -> HRESULT,
    pub SetProcessingPeriod: unsafe extern "system" fn(this: *mut c_void, PCWSTR, *mut i64) -> HRESULT,
    pub GetShareMode: unsafe extern "system" fn(this: *mut c_void, PCWSTR, *mut DeviceSharingMode) -> HRESULT,
    pub SetShareMode: unsafe extern "system" fn(this: *mut c_void, PCWSTR, *mut DeviceSharingMode) -> HRESULT,
    pub GetPropertyValue: unsafe extern "system" fn(this: *mut c_void, PCWSTR, *const PROPERTYKEY, *mut PROPVARIANT) -> HRESULT,
    pub SetPropertyValue: unsafe extern "system" fn(this: *mut c_void, PCWSTR, *const PROPERTYKEY, *mut PROPVARIANT) -> HRESULT,
    pub SetDefaultEndpoint: unsafe extern "system" fn(this: *mut c_void, PCWSTR, ERole) -> HRESULT,
    pub SetEndpointVisibility: unsafe extern "system" fn(this: *mut c_void, PCWSTR, i32) -> HRESULT
}