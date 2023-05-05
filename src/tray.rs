use tao::event_loop::EventLoopWindowTarget;
use tao::menu::{ContextMenu, CustomMenuItem, MenuId, MenuItem, MenuItemAttributes};
use tao::system_tray::{SystemTray, SystemTrayBuilder};

use crate::ui::WINDOW_ICON;

pub struct AppTray {
    tray: SystemTray,
    menu: TrayMenu
}

impl AppTray {
    pub fn new<T>(event_loop: &EventLoopWindowTarget<T>) -> Self {
        let (m, menu) = TrayMenu::new(0, |_| ("", false));
        let tray = SystemTrayBuilder::new(WINDOW_ICON.clone(), Some(m))
            .build(event_loop)
            .expect("Could not build tray icon");
        Self { tray, menu }
    }

    pub fn build_menu<'a, F>(&mut self, profile_count: usize, func: F)
    where
        F: Fn(usize) -> (&'a str, bool)
    {
        self.menu.update(&mut self.tray, profile_count, func)
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

fn next(id: &mut MenuId) -> MenuId {
    id.0 += 1;
    *id
}

impl TrayMenu {
    pub fn new<'a, F>(profile_count: usize, func: F) -> (ContextMenu, Self)
    where
        F: Fn(usize) -> (&'a str, bool)
    {
        let mut id = MenuId::EMPTY;
        let mut menu = ContextMenu::new();
        let mut profiles = ContextMenu::new();
        let mut profile_buttons = Vec::new();
        for i in 0..profile_count {
            let (name, selected) = func(i);
            let item = MenuItemAttributes::new(name)
                .with_id(next(&mut id))
                .with_selected(selected);
            profile_buttons.push(profiles.add_item(item));
        }
        menu.add_submenu("Profiles", profile_count > 0, profiles);
        menu.add_native_item(MenuItem::Separator);
        let open_button = menu.add_item(MenuItemAttributes::new("Open").with_id(next(&mut id)));
        let quit_button = menu.add_item(MenuItemAttributes::new("Quit").with_id(next(&mut id)));
        (
            menu,
            Self {
                profile_buttons,
                quit_button,
                open_button
            }
        )
    }

    pub fn update<'a, F>(&mut self, tray: &mut SystemTray, profile_count: usize, func: F)
    where
        F: Fn(usize) -> (&'a str, bool)
    {
        if profile_count == self.profile_buttons.len() {
            tracing::trace!("Reusing existing menu");
            for (i, button) in self.profile_buttons.iter_mut().enumerate() {
                let (name, selected) = func(i);
                tracing::trace!(name, selected);
                button.set_title(name);
                button.set_selected(selected);
            }
        } else {
            tracing::trace!("Creating new menu");
            let (m, menu) = Self::new(profile_count, func);
            tray.set_menu(&m);
            *self = menu;
        }
    }

    pub fn handle_event(&self, id: MenuId) -> Option<TrayEvent> {
        if self.open_button.clone().id() == id {
            return Some(TrayEvent::Open);
        }
        if self.quit_button.clone().id() == id {
            return Some(TrayEvent::Quit);
        }
        for (i, profile) in self.profile_buttons.iter().enumerate() {
            if profile.clone().id() == id {
                return Some(TrayEvent::Profile(i));
            }
        }
        None
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TrayEvent {
    Open,
    Quit,
    Profile(usize)
}
