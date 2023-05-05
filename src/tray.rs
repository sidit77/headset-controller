use tao::event_loop::EventLoopWindowTarget;
use tao::menu::{ContextMenu, CustomMenuItem, MenuId, MenuItemAttributes};
use tao::system_tray::{SystemTray, SystemTrayBuilder};
use crate::ui::WINDOW_ICON;

pub struct AppTray {
    tray: SystemTray,
    menu: TrayMenu
}

impl AppTray {

    pub fn new<T>(event_loop: &EventLoopWindowTarget<T>) -> Self {
        let (m, menu) = TrayMenu::new(std::iter::empty());
        let tray = SystemTrayBuilder::new(WINDOW_ICON.clone(), Some(m))
            .build(event_loop)
            .expect("Could not build tray icon");
        Self {
            tray,
            menu,
        }
    }

    pub fn build_menu<'a>(&mut self, profiles_names: impl Iterator<Item=&'a str>) {
        let (m, menu) = TrayMenu::new(profiles_names);
        self.menu = menu;
        self.tray.set_menu(&m);
    }

    pub fn set_tooltip(&mut self, tooltip: &str) {
        self.tray.set_tooltip(tooltip)
    }

    pub fn handle_event(&self, id: MenuId) -> Option<TrayEvent> {
        self.menu.handle_event(id)
    }

}

struct TrayMenu {
    profile_buttons: Vec<CustomMenuItem>,
    quit_button: CustomMenuItem,
    open_button: CustomMenuItem
}

impl TrayMenu {

    pub fn new<'a>(profiles_names: impl Iterator<Item=&'a str>) -> (ContextMenu, Self) {
        let mut menu = ContextMenu::new();
        let mut profiles = ContextMenu::new();
        let mut profile_buttons = Vec::new();
        for profile in profiles_names {
            profile_buttons.push(profiles.add_item(MenuItemAttributes::new(profile)));
        }
        menu.add_submenu("Profiles", true, profiles);
        let open_button = menu.add_item(MenuItemAttributes::new("Open"));
        let quit_button = menu.add_item(MenuItemAttributes::new("Quit"));
        (menu, Self {
            profile_buttons,
            quit_button,
            open_button,
        })
    }

    pub fn handle_event(&self, id: MenuId) -> Option<TrayEvent> {
        if self.open_button.clone().id() == id {
            return Some(TrayEvent::Open)
        }
        if self.quit_button.clone().id() == id {
            return Some(TrayEvent::Quit)
        }
        for profile in &self.profile_buttons {
            if profile.clone().id() == id {
                tracing::info!("{}", profile.title());
            }
        }
        None
    }

}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TrayEvent {
    Open,
    Quit
}