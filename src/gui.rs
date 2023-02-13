mod renderer;

use std::sync::Arc;
use std::time::Instant;
use egui::Visuals;
use glow::Context;
use log::LevelFilter;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget};
use tao::menu::{ContextMenu, MenuItemAttributes};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::system_tray::SystemTrayBuilder;
use tao::window::Icon;
use crate::renderer::{create_display, GlutinWindowContext};
use crate::renderer::egui_glow_tao::EguiGlow;


fn main() {
    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        .format_timestamp(None)
        .parse_default_env()
        .init();

    let mut event_loop = EventLoop::new();

    let icon = Icon::from_rgba(vec![0xff; 32 * 32 * 4], 32, 32).unwrap();
    let mut tray_menu = ContextMenu::new();
    let open_item = tray_menu.add_item(MenuItemAttributes::new("Open"));
    let quit_item = tray_menu.add_item(MenuItemAttributes::new("Quit"));
    let _tray = SystemTrayBuilder::new(icon, Some(tray_menu))
        .with_tooltip("Connected")
        .build(&event_loop)
        .expect("Can not build system tray");


    let mut window: Option<EguiWindow> = Some(EguiWindow::new(&event_loop));


    let mut name = String::from("Simon");
    let mut age = 24;

    event_loop.run_return(move |event, event_loop, control_flow| {
        *control_flow = ControlFlow::Wait;

        if window.as_mut().map(|w| w.handle_events(&event, control_flow, |egui_ctx| {
            egui::CentralPanel::default().show(egui_ctx, |ui| {
                ui.vertical_centered_justified(|ui|{
                    ui.heading("My egui Application");
                    ui.horizontal(|ui| {
                        let name_label = ui.label("Your name: ");
                        ui.text_edit_singleline(&mut name)
                        .labelled_by(name_label.id);
                    });
                    ui.add(egui::Slider::new(&mut age, 0..=120).text("age"));
                    if ui.button("Click each year").clicked() {
                        age += 1;
                    }
                    ui.label(format!("Hello '{}', age {}", name, age));
                });
                //ui.spinner();
            });
        })).unwrap_or(false) {
            window.take();
            *control_flow = ControlFlow::Exit;
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
            next_repaint: None,
        }
    }

    fn redraw(&mut self, control_flow: &mut ControlFlow, gui: impl FnMut(&egui::Context)) {
        self.next_repaint = None;
        let repaint_after = self.egui_glow.run(self.gl_window.window(), gui);

        *control_flow = if repaint_after.is_zero() {
            self.gl_window.window().request_redraw();
            ControlFlow::Poll
        } else {
            self.next_repaint = Instant::now().checked_add(repaint_after);
            self.next_repaint
                .map(ControlFlow::WaitUntil)
                .unwrap_or(ControlFlow::Wait)
        };
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

    fn handle_events(&mut self, event: &Event<()>, control_flow: &mut ControlFlow, gui: impl FnMut(&egui::Context)) -> bool{
        match event {
            Event::RedrawEventsCleared if cfg!(windows) => self.redraw(control_flow, gui),
            Event::RedrawRequested(_) if !cfg!(windows) => self.redraw(control_flow, gui),
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
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if self.next_repaint.map(|t| Instant::now().checked_duration_since(t)).is_some() {
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