use std::marker::PhantomData;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize};

struct ComWrapper {
    _ptr: PhantomData<*mut ()>
}

impl Default for ComWrapper {
    fn default() -> Self {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .expect("Could not initialize COM");
            let thread = std::thread::current();
            tracing::trace!("Initialized COM on thread \"{}\"", thread.name().unwrap_or(""));
            ComWrapper {
                _ptr: Default::default(),
            }
        }
    }
}

impl Drop for ComWrapper {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

thread_local!(static COM_INITIALIZED: ComWrapper = ComWrapper::default());

#[inline]
pub fn initialize_com() {
    COM_INITIALIZED.with(|_| {});
}

pub struct ComPtr<T>(*mut T);

impl<T> ComPtr<T> {
    pub unsafe fn from_ptr(ptr: *mut T) -> Self {
        Self(ptr)
    }
    pub fn ptr(&self) -> *mut T {
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
