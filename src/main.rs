#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod renderer;
mod devices;
mod util;

use std::sync::Arc;
use std::time::Instant;
use egui::Visuals;
use glow::Context;
use log::LevelFilter;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget};
use tao::menu::{ContextMenu, MenuItemAttributes};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::system_tray::SystemTrayBuilder;
use tao::window::Icon;
use crate::devices::BatteryLevel;
use crate::renderer::{create_display, GlutinWindowContext};
use crate::renderer::egui_glow_tao::EguiGlow;


fn main() {
    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        .format_timestamp(None)
        .parse_default_env()
        .init();

    let mut device = devices::find_device().unwrap();

    let mut event_loop = EventLoop::new();

    let icon = Icon::from_rgba(vec![0xff; 32 * 32 * 4], 32, 32).unwrap();
    let mut tray_menu = ContextMenu::new();
    let open_item = tray_menu.add_item(MenuItemAttributes::new("Open"));
    let quit_item = tray_menu.add_item(MenuItemAttributes::new("Quit"));
    let mut tray = SystemTrayBuilder::new(icon, Some(tray_menu))
        .with_tooltip("Not Connected")
        .build(&event_loop)
        .expect("Can not build system tray");


    let mut window: Option<EguiWindow> = Some(EguiWindow::new(&event_loop));


    let mut next_device_poll = Instant::now();
    event_loop.run_return(move |event, event_loop, control_flow| {
        if next_device_poll <= Instant::now() {
            let (last_connected, last_battery) = (device.is_connected(), device.get_battery_status());
            next_device_poll = Instant::now() + device.poll().unwrap();

            if last_connected != device.is_connected() {
                notifica::notify(device.get_name(), match device.is_connected() {
                    true => "Connected",
                    false => "Disconnected"
                }).unwrap();
            }
            if last_battery != device.get_battery_status() {
                tray.set_tooltip(&match device.get_battery_status() {
                    Some(BatteryLevel::Charging) => format!("{}\nBattery: Charging", device.get_name()),
                    Some(BatteryLevel::Level(level)) => format!("{}\nBattery: {}%", device.get_name(), level),
                    _ => format!("{}\nDisconnected", device.get_name())
                });
            }

        }
        let next_update = window
            .as_ref()
            .and_then(|w|w.next_repaint)
            .map(|t| t.min(next_device_poll))
            .unwrap_or(next_device_poll);
        *control_flow = match next_update <= Instant::now() {
            true => ControlFlow::Poll,
            false => ControlFlow::WaitUntil(next_update)
        };
        if window.as_mut().map(|w| w.handle_events(&event, |egui_ctx| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.vertical_centered_justified(|ui|{
                    ui.heading(device.get_name());
                    ui.label(format!("Connected '{:?}'", device.is_connected()));
                    ui.label(format!("Battery '{:?}'", device.get_battery_status()));
                    ui.label(format!("Chatmix '{:?}'", device.get_chat_mix()));
                });
                //ui.spinner();
            });
        })).unwrap_or(false) {
            window.take();
            if cfg!(debug_assertions) {
                *control_flow = ControlFlow::Exit;
            }
        }

        match event {
            Event::MenuEvent { menu_id, ..} => {
                if menu_id == open_item.clone().id() {
                    match &mut window {
                        None => window = Some(EguiWindow::new(event_loop)),
                        Some(window) => {
                            window.gl_window.window().set_focus();
                        }
                    }
                }
                if menu_id == quit_item.clone().id() {
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => (),
        }
    });
}

struct EguiWindow {
    gl_window: GlutinWindowContext,
    gl: Arc<Context>,
    egui_glow: EguiGlow,
    next_repaint: Option<Instant>
}

impl EguiWindow {

    fn new(event_loop: &EventLoopWindowTarget<()>) -> Self {
        let (gl_window, gl) = create_display(&event_loop);
        let gl = Arc::new(gl);
        let egui_glow = EguiGlow::new(&event_loop, gl.clone(), None);
        egui_glow.egui_ctx.set_visuals(Visuals::light());
        gl_window.window().set_visible(true);

        Self {
            gl_window,
            gl,
            egui_glow,
            next_repaint: Some(Instant::now()),
        }
    }

    fn redraw(&mut self, gui: impl FnMut(&egui::Context)) {
        let repaint_after = self.egui_glow.run(self.gl_window.window(), gui);
        self.next_repaint = Instant::now().checked_add(repaint_after);
        {
            let clear_color = [0.1, 0.1, 0.1];
            unsafe {
                use glow::HasContext as _;
                self.gl.clear_color(clear_color[0], clear_color[1], clear_color[2], 1.0);
                self.gl.clear(glow::COLOR_BUFFER_BIT);
            }
            self.egui_glow.paint(self.gl_window.window());
            self.gl_window.swap_buffers().unwrap();
        }
    }

    fn handle_events(&mut self, event: &Event<()>, gui: impl FnMut(&egui::Context)) -> bool{
        if self.next_repaint.map(|t| Instant::now().checked_duration_since(t)).is_some() {
            self.gl_window.window().request_redraw();
        }
        match event {
            Event::RedrawEventsCleared if cfg!(windows) => self.redraw(gui),
            Event::RedrawRequested(_) if !cfg!(windows) => self.redraw(gui),
            Event::WindowEvent { event, .. } => {
                match &event {
                    WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                        return true;
                    },
                    WindowEvent::Resized(physical_size) => {
                        self.gl_window.resize(*physical_size);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        self.gl_window.resize(**new_inner_size);
                    },
                    _ => {}
                }

                let event_response = self.egui_glow.on_event(&event);
                if event_response.repaint {
                    self.gl_window.window().request_redraw();
                }
            }
            _ => (),
        }
        false
    }
}

impl Drop for EguiWindow {
    fn drop(&mut self) {
        self.egui_glow.destroy();
    }
}