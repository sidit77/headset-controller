mod profile;
mod headset;

use egui::*;
use crate::audio::{AudioDevice, AudioManager};
use crate::config::{Config};
use crate::debouncer::{Action, Debouncer};
use crate::devices::{Device};
use crate::{submit_profile_change};
use crate::ui::central_panel::headset::headset_section;
use crate::ui::central_panel::profile::profile_section;

pub fn central_panel(ui: &mut Ui, debouncer: &mut Debouncer, config: &mut Config, device: &dyn Device, audio_devices: &[AudioDevice], audio_manager: &AudioManager) {
    ui.style_mut().text_styles.get_mut(&TextStyle::Heading).unwrap().size = 25.0;
    ui.style_mut().text_styles.get_mut(&TextStyle::Body).unwrap().size = 14.0;
    ui.style_mut().text_styles.get_mut(&TextStyle::Button).unwrap().size = 14.0;
    ScrollArea::both()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let auto_update = config.auto_apply_changes;
            let headset = config.get_headset(&device.get_info().name);
            ui.heading("Profile");
            ui.add_space(7.0);
            profile_section(ui, debouncer, auto_update, headset.selected_profile(), device);
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);
            ui.heading("Headset");
            ui.add_space(7.0);
            headset_section(ui, debouncer, auto_update, headset, device, audio_devices, audio_manager);
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);
            ui.heading("Application");
            ui.add_space(7.0);
            if ui.checkbox(&mut config.auto_apply_changes, "Auto Apply Changes").changed() {
                debouncer.submit(Action::SaveConfig);
            }
            ui.with_layout(Layout::default().with_main_align(Align::Center), |ui| {
                if ui.add_sized([200.0, 20.0], Button::new("Apply Now")).clicked(){
                    submit_profile_change(debouncer);
                }
            });
            ui.add_space(10.0);
            ui.checkbox(&mut false, "Run On Startup");
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

