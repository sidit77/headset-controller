use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::cell::Cell;
use egui::{ClippedPrimitive, TextureId};
use egui::epaint::ImageDelta;
use egui_glow::Painter;
use glow::{COLOR_BUFFER_BIT, Context, HasContext};
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, NotCurrentGlContextSurfaceAccessor, PossiblyCurrentContext, PossiblyCurrentContextGlSurfaceAccessor};
use glutin::display::{Display, GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::{ApiPrefence, DisplayBuilder, finalize_window, GlWindow};
use raw_window_handle::HasRawWindowHandle;
use tracing::instrument;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::{Window, WindowBuilder};
use crate::framework::window::graphics::{GraphicsContext, GraphicsContextBuilder};

static COUNTER: AtomicU32 = AtomicU32::new(0);
thread_local! { static CURRENT_CONTEXT: Cell<u32> = Cell::new(0) }

pub struct OpenGLContext {
    id: u32,
    context: PossiblyCurrentContext,
    _display: Display,
    surface: Surface<WindowSurface>,
    gl: Arc<Context>,
    painter: Painter
}

impl GraphicsContextBuilder for OpenGLContext {
    type Context = Self;

    #[instrument(skip_all)]
    fn initialize<T>(window_builder: WindowBuilder, event_loop: &EventLoopWindowTarget<T>) -> (Window, Self) {
        let template = ConfigTemplateBuilder::new()
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false)
            .prefer_hardware_accelerated(None);

        tracing::debug!("trying to get gl_config");
        let (mut window, config) = DisplayBuilder::new()
            .with_preference(ApiPrefence::FallbackEgl)
            .with_window_builder(Some(window_builder.clone()))
            .build(event_loop, template, |mut configs| {
                configs
                    .next()
                    .expect("failed to find a matching configuration for creating glutin config")
            })
            .expect("failed to create gl_config");

        tracing::debug!("found gl_config: {:?}", &config);

        let raw_window_handle = window.as_ref().map(|window| window.raw_window_handle());
        tracing::debug!("raw window handle: {:?}", raw_window_handle);
        let display = config.display();

        let context_attributes = ContextAttributesBuilder::new().build(raw_window_handle);

        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(raw_window_handle);

        let mut not_current_gl_context = Some(unsafe {
            display
                .create_context(&config, &context_attributes)
                .unwrap_or_else(|_| {
                    tracing::debug!(
                        "failed to create gl_context with attributes: {:?}. retrying with fallback context attributes: {:?}",
                        &context_attributes,
                        &fallback_context_attributes
                    );
                    display
                        .create_context(&config, &fallback_context_attributes)
                        .expect("failed to create context")
                })
        });

        let window = window.take().unwrap_or_else(|| {
            tracing::debug!("window doesn't exist yet. creating one now with finalize_window");
            finalize_window(event_loop, window_builder, &config).expect("failed to finalize glutin window")
        });

        let attrs = window.build_surface_attributes(SurfaceAttributesBuilder::default());
        tracing::debug!("creating surface with attributes: {:?}", &attrs);
        let surface = unsafe {
            config
                .display()
                .create_window_surface(&config, &attrs)
                .expect("Failed to create window surface")
        };

        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        tracing::debug!("surface created successfully: {:?}.making context current", surface);
        let context = not_current_gl_context
            .take()
            .unwrap()
            .make_current(&surface)
            .expect("Could not make context current");
        CURRENT_CONTEXT.with(|ctx| ctx.set(id));

        surface
            .set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
            .expect("Failed to activate vsync");

        let gl = Arc::new(unsafe {
            Context::from_loader_function(|s| {
                let s = std::ffi::CString::new(s).expect("failed to construct C string from string for gl proc address");
                display.get_proc_address(&s)
            })
        });

        let painter = Painter::new(gl.clone(), "", None).unwrap();

        (window, Self {
            id,
            context,
            _display: display,
            surface,
            gl,
            painter,
        })
    }
}

impl GraphicsContext for OpenGLContext {

    #[instrument(skip(self))]
    fn resize(&self, physical_size: PhysicalSize<u32>) {
        if physical_size.height != 0 && physical_size.width != 0 {
            self.ensure_context_current();
            self.surface.resize(
                &self.context,
                physical_size.width.try_into().unwrap(),
                physical_size.height.try_into().unwrap()
            );
        }
    }

    #[instrument(skip(self))]
    fn clear(&self) {
        self.ensure_context_current();
        let clear_color = [0.1, 0.1, 0.1];
        unsafe {
            self.gl
                .clear_color(clear_color[0], clear_color[1], clear_color[2], 1.0);
            self.gl.clear(COLOR_BUFFER_BIT);
        }
    }

    #[instrument(skip(self))]
    fn swap_buffers(&self) {
        assert_eq!(CURRENT_CONTEXT.with(Cell::get), self.id);
        self.surface
            .swap_buffers(&self.context)
            .expect("Failed to swap buffers")
    }

    #[inline]
    fn paint_primitives(&mut self, screen_size_px: [u32; 2], pixels_per_point: f32, clipped_primitives: &[ClippedPrimitive]) {
        self.painter.paint_primitives(screen_size_px, pixels_per_point, clipped_primitives)
    }

    #[inline]
    fn set_texture(&mut self, tex_id: TextureId, delta: &ImageDelta) {
        self.painter.set_texture(tex_id, delta)
    }

    #[inline]
    fn free_texture(&mut self, tex_id: TextureId) {
        self.painter.free_texture(tex_id)
    }
}

impl OpenGLContext {
    fn ensure_context_current(&self) {
        CURRENT_CONTEXT.with(|current| {
            if self.id != current.get() {
                self.context.make_current(&self.surface)
                    .expect("Failed to make context current");
                current.set(self.id);
            }
        });
    }
}

impl Drop for OpenGLContext {
    fn drop(&mut self) {
        self.ensure_context_current();
        self.painter.destroy();
    }
}
