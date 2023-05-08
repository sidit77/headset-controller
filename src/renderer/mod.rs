#[cfg(all(not(windows), feature = "directx"))]
compile_error!("DirectX is only supported on windows.");

#[cfg(not(any(feature = "directx", feature = "opengl")))]
compile_error!("You must select a backend. Use --feature directx/opengl");

#[cfg(feature = "directx")]
#[path = "d3d11.rs"]
mod backend;

#[cfg(feature = "opengl")]
#[path = "gl.rs"]
mod backend;

use std::time::Instant;

use egui::{Context, FullOutput, Visuals};
use egui_tao::State;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::EventLoopWindowTarget;
#[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
use tao::platform::unix::WindowBuilderExtUnix;
#[cfg(windows)]
use tao::platform::windows::WindowBuilderExtWindows;
use tao::window::WindowBuilder;
use tracing::instrument;

use crate::renderer::backend::{GraphicsWindow, Painter};

pub struct EguiWindow {
    window: GraphicsWindow,
    painter: Painter,
    ctx: Context,
    state: State,

    next_repaint: Option<Instant>
}

impl EguiWindow {
    #[instrument(skip_all, name = "egui_window_new")]
    pub fn new(event_loop: &EventLoopWindowTarget<()>) -> Self {
        let window_builder = WindowBuilder::new()
            .with_resizable(true)
            .with_inner_size(LogicalSize { width: 800.0, height: 600.0 })
            .with_window_icon(Some(crate::ui::WINDOW_ICON.clone()))
            .with_title("Headset Controller");

        #[cfg(windows)]
        let window_builder = window_builder
            .with_drag_and_drop(false);

        #[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
        let window_builder = window_builder
            .with_double_buffered(false)
            .with_app_paintable(true);

        let window = GraphicsWindow::new(window_builder, event_loop);

        let painter = window.make_painter();

        let ctx = Context::default();
        ctx.set_visuals(Visuals::light());

        Self {
            window,
            painter,
            ctx,
            state: State::new(),
            next_repaint: Some(Instant::now())
        }
    }

    pub fn next_repaint(&self) -> Option<Instant> {
        self.next_repaint
    }

    pub fn focus(&self) {
        self.window.window().set_focus();
    }

    #[instrument(skip_all)]
    fn redraw(&mut self, gui: impl FnMut(&Context)) {
        let window = self.window.window();
        let raw_input = self.state.take_egui_input(window);
        let FullOutput {
            platform_output,
            repaint_after,
            mut textures_delta,
            shapes
        } = self.ctx.run(raw_input, gui);

        self.state
            .handle_platform_output(window, &self.ctx, platform_output);

        self.next_repaint = Instant::now().checked_add(repaint_after);
        {
            self.window.clear();

            for (id, image_delta) in textures_delta.set {
                self.painter.set_texture(id, &image_delta);
            }

            let clipped_primitives = self.ctx.tessellate(shapes);
            let dimensions: [u32; 2] = window.inner_size().into();
            self.painter
                .paint_primitives(dimensions, self.ctx.pixels_per_point(), &clipped_primitives);

            for id in textures_delta.free.drain(..) {
                self.painter.free_texture(id);
            }

            self.window.swap_buffers();
        }
    }

    #[instrument(skip_all)]
    pub fn handle_events(&mut self, event: &Event<()>, gui: impl FnMut(&Context)) -> bool {
        if self
            .next_repaint
            .map(|t| Instant::now().checked_duration_since(t))
            .is_some()
        {
            self.window.window().request_redraw();
        }
        match event {
            Event::RedrawEventsCleared if cfg!(windows) => self.redraw(gui),
            Event::RedrawRequested(_) if !cfg!(windows) => self.redraw(gui),
            Event::WindowEvent { event, .. } => {
                match &event {
                    WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                        return true;
                    }
                    WindowEvent::Resized(physical_size) => {
                        self.window.resize(*physical_size);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        self.window.resize(**new_inner_size);
                    }
                    _ => {}
                }

                let event_response = self.state.on_event(&self.ctx, event);
                if event_response.repaint {
                    self.window.window().request_redraw();
                }
            }
            _ => ()
        }
        false
    }
}

impl Drop for EguiWindow {
    #[instrument(skip_all)]
    fn drop(&mut self) {
        self.painter.destroy();
    }
}
