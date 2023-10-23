mod window;

mod runtime;

use once_cell::sync::Lazy;
use winit::window::Icon;
pub use window::Gui;
pub use runtime::{block_on, AsyncGuiWindow};

#[cfg(windows)]
pub static WINDOW_ICON: Lazy<Icon> = Lazy::new(|| {
    use winit::platform::windows::IconExtWindows;
    Icon::from_resource(32512, None).unwrap()
});

#[cfg(not(windows))]
pub static WINDOW_ICON: Lazy<Icon> = Lazy::new(|| {
    let mut decoder = png::Decoder::new(include_bytes!("../../resources/icon.png").as_slice());
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    Icon::from_rgba(buf, info.width, info.height).unwrap()
});
