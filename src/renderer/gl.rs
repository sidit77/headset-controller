use std::num::NonZeroU32;
use std::sync::Arc;
pub use egui_glow::Painter;
use tao::window::{Window, WindowBuilder};
use color_eyre::Result;
use glow::{COLOR_BUFFER_BIT, Context, HasContext};
use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, NotCurrentGlContextSurfaceAccessor, PossiblyCurrentContext};
use glutin::display::{Display, GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_tao::{ApiPreference, DisplayBuilder, finalize_window, GlWindow};
use raw_window_handle::HasRawWindowHandle;
use tao::dpi::PhysicalSize;
use tao::event_loop::EventLoopWindowTarget;
use tracing::instrument;

pub struct GraphicsWindow {
    window: Window,
    gl_context: PossiblyCurrentContext,
    _gl_display: Display,
    gl_surface: Surface<WindowSurface>,
    gl: Arc<Context>
}

impl GraphicsWindow {

    #[instrument(skip_all)]
    pub fn new(window_builder: WindowBuilder, event_loop: &EventLoopWindowTarget<()>) -> Result<Self> {
        let template = ConfigTemplateBuilder::new()
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false)
            .prefer_hardware_accelerated(None);

        let display_builder = DisplayBuilder::new()
            .with_preference(ApiPreference::FallbackEgl)
            .with_window_builder(Some(window_builder.clone()));

        let (mut window, gl_config) = display_builder
            .build(event_loop, template, |configs| {
                configs
                    .reduce(|accum, config| match config.num_samples() > accum.num_samples() {
                        true => config,
                        false => accum
                    })
                    .expect("failed to find a matching configuration for creating glutin config")
            })
            .expect("failed to create gl_config");

        println!("Picked a config with {} samples", gl_config.num_samples());

        let raw_window_handle = window.as_ref().map(|window| window.raw_window_handle());
        let gl_display = gl_config.display();

        let context_attributes = ContextAttributesBuilder::new().build(raw_window_handle);

        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(raw_window_handle);

        let mut not_current_gl_context = Some(unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    gl_display
                        .create_context(&gl_config, &fallback_context_attributes)
                        .expect("failed to create context")
                })
        });

        let window = window.take().unwrap_or_else(|| {
            println!("window doesn't exist yet. creating one now with finalize_window");
            finalize_window(event_loop, window_builder, &gl_config).expect("failed to finalize glutin window")
        });

        let attrs = window.build_surface_attributes(SurfaceAttributesBuilder::default());
        println!("creating surface with attributes: {:?}", &attrs);
        let gl_surface = unsafe {
            gl_config
                .display()
                .create_window_surface(&gl_config, &attrs)
                .unwrap()
        };
        println!("surface created successfully: {gl_surface:?}.making context current");
        let gl_context = not_current_gl_context
            .take()
            .unwrap()
            .make_current(&gl_surface)
            .unwrap();

        gl_surface
            .set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
            .unwrap();

        let gl = Arc::new(unsafe {
            Context::from_loader_function(|s| {
                let s = std::ffi::CString::new(s)
                    .expect("failed to construct C string from string for gl proc address");
                gl_display.get_proc_address(&s)
            })
        });

        Ok(Self {
            window,
            gl_context,
            _gl_display: gl_display,
            gl_surface,
            gl,
        })
    }

    pub fn make_painter(&self) -> Painter {
        Painter::new(self.gl.clone(), "", None)
            .unwrap()
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn resize(&self, physical_size: PhysicalSize<u32>) {
        self.gl_surface.resize(
            &self.gl_context,
            physical_size.width.try_into().unwrap(),
            physical_size.height.try_into().unwrap()
        );
    }

    pub fn clear(&self) {
        let clear_color = [0.1, 0.1, 0.1];
        unsafe {
            self.gl
                .clear_color(clear_color[0], clear_color[1], clear_color[2], 1.0);
            self.gl.clear(COLOR_BUFFER_BIT);
        }
    }

    pub fn swap_buffers(&self) {
        self.gl_surface.swap_buffers(&self.gl_context)
            .expect("Failed to swap buffers")
    }

}

