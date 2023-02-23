use egui::*;
use crate::config::{Config, Profile};
use crate::debouncer::{Action, Debouncer};
use crate::devices::Device;
use crate::submit_profile_change;

pub fn side_panel(ui: &mut Ui, debouncer: &mut Debouncer, config: &mut Config, device: &dyn Device) {
    let headset = config.get_headset(&device.get_info().name);
    ui.style_mut().text_styles.get_mut(&TextStyle::Body).unwrap().size = 14.0;
    ui.label(RichText::from(&device.get_info().manufacturer)
        .heading()
        .size(30.0));
    ui.label(RichText::from(&device.get_info().product)
        .heading()
        .size(20.0));
    ui.separator();
    if device.is_connected() {
        if let Some(battery) = device.get_battery_status() {
            ui.label(format!("Battery: {}", battery));
        }
        ui.add_space(10.0);
        if let Some(mix) = device.get_chat_mix() {
            ui.label("Chat Mix:");
            ProgressBar::new(mix.chat as f32 / 100.0)
                .text("Chat")
                .ui(ui);
            ProgressBar::new(mix.game as f32 / 100.0)
                .text("Game")
                .ui(ui);
        }
    } else {
        ui.label("Not Connected");
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.heading("Profiles");
        let resp = ui.with_layout(Layout::right_to_left(Align::Center), |ui|
            ui.selectable_label(false, RichText::from("+").heading())).inner;
        if resp.clicked() {
            headset.profiles.push(Profile::new(String::from("New Profile")));
            debouncer.submit(Action::SaveConfig);
        }
    });
    ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let old_profile_index = headset.selected_profile_index;
            let mut deleted = None;
            let profile_count = headset.profiles.len();
            for (i, profile) in headset.profiles.iter_mut().enumerate() {
                let resp = ui.with_layout(Layout::default().with_cross_justify(true), |ui|
                    ui.selectable_label(i as u32 == headset.selected_profile_index, &profile.name)).inner;
                let resp = resp.context_menu(|ui| {
                    ui.text_edit_singleline(&mut profile.name);
                    ui.add_space(4.0);
                    if ui.add_enabled(profile_count > 1, Button::new("Delete")).clicked() {
                        deleted = Some(i);
                        ui.close_menu();
                    }
                });
                if resp.clicked() {
                    headset.selected_profile_index = i as u32;
                }
            }
            if let Some(i) = deleted {
                headset.profiles.remove(i);
                debouncer.submit(Action::SaveConfig);
                if i as u32 <= headset.selected_profile_index && headset.selected_profile_index > 0{
                    headset.selected_profile_index -= 1;
                }
            }
            if headset.selected_profile_index != old_profile_index {
                submit_profile_change(debouncer);
                debouncer.submit(Action::SaveConfig);
            }
        });
}