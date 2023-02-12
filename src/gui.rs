mod renderer;

use std::time::Instant;
use egui::Visuals;
use log::LevelFilter;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use crate::renderer::{create_display, egui_glow_tao};


fn main() {
    env_logger::builder()
        .filter_level(LevelFilter::Trace)
        .format_timestamp(None)
        .parse_default_env()
        .init();

    let clear_color = [0.1, 0.1, 0.1];

    let event_loop = EventLoop::new();
    let (gl_window, gl) = create_display(&event_loop);
    let gl = std::sync::Arc::new(gl);
    let mut egui_glow = egui_glow_tao::EguiGlow::new(&event_loop, gl.clone(), None);
    egui_glow.egui_ctx.set_visuals(Visuals::light());

    let mut name = String::from("Simon");
    let mut age = 24;

    event_loop.run(move |event, _, control_flow| {
        let mut redraw = || {
            let repaint_after = egui_glow.run(gl_window.window(), |egui_ctx| {
                egui::CentralPanel::default().show(egui_ctx, |ui| {
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
            });

            *control_flow = if repaint_after.is_zero() {
                gl_window.window().request_redraw();
                ControlFlow::Poll
            } else {
                Instant::now()
                    .checked_add(repaint_after)
                    .map(ControlFlow::WaitUntil)
                    .unwrap_or(ControlFlow::Wait)
            };

            {
                unsafe {
                    use glow::HasContext as _;
                    gl.clear_color(clear_color[0], clear_color[1], clear_color[2], 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                }
                egui_glow.paint(gl_window.window());
                gl_window.swap_buffers().unwrap();
                gl_window.window().set_visible(true);
            }
        };

        match event {
            Event::RedrawEventsCleared if cfg!(windows) => redraw(),
            Event::RedrawRequested(_) if !cfg!(windows) => redraw(),
            Event::WindowEvent { event, .. } => {
                match &event {
                    WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                        *control_flow = ControlFlow::Exit
                    },
                    WindowEvent::Resized(physical_size) => {
                        gl_window.resize(*physical_size);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        gl_window.resize(**new_inner_size);
                    },
                    _ => {}
                }

                let event_response = egui_glow.on_event(&event);
                if event_response.repaint {
                    gl_window.window().request_redraw();
                }
            }
            Event::LoopDestroyed => {
                egui_glow.destroy();
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                gl_window.window().request_redraw();
            }
            _ => (),
        }
    });
}

