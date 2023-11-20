mod central_panel;
mod side_panel;

use egui::panel::Side;
use egui::{CentralPanel, Context, Response, RichText, SidePanel};
use tracing::instrument;

use crate::config::Config;
use crate::debouncer::{Action, ActionProxy, ActionSender};
use crate::devices::{Device, DeviceList};
use crate::ui::central_panel::central_panel;
use crate::ui::side_panel::side_panel;


#[instrument(skip_all)]
pub fn config_ui(
    ctx: &Context,
    sender: &mut ActionProxy,
    config: &mut Config,
    device: &dyn Device,
    device_list: &DeviceList,
    audio_devices: &[String]
) {
    SidePanel::new(Side::Left, "Profiles")
        .resizable(true)
        .width_range(175.0..=400.0)
        .show(ctx, |ui| side_panel(ui, sender, config, device, device_list));
    CentralPanel::default().show(ctx, |ui| central_panel(ui, sender, config, device, audio_devices));
}

#[instrument(skip_all)]
pub fn no_device_ui(ctx: &Context, sender: &mut ActionProxy) {
    CentralPanel::default().show(ctx, |ctx| {
        ctx.vertical_centered(|ctx| {
            ctx.add_space(ctx.available_height() / 3.0);
            ctx.label(RichText::new("No supported device detected!").size(20.0));
            ctx.add_space(10.0);
            if ctx.button(RichText::new("Refresh").size(15.0)).clicked() {
                sender.submit_all([Action::RefreshDeviceList, Action::SwitchDevice]);
                sender.submit_full_change();
            }
        });
    });
}

trait ResponseExt {
    fn submit(self, sender: &mut ActionProxy, auto_update: bool, action: Action) -> Self;
}

impl ResponseExt for Response {
    #[instrument(skip(self, sender, action), name = "submit_response")]
    fn submit(self, sender: &mut ActionProxy, auto_update: bool, action: Action) -> Self {
        if self.changed() {
            sender.submit(Action::SaveConfig);
            if auto_update {
                sender.submit(action);
            }
        }
        if self.drag_released() {
            sender.force(action);
        }
        self
    }
}
