mod profile;
mod headset;

use egui::*;
use crate::audio::AudioSystem;
use crate::config::{Config};
use crate::debouncer::{Action, Debouncer};
use crate::devices::{Device};
use crate::{submit_full_change};
use crate::ui::central_panel::headset::headset_section;
use crate::ui::central_panel::profile::profile_section;
use crate::util::LogResultExt;

pub fn central_panel(ui: &mut Ui, debouncer: &mut Debouncer, config: &mut Config, device: &dyn Device, audio_system: &mut AudioSystem) {
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
            headset_section(ui, debouncer, auto_update, headset, device, audio_system);
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
                    submit_full_change(debouncer);
                }
            });
            ui.add_space(10.0);
            #[cfg(target_os = "windows")]
            {
                let mut auto_start = autostart::is_enabled()
                    .log_ok("Can not get autostart status")
                    .unwrap_or(false);
                if ui.checkbox(&mut auto_start, "Run On Startup").changed() {
                    if auto_start {
                        autostart::enable()
                            .log_ok("Can not enable auto start");
                    } else {
                        autostart::disable()
                            .log_ok("Can not disable auto start");
                    }
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

#[cfg(target_os = "windows")]
mod autostart {
    use std::ffi::OsString;
    use anyhow::Result;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    use winreg::types::FromRegValue;
    use crate::util::LogResultExt;

    fn directory() -> Result<RegKey> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")?;
        Ok(key)
    }

    fn reg_key() -> &'static str  {
        "HeadsetController"
    }

    fn start_cmd() -> Result<OsString> {
        let mut cmd = OsString::from("\"");
        let exe_dir = dunce::canonicalize(std::env::current_exe()?)?;
        cmd.push(exe_dir);
        cmd.push("\"  --quiet");
        Ok(cmd)
    }

    pub fn is_enabled() -> Result<bool> {
        let cmd = start_cmd()?;
        let result = directory()?
            .enum_values()
            .filter_map(|r| r.log_ok("Problem enumerating registry key"))
            .any(|(key, value)|
                key.eq(reg_key()) &&
                    OsString::from_reg_value(&value)
                        .log_ok("Can not decode registry value")
                        .map(|v| v.eq(&cmd))
                        .unwrap_or(false));
        Ok(result)
    }

    pub fn enable() -> Result<()> {
        let key = directory()?;
        let cmd = start_cmd()?;
        key.set_value(reg_key(), &cmd)?;
        Ok(())
    }

    pub fn disable() -> Result<()> {
        let key = directory()?;
        key.delete_value(reg_key())?;
        Ok(())
    }
}
