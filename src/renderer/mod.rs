mod gl;

use std::time::Instant;
use egui::{Context, FullOutput, Visuals};
use egui_tao::State;

use tao::dpi::{LogicalSize};
use tao::event::{Event, WindowEvent};
use tao::event_loop::EventLoopWindowTarget;
use tao::window::{WindowBuilder};
use crate::renderer::gl::{GraphicsWindow, Painter};

#[cfg(windows)]
use tao::platform::windows::WindowBuilderExtWindows;

pub struct EguiWindow {
    window: GraphicsWindow,
    painter: Painter,
    ctx: Context,
    state: State,

    next_repaint: Option<Instant>
}

impl EguiWindow {
    pub fn new(event_loop: &EventLoopWindowTarget<()>) -> Self {
        let window_builder = WindowBuilder::new()
            .with_resizable(true)
            .with_inner_size(LogicalSize { width: 800.0, height: 600.0 })
            .with_window_icon(Some(crate::ui::WINDOW_ICON.clone()))
            .with_title("Headset Controller");

        #[cfg(windows)]
        let window_builder = window_builder.with_drag_and_drop(false);

        let window = GraphicsWindow::new(window_builder, event_loop)
            .expect("Failed to create graphics window");

        let painter = window.make_painter();

        let ctx = Context::default();
        ctx.set_visuals(Visuals::light());

        Self {
            window,
            painter,
            ctx,
            state: State::new(),
            next_repaint: Some(Instant::now()),
        }

    }

    pub fn next_repaint(&self) -> Option<Instant> {
        self.next_repaint
    }

    pub fn focus(&self) {
        self.window.window().set_focus();
    }

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

            self.window
                .swap_buffers();
        }
    }

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
    fn drop(&mut self) {
        self.painter.destroy();
    }
}
