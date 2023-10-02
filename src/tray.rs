use tray_icon::{TrayIcon, TrayIconBuilder};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use winit::event_loop::EventLoop;

use crate::ui::TRAY_ICON;
use crate::util::SenderExt;

pub struct AppTray {
    tray: TrayIcon
}

impl AppTray {
    pub fn new<T: From<TrayEvent> + Send>(event_loop: &EventLoop<T>) -> Self {
        let tray = TrayIconBuilder::new()
            .with_icon(TRAY_ICON.clone())
            .build()
            .expect("Could not build tray icon");

        let proxy = event_loop.create_proxy();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            match event.id.0.parse::<usize>() {
                Ok(0) => proxy.send_log(TrayEvent::Open),
                Ok(1) => proxy.send_log(TrayEvent::Quit),
                Ok(n) => proxy.send_log(TrayEvent::Profile(n - 2)),
                _ => {}
            }
        }));

        Self {
            tray
        }
    }

    pub fn build_menu<'a, F>(&mut self, profile_count: usize, func: F)
    where
        F: Fn(usize) -> (&'a str, bool)
    {
        let profiles = Submenu::with_id("profiles", "Profiles", true);

        for i in 0..profile_count {
            let (title, selected) = func(i);
            profiles
                .append(&CheckMenuItem::with_id(2 + i, title, true, selected, None))
                .unwrap();
        }

        let menu = Menu::with_items(&[
            &profiles,
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(0,"Open", true, None),
            &MenuItem::with_id(1, "Quit", true, None)
        ]).unwrap();

        self.tray.set_menu(Some(Box::new(menu)));
    }

    pub fn set_tooltip(&mut self, tooltip: &str) {
        self.tray
            .set_tooltip(Some(tooltip))
            .unwrap_or_else(|err| tracing::warn!("Failed to update tray icon tooltip: {:?}", err));
    }

}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TrayEvent {
    Open,
    Quit,
    Profile(usize)
}
