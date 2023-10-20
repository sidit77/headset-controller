use std::num::NonZeroU32;
use std::sync::Arc;
use egui::{ClippedPrimitive, TextureId};
use egui::epaint::ImageDelta;
use egui_glow::Painter;
use glow::{COLOR_BUFFER_BIT, Context, HasContext};
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, NotCurrentGlContextSurfaceAccessor, PossiblyCurrentContext};
use glutin::display::{Display, GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::{ApiPrefence, DisplayBuilder, finalize_window, GlWindow};
use raw_window_handle::HasRawWindowHandle;
use tracing::instrument;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::{Window, WindowBuilder};
use crate::framework::window::graphics::{GraphicsContext, GuiPainter};

pub struct OpenGLContext {
    gl_context: PossiblyCurrentContext,
    _gl_display: Display,
    gl_surface: Surface<WindowSurface>,
    gl: Arc<Context>
}

impl GraphicsContext for OpenGLContext {
    type Painter = Painter;

    #[instrument(skip_all)]
    fn initialize<T>(window_builder: WindowBuilder, event_loop: &EventLoopWindowTarget<T>) -> (Window, Self) {
        let template = ConfigTemplateBuilder::new()
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false)
            .prefer_hardware_accelerated(None);

        tracing::debug!("trying to get gl_config");
        let (mut window, gl_config) = DisplayBuilder::new()
            .with_preference(ApiPrefence::FallbackEgl)
            .with_window_builder(Some(window_builder.clone()))
            .build(event_loop, template, |mut configs| {
                configs
                    .next()
                    .expect("failed to find a matching configuration for creating glutin config")
            })
            .expect("failed to create gl_config");

        tracing::debug!("found gl_config: {:?}", &gl_config);

        let raw_window_handle = window.as_ref().map(|window| window.raw_window_handle());
        tracing::debug!("raw window handle: {:?}", raw_window_handle);
        let gl_display = gl_config.display();

        let context_attributes = ContextAttributesBuilder::new().build(raw_window_handle);

        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(raw_window_handle);

        let mut not_current_gl_context = Some(unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    tracing::debug!(
                        "failed to create gl_context with attributes: {:?}. retrying with fallback context attributes: {:?}",
                        &context_attributes,
                        &fallback_context_attributes
                    );
                    gl_display
                        .create_context(&gl_config, &fallback_context_attributes)
                        .expect("failed to create context")
                })
        });

        let window = window.take().unwrap_or_else(|| {
            tracing::debug!("window doesn't exist yet. creating one now with finalize_window");
            finalize_window(event_loop, window_builder, &gl_config).expect("failed to finalize glutin window")
        });

        let attrs = window.build_surface_attributes(SurfaceAttributesBuilder::default());
        tracing::debug!("creating surface with attributes: {:?}", &attrs);
        let gl_surface = unsafe {
            gl_config
                .display()
                .create_window_surface(&gl_config, &attrs)
                .expect("Failed to create window surface")
        };
        tracing::debug!("surface created successfully: {:?}.making context current", gl_surface);
        let gl_context = not_current_gl_context
            .take()
            .unwrap()
            .make_current(&gl_surface)
            .expect("Could not make context current");

        gl_surface
            .set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
            .expect("Failed to activate vsync");

        let gl = Arc::new(unsafe {
            Context::from_loader_function(|s| {
                let s = std::ffi::CString::new(s).expect("failed to construct C string from string for gl proc address");
                gl_display.get_proc_address(&s)
            })
        });

        (window, Self {
            gl_context,
            _gl_display: gl_display,
            gl_surface,
            gl
        })
    }

    #[instrument(skip_all)]
    fn make_painter(&self) -> Self::Painter {
        Painter::new(self.gl.clone(), "", None).unwrap()
    }


    #[instrument(skip(self))]
    fn resize(&self, physical_size: PhysicalSize<u32>) {
        if physical_size.height != 0 && physical_size.width != 0 {
            self.gl_surface.resize(
                &self.gl_context,
                physical_size.width.try_into().unwrap(),
                physical_size.height.try_into().unwrap()
            );
        }
    }

    #[instrument(skip(self))]
    fn clear(&self) {
        let clear_color = [0.1, 0.1, 0.1];
        unsafe {
            self.gl
                .clear_color(clear_color[0], clear_color[1], clear_color[2], 1.0);
            self.gl.clear(COLOR_BUFFER_BIT);
        }
    }

    #[instrument(skip(self))]
    fn swap_buffers(&self) {
        self.gl_surface
            .swap_buffers(&self.gl_context)
            .expect("Failed to swap buffers")
    }
}

impl GuiPainter for Painter {
    #[inline]
    fn paint_primitives(&mut self, screen_size_px: [u32; 2], pixels_per_point: f32, clipped_primitives: &[ClippedPrimitive]) {
        self.paint_primitives(screen_size_px, pixels_per_point, clipped_primitives)
    }

    #[inline]
    fn set_texture(&mut self, tex_id: TextureId, delta: &ImageDelta) {
        self.set_texture(tex_id, delta)
    }

    #[inline]
    fn free_texture(&mut self, tex_id: TextureId) {
        self.free_texture(tex_id)
    }

    #[inline]
    fn destroy(&mut self) {
        self.destroy()
    }
}