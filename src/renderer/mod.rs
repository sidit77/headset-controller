pub mod egui_glow_tao;

use std::num::NonZeroU32;
use egui::NumExt;
use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::display::{Display, GetGlDisplay};
use glutin::prelude::*;
use glutin::surface::{Surface, SwapInterval, WindowSurface};
use glutin_tao::{finalize_window, DisplayBuilder, ApiPreference, GlWindow};
use raw_window_handle::HasRawWindowHandle;
use tao::dpi::{LogicalSize, PhysicalSize};
use tao::event_loop::EventLoopWindowTarget;
use tao::window::{Window, WindowBuilder};

/// The majority of `GlutinWindowContext` is taken from `eframe`
pub struct GlutinWindowContext {
    window: Window,
    gl_context: PossiblyCurrentContext,
    gl_display: Display,
    gl_surface: Surface<WindowSurface>
}

impl GlutinWindowContext {
    // refactor this function to use `glutin-winit` crate eventually.
    // preferably add android support at the same time.
    #[allow(unsafe_code)]
    unsafe fn new(event_loop: &EventLoopWindowTarget<()>) -> Self {
        let window_builder = WindowBuilder::new()
            .with_resizable(true)
            .with_inner_size(LogicalSize { width: 800.0, height: 600.0 })
            .with_window_icon(Some(crate::ui::WINDOW_ICON.clone()))
            .with_title("Headset Controller");

        #[cfg(windows)]
        let winit_window_builder = tao::platform::windows::WindowBuilderExtWindows::with_drag_and_drop(winit_window_builder, false);

        let template = ConfigTemplateBuilder::new()
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false)
            .prefer_hardware_accelerated(None);

        let display_builder = DisplayBuilder::new()
            .with_preference(ApiPreference::FallbackEgl)
            .with_window_builder(Some(window_builder.clone()));

        let (mut window, gl_config) = display_builder
            .build(&event_loop, template, |configs| {
                configs.reduce(|accum, config| match config.num_samples() > accum.num_samples() {
                    true => config,
                    false => accum
                }).expect("failed to find a matching configuration for creating glutin config")
            })
            .expect("failed to create gl_config");

        println!("Picked a config with {} samples", gl_config.num_samples());

        let raw_window_handle = window.as_ref().map(|window| window.raw_window_handle());
        let gl_display = gl_config.display();

        let context_attributes = ContextAttributesBuilder::new()
            .build(raw_window_handle);

        let fallback_context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(raw_window_handle);

        let mut not_current_gl_context = Some(unsafe {
            gl_display.create_context(&gl_config, &context_attributes).unwrap_or_else(|_| {
                gl_display.create_context(&gl_config, &fallback_context_attributes).expect("failed to create context")
            })
        });

        let window = window.take().unwrap_or_else(|| {
            println!("window doesn't exist yet. creating one now with finalize_window");
            finalize_window(&event_loop, window_builder, &gl_config)
                .expect("failed to finalize glutin window")
        });

        let attrs = window
            .build_surface_attributes(<_>::default());
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

        GlutinWindowContext {
            window,
            gl_context,
            gl_display,
            gl_surface
        }
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

    pub fn swap_buffers(&self) -> glutin::error::Result<()> {
        self.gl_surface.swap_buffers(&self.gl_context)
    }

    fn get_proc_address(&self, addr: &std::ffi::CStr) -> *const std::ffi::c_void {
        self.gl_display.get_proc_address(addr)
    }
}

pub fn create_display(event_loop: &EventLoopWindowTarget<()>) -> (GlutinWindowContext, glow::Context) {
    let glutin_window_context = unsafe { GlutinWindowContext::new(event_loop) };
    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            let s = std::ffi::CString::new(s).expect("failed to construct C string from string for gl proc address");

            glutin_window_context.get_proc_address(&s)
        })
    };

    (glutin_window_context, gl)
}
