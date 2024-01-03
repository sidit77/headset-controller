mod headset;
mod profile;

use std::sync::atomic::{AtomicU8, Ordering};
use egui::*;
use tracing::instrument;

use crate::config::{AUTO_START, Config};
use crate::debouncer::{Action, ActionProxy, ActionSender};
use crate::devices::Device;
use crate::ui::central_panel::headset::headset_section;
use crate::ui::central_panel::profile::profile_section;

#[instrument(skip_all)]
pub fn central_panel(ui: &mut Ui, sender: &mut ActionProxy, config: &mut Config, device: &dyn Device, audio_devices: &[String]) {
    ui.style_mut()
        .text_styles
        .get_mut(&TextStyle::Heading)
        .unwrap()
        .size = 25.0;
    ui.style_mut()
        .text_styles
        .get_mut(&TextStyle::Body)
        .unwrap()
        .size = 14.0;
    ui.style_mut()
        .text_styles
        .get_mut(&TextStyle::Button)
        .unwrap()
        .size = 14.0;
    ScrollArea::both().auto_shrink([false; 2]).show(ui, |ui| {
        let auto_update = config.auto_apply_changes;
        let headset = config.get_headset(device.name());
        ui.heading("Profile");
        ui.add_space(7.0);
        profile_section(ui, sender, auto_update, headset.selected_profile(), device);
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);
        ui.heading("Headset");
        ui.add_space(7.0);
        headset_section(ui, sender, auto_update, headset, device, audio_devices);
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);
        ui.heading("Application");
        ui.add_space(7.0);
        if ui
            .checkbox(&mut config.auto_apply_changes, "Auto Apply Changes")
            .changed()
        {
            sender.submit(Action::SaveConfig);
        }
        ui.with_layout(Layout::default().with_main_align(Align::Center), |ui| {
            if ui
                .add_sized([200.0, 20.0], Button::new("Apply Now"))
                .clicked()
            {
                sender.submit_full_change();
            }
        });
        ui.add_space(10.0);
        if let Some(manager) = AUTO_START.as_ref() {
            static CACHED_AUTOSTART: AtomicU8 = AtomicU8::new(2);
            let mut auto_start = match CACHED_AUTOSTART.load(Ordering::Acquire) {
                0 => false,
                1 => true,
                _ => {
                    let v = manager.is_enabled()
                        .map_err(|err| tracing::warn!("Can not get autostart status: {}", err))
                        .unwrap_or(false);
                    CACHED_AUTOSTART.store(v.into(), Ordering::Release);
                    v
                }
            };
            if ui.checkbox(&mut auto_start, "Run On Startup").changed() {
                if auto_start {
                    manager.enable().unwrap_or_else(|err| tracing::warn!("Can not enable auto start: {:?}", err));
                } else {
                    manager.disable().unwrap_or_else(|err| tracing::warn!("Can not disable auto start: {:?}", err));
                }
                CACHED_AUTOSTART.store(2, Ordering::Release);
            }
        }

        ui.add_space(20.0);
        ui.separator();
        ui.add_space(10.0);
        ui.heading("Information");
        ui.add_space(7.0);
        ui.label(concat!("Version: ", env!("CARGO_PKG_VERSION")));
        ui.add_space(7.0);
        ui.horizontal(|ui| {
            ui.label("Repository: ");
            ui.hyperlink("https://github.com/sidit77/headset-controller");
        });
        ui.add_space(7.0);
        ui.label(format!("Config Location: {}", Config::path().display()));
        ui.add_space(12.0);
    });
}

