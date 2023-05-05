mod central_panel;
mod side_panel;

use egui::panel::Side;
use egui::{CentralPanel, Context, Response, RichText, SidePanel};
use once_cell::sync::Lazy;
use tao::window::Icon;
use tracing::instrument;

use crate::audio::AudioSystem;
use crate::config::Config;
use crate::debouncer::{Action, Debouncer};
use crate::devices::Device;
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

#[instrument(skip_all)]
pub fn config_ui(ctx: &Context, debouncer: &mut Debouncer, config: &mut Config, device: &dyn Device, audio_system: &mut AudioSystem) {
    SidePanel::new(Side::Left, "Profiles")
        .resizable(true)
        .width_range(175.0..=400.0)
        .show(ctx, |ui| side_panel(ui, debouncer, config, device));
    CentralPanel::default().show(ctx, |ui| central_panel(ui, debouncer, config, device, audio_system));
}

pub fn no_device_ui(ctx: &Context) {
    CentralPanel::default().show(ctx, |ctx| {
        ctx.centered_and_justified(|ctx| {
            ctx.label(RichText::new("No supported device detected!").size(20.0));
        });
    });
}

trait ResponseExt {
    fn submit(self, debouncer: &mut Debouncer, auto_update: bool, action: Action) -> Self;
}

impl ResponseExt for Response {
    #[instrument(skip(self, debouncer, action), name = "submit_response")]
    fn submit(self, debouncer: &mut Debouncer, auto_update: bool, action: Action) -> Self {
        if self.changed() {
            debouncer.submit(Action::SaveConfig);
            if auto_update {
                debouncer.submit(action);
            }
        }
        if self.drag_released() {
            debouncer.force(action);
        }
        self
    }
}
