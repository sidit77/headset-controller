#[cfg(target_os = "windows")]
mod windows;
#[cfg(not(target_os = "windows"))]
mod unsupported;

#[cfg(target_os = "windows")]
pub use windows::{AudioManager, AudioDevice, AudioRedirection};

#[cfg(not(target_os = "windows"))]
pub use unsupported::{AudioManager, AudioDevice, AudioRedirection};