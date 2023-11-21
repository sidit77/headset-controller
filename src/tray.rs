use std::sync::Arc;
use betrayer::{Icon, Menu, MenuItem, TrayEvent, TrayIconBuilder};
use flume::{Receiver, Sender};
use tracing::instrument;
use hc_foundation::Result;
use futures_lite::{FutureExt, StreamExt};
use parking_lot::Mutex;
use crate::{SharedState, WindowUpdate};
use crate::config::{HeadsetConfig};
use crate::debouncer::{Action, ActionProxy, ActionSender};

pub enum TrayUpdate {
    RefreshProfiles,
    RefreshTooltip
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum TrayMenuEvent {
    Profile(u32),
    Open,
    Quit
}

#[instrument(skip_all)]
pub async fn manage_tray(
    shared_state: Arc<Mutex<SharedState>>,
    window_sender: Sender<WindowUpdate>,
    mut action_sender: ActionProxy,
    tray_receiver: Receiver<TrayUpdate>) -> Result<()>
{
    #[cfg(windows)]
    let icon = Icon::from_resource(32512, None)?;
    #[cfg(not(windows))]
    let icon = Icon::from_png_bytes(include_bytes!("../resources/icon.png"))?;

    let (menu_sender, menu_receiver) = flume::unbounded();
    let tray = TrayIconBuilder::<TrayMenuEvent>::new()
        .with_icon(icon)
        .with_menu(construct_menu(None))
        .build(move |event| if let TrayEvent::Menu(event) = event {
            let _ = menu_sender.send(event);
        })?;

    let event_handler = menu_receiver
        .stream()
        .take_while(|event| *event != TrayMenuEvent::Quit)
        .for_each(|event| match event {
            TrayMenuEvent::Profile(id) => {
                let _span = tracing::info_span!("profile_change", id).entered();
                let mut state = shared_state.lock();
                if let Some(config) = state.current_headset_config() {
                    if id != config.selected_profile_index {
                        let len = config.profiles.len() as u32;
                        if id < len {
                            config.selected_profile_index = id;
                            action_sender.submit_profile_change();
                            action_sender.submit_all([Action::SaveConfig, Action::UpdateTray]);
                        } else {
                            tracing::warn!(len, "Profile id out of range")
                        }
                    } else {
                        tracing::trace!("Profile already selected");
                    }
                }
            }
            TrayMenuEvent::Open => {
                let _ = window_sender.send(WindowUpdate::Show);
            }
            TrayMenuEvent::Quit => unreachable!()
        });
    let update_handler = tray_receiver
        .stream()
        .for_each(|update| match update {
            TrayUpdate::RefreshProfiles => {
                let menu = construct_menu(shared_state
                    .lock()
                    .current_headset_config());
                tray.set_menu(Some(menu));
            },
            TrayUpdate::RefreshTooltip => {
                let tooltip = shared_state
                    .lock()
                    .device
                    .as_ref()
                    .map(|d| d.name())
                    .unwrap_or("Disconnected");
                tray.set_tooltip(tooltip);
            }
        });
    Ok(update_handler.or(event_handler).await)
}

fn construct_menu(config: Option<&mut HeadsetConfig>) -> Menu<TrayMenuEvent> {
    Menu::new([
        MenuItem::menu("Profiles", config
            .iter()
            .flat_map(|config| config
                .profiles
                .iter()
                .enumerate()
                .map(|(index, profile)| MenuItem::check_button(
                    &profile.name,
                    TrayMenuEvent::Profile(index as u32),
                    index == config.selected_profile_index as usize)))),
        MenuItem::separator(),
        MenuItem::button("Open", TrayMenuEvent::Open),
        MenuItem::button("Close", TrayMenuEvent::Quit)
    ])
}

/*
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
*/