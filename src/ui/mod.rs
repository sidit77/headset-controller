mod side_panel;
mod central_panel;

use egui::{CentralPanel, Context, SidePanel};
use egui::panel::Side;
use once_cell::sync::Lazy;
use tao::window::Icon;
use crate::audio::{AudioDevice, AudioManager};
use crate::config::{Config};
use crate::debouncer::{Debouncer};
use crate::devices::{Device};

use crate::ui::central_panel::central_panel;
use crate::ui::side_panel::side_panel;

#[cfg(windows)]
pub static WINDOW_ICON: Lazy<Icon> = Lazy::new(|| {
    use tao::platform::windows::IconExtWindows;
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

pub fn config_ui(ctx: &Context, debouncer: &mut Debouncer, config: &mut Config, device: &dyn Device, audio_devices: &[AudioDevice], audio_manager: &AudioManager) {
    SidePanel::new(Side::Left, "Profiles")
        .resizable(true)
        .width_range(175.0..=400.0)
        .show(ctx, |ui| side_panel(ui, debouncer, config, device));
    CentralPanel::default()
        .show(ctx, |ui| central_panel(ui, debouncer, config, device, audio_devices, audio_manager));
}

