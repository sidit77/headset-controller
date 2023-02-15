
#[cfg(target_os = "windows")]
#[path = "platforms/windows.rs"]
mod platform;

#[cfg(not(target_os = "windows"))]
#[path = "platforms/dummy.rs"]
compile_error!("unsupported right now");

pub use platform::*;