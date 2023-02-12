mod renderer;

use std::time::Instant;
use egui::NumExt;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::display::{Display, GetGlDisplay};
use glutin::prelude::*;
use glutin::surface::{Surface, WindowSurface};
use log::LevelFilter;
use raw_window_handle::HasRawWindowHandle;
use tao::dpi::{LogicalSize, PhysicalSize};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget};
use tao::window::{Window, WindowBuilder};
use glutin_tao::{ApiPrefence, DisplayBuilder, finalize_window};
use crate::renderer::egui_glow_tao;

/// The majority of `GlutinWindowContext` is taken from `eframe`
struct GlutinWindowContext {
    window: Window,
    gl_context: PossiblyCurrentContext,
    gl_display: Display,
    gl_surface: Surface<WindowSurface>,
}

impl GlutinWindowContext {
    // refactor this function to use `glutin-winit` crate eventually.
    // preferably add android support at the same time.
    #[allow(unsafe_code)]
    unsafe fn new(event_loop: &EventLoopWindowTarget<()>) -> Self {
        let winit_window_builder = WindowBuilder::new()
            .with_resizable(true)
            .with_inner_size(LogicalSize {
                width: 800.0,
                height: 600.0,
            })
            .with_title("egui_glow example") // Keep hidden until we've painted something. See https://github.com/emilk/egui/pull/2279
            .with_visible(false);

        let config_template_builder = ConfigTemplateBuilder::new()
            .prefer_hardware_accelerated(None)
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false);

        log::debug!("trying to get gl_config");
        let (mut window, gl_config) =
            DisplayBuilder::new() // let glutin-winit helper crate handle the complex parts of opengl context creation
                .with_preference(ApiPrefence::FallbackEgl) // https://github.com/emilk/egui/issues/2520#issuecomment-1367841150
                .with_window_builder(Some(winit_window_builder.clone()))
                .build(
                    event_loop,
                    config_template_builder,
                    |mut config_iterator| {
                        config_iterator.next().expect(
                            "failed to find a matching configuration for creating glutin config",
                        )
                    },
                )
                .expect("failed to create gl_config");
        let gl_display = gl_config.display();
        log::debug!("found gl_config: {:?}", &gl_config);

        let raw_window_handle = window.as_ref().map(|w| w.raw_window_handle());
        log::debug!("raw window handle: {:?}", raw_window_handle);
        let context_attributes =
            ContextAttributesBuilder::new().build(raw_window_handle);
        // by default, glutin will try to create a core opengl context. but, if it is not available, try to create a gl-es context using this fallback attributes
        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(glutin::context::ContextApi::Gles(None))
            .build(raw_window_handle);
        let not_current_gl_context = unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    log::debug!("failed to create gl_context with attributes: {:?}. retrying with fallback context attributes: {:?}",
                            &context_attributes,
                            &fallback_context_attributes);
                    gl_config
                        .display()
                        .create_context(&gl_config, &fallback_context_attributes)
                        .expect("failed to create context even with fallback attributes")
                })
        };

        // this is where the window is created, if it has not been created while searching for suitable gl_config
        let window = window.take().unwrap_or_else(|| {
            log::debug!("window doesn't exist yet. creating one now with finalize_window");
            finalize_window(event_loop, winit_window_builder.clone(), &gl_config)
                .expect("failed to finalize glutin window")
        });
        let (width, height): (u32, u32) = window.inner_size().into();
        let width = std::num::NonZeroU32::new(width.at_least(1)).unwrap();
        let height = std::num::NonZeroU32::new(u32::at_least(height, 1)).unwrap();
        let surface_attributes =
            glutin::surface::SurfaceAttributesBuilder::<WindowSurface>::new()
                .build(window.raw_window_handle(), width, height);
        log::debug!(
            "creating surface with attributes: {:?}",
            &surface_attributes
        );
        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attributes)
                .unwrap()
        };
        log::debug!("surface created successfully: {gl_surface:?}.making context current");
        let gl_context = not_current_gl_context.make_current(&gl_surface).unwrap();

        gl_surface
            .set_swap_interval(
                &gl_context,
                glutin::surface::SwapInterval::Wait(std::num::NonZeroU32::new(1).unwrap()),
            )
            .unwrap();

        GlutinWindowContext {
            window,
            gl_context,
            gl_display,
            gl_surface,
        }
    }

    fn window(&self) -> &Window {
        &self.window
    }

    fn resize(&self, physical_size: PhysicalSize<u32>) {
        self.gl_surface.resize(
            &self.gl_context,
            physical_size.width.try_into().unwrap(),
            physical_size.height.try_into().unwrap(),
        );
    }

    fn swap_buffers(&self) -> glutin::error::Result<()> {
        self.gl_surface.swap_buffers(&self.gl_context)
    }

    fn get_proc_address(&self, addr: &std::ffi::CStr) -> *const std::ffi::c_void {
        self.gl_display.get_proc_address(addr)
    }
}

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

    let mut name = String::from("Simon");
    let mut age = 24;

    event_loop.run(move |event, _, control_flow| {
        let mut redraw = || {
            let quit = false;

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

            *control_flow = if quit {
                ControlFlow::Exit
            } else if repaint_after.is_zero() {
                gl_window.window().request_redraw();
                ControlFlow::Poll
            } else if let Some(repaint_after_instant) =
                Instant::now().checked_add(repaint_after)
            {
                ControlFlow::WaitUntil(repaint_after_instant)
            } else {
                ControlFlow::Wait
            };

            {
                unsafe {
                    use glow::HasContext as _;
                    gl.clear_color(clear_color[0], clear_color[1], clear_color[2], 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                }

                // draw things behind egui here

                egui_glow.paint(gl_window.window());

                // draw things on top of egui here

                gl_window.swap_buffers().unwrap();
                gl_window.window().set_visible(true);
            }
        };

        match event {
            // Platform-dependent event handlers to workaround a winit bug
            // See: https://github.com/rust-windowing/winit/issues/987
            // See: https://github.com/rust-windowing/winit/issues/1619
            Event::RedrawEventsCleared if cfg!(windows) => redraw(),
            Event::RedrawRequested(_) if !cfg!(windows) => redraw(),
            Event::WindowEvent { event, .. } => {
                if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
                    *control_flow = ControlFlow::Exit;
                }

                if let WindowEvent::Resized(physical_size) = &event {
                    gl_window.resize(*physical_size);
                } else if let WindowEvent::ScaleFactorChanged {
                    new_inner_size, ..
                } = &event
                {
                    gl_window.resize(**new_inner_size);
                }

                let event_response = egui_glow.on_event(&event);

                if event_response.repaint {
                    gl_window.window().request_redraw();
                }
            }
            Event::LoopDestroyed => {
                egui_glow.destroy();
            }
            Event::NewEvents(StartCause::ResumeTimeReached {
                                               ..
                                           }) => {
                gl_window.window().request_redraw();
            }

            _ => (),
        }
    });
}

fn create_display(event_loop: &EventLoopWindowTarget<()>, ) -> (GlutinWindowContext, glow::Context) {
    let glutin_window_context = unsafe { GlutinWindowContext::new(event_loop) };
    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            let s = std::ffi::CString::new(s)
                .expect("failed to construct C string from string for gl proc address");

            glutin_window_context.get_proc_address(&s)
        })
    };

    (glutin_window_context, gl)
}