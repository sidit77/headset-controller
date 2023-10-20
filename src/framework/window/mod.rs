mod graphics;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use egui::{Context, FullOutput, Visuals};
use egui_winit::State;
use tracing::instrument;
use winit::dpi::LogicalSize;
use winit::event::Event;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::{Window, WindowBuilder};

use graphics::{GraphicsContext, GuiPainter, WindowBuilderExt, OpenGLContext};

pub type DefaultGuiWindow = GuiWindow<OpenGLContext>;

pub struct GuiWindow<C: GraphicsContext> {
    window: Window,
    graphics: C,
    painter: C::Painter,
    ctx: Context,
    state: State,
    next_repaint: Option<Instant>,
    close_requested: bool
}

impl<C: GraphicsContext> GuiWindow<C> {
    pub fn new<T>(event_loop: &EventLoopWindowTarget<T>) -> Self {
        let (window, graphics) = WindowBuilder::new()
            .with_resizable(true)
            .with_inner_size(LogicalSize { width: 800.0, height: 600.0 })
            //.with_window_icon(Some(crate::ui::WINDOW_ICON.clone()))
            .with_title("Headset Controller")
            .build_context::<T, C>(event_loop);

        let painter = graphics.make_painter();

        let ctx = Context::default();
        ctx.set_visuals(Visuals::light());

        let state = State::new(&window);

        Self {
            window,
            graphics,
            painter,
            ctx,
            state,
            next_repaint: Some(Instant::now()),
            close_requested: false,
        }
    }

    pub fn next_repaint(&self) -> Option<Instant> {
        self.next_repaint
    }

    pub fn is_close_requested(&self) -> bool {
        self.close_requested
    }

    fn request_redraw(&mut self) {
        self.window.request_redraw();
    }

    #[instrument(skip_all)]
    pub fn handle_events<T>(&mut self, event: &Event<T>) {
        let id = self.window.id();
        match event {
            Event::RedrawRequested(window_id) if window_id == &id => self.redraw(),

            Event::WindowEvent { window_id, event} if window_id == &id => {
                use winit::event::WindowEvent;

                if let WindowEvent::CloseRequested = &event {
                    self.close_requested = true;
                }else if let WindowEvent::Resized(physical_size) = &event {
                    self.graphics.resize(*physical_size);
                } else if let WindowEvent::ScaleFactorChanged { new_inner_size, .. } = &event {
                    self.graphics.resize(**new_inner_size);
                }

                let event_response = self.state.on_event(&self.ctx, event);

                if event_response.repaint {
                    self.request_redraw();
                }
            }
            Event::LoopDestroyed => {
                //egui_glow.destroy();
            }
            //Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
            //    gl_window.window().request_redraw();
            //    redraw_requested = true;
            //},
            //Event::NewEvents(StartCause::Poll) => {
            //    if redraw_requested {
            //        gl_window.window().request_redraw();
            //    }
            //}
            Event::NewEvents(_) => {
                self
                    .next_repaint
                    .map(|t| Instant::now().checked_duration_since(t))
                    .is_some()
                    .then(|| self.request_redraw());
            }
            _ => (),
        }
    }

    fn redraw(&mut self) {
        let gui = |ctx: &Context| {
            static REPAINTS: AtomicU64 = AtomicU64::new(0);
            egui::SidePanel::left("my_side_panel").show(ctx, |ui| {
                ui.heading("Hello World!");

                if ui.button("Quit").clicked() {
                    //quit = true;
                    println!("Click!");
                }
                //ui.color_edit_button_rgb(&mut clear_color);
                ui.collapsing("Spinner", |ui| ui.spinner());
            });
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(format!("draws: {}", REPAINTS.fetch_add(1, Ordering::Relaxed)));
                });
            });
        };


        let raw_input = self.state.take_egui_input(&self.window);
        let FullOutput {
            platform_output,
            repaint_after,
            mut textures_delta,
            shapes
        } = self.ctx.run(raw_input, gui);

        self.state.handle_platform_output(&self.window, &self.ctx, platform_output);

        self.next_repaint = Instant::now().checked_add(repaint_after);
        {
            self.graphics.clear();

            for (id, image_delta) in textures_delta.set {
                self.painter.set_texture(id, &image_delta);
            }

            let clipped_primitives = self.ctx.tessellate(shapes);
            let dimensions: [u32; 2] = self.window.inner_size().into();
            self.painter
                .paint_primitives(dimensions, self.ctx.pixels_per_point(), &clipped_primitives);

            for id in textures_delta.free.drain(..) {
                self.painter.free_texture(id);
            }

            self.graphics.swap_buffers();
        }
    }

}

impl<C: GraphicsContext> Drop for GuiWindow<C> {
    #[instrument(skip_all)]
    fn drop(&mut self) {
        self.painter.destroy();
    }
}