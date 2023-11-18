#[cfg(feature = "opengl")]
mod opengl;
#[cfg(feature = "directx")]
mod d3d11;

use std::sync::OnceLock;
use egui::{ClippedPrimitive, TextureId};
use egui::epaint::ImageDelta;
use enum_iterator::{all, Sequence};
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::{Window, WindowBuilder};
use crate::util::DebugIter;

#[cfg(feature = "opengl")]
pub use opengl::OpenGLContext;
#[cfg(feature = "directx")]
pub use d3d11::D3D11Context;


#[cfg(not(any(feature = "opengl", feature = "directx")))]
compile_error!("No graphics backend is enabled");

#[derive(Debug, Copy, Clone, Eq, PartialEq, Sequence)]
pub enum GraphicsBackend {
    #[cfg(feature = "directx")]
    DirectX,
    #[cfg(feature = "opengl")]
    OpenGL
}

impl Default for GraphicsBackend {
    fn default() -> Self {
        static DEFAULT: OnceLock<GraphicsBackend> = OnceLock::new();
        *DEFAULT.get_or_init(|| {
            let backend = std::env::var("BACKEND")
                .ok()
                .and_then(|cmd| all::<GraphicsBackend>()
                    .find(|b| cmd.eq_ignore_ascii_case(&format!("{b:?}"))))
                .or(Self::first())
                .expect("Not graphics backend available");
            tracing::debug!("Using {:?} as default graphics backend (available: {:?})", backend, DebugIter(all::<Self>()));
            backend
        })
    }
}

pub trait GraphicsContextBuilder {
    type Context: GraphicsContext;

    fn initialize<T>(window_builder: WindowBuilder, event_loop: &EventLoopWindowTarget<T>) -> (Window, Self::Context);
}

pub trait GraphicsContext {

    fn clear(&self);
    fn swap_buffers(&self);

    fn resize(&self, physical_size: PhysicalSize<u32>);

    fn paint_primitives(&mut self, screen_size_px: [u32; 2], pixels_per_point: f32, clipped_primitives: &[ClippedPrimitive]);

    fn set_texture(&mut self, tex_id: TextureId, delta: &ImageDelta);
    fn free_texture(&mut self, tex_id: TextureId);
}

pub trait WindowBuilderExt {
    fn build_context<T, C>(self, event_loop: &EventLoopWindowTarget<T>) -> (Window, C::Context) where C: GraphicsContextBuilder;

    fn build_dynamic_context<T>(self, backend: GraphicsBackend, event_loop: &EventLoopWindowTarget<T>) -> (Window, Box<dyn GraphicsContext>) where Self: Sized {
        match backend {
            #[cfg(feature = "opengl")]
            GraphicsBackend::OpenGL => make_dynamic(self.build_context::<T, OpenGLContext>(event_loop)),
            #[cfg(feature = "directx")]
            GraphicsBackend::DirectX => make_dynamic(self.build_context::<T, D3D11Context>(event_loop))
        }
    }
}

impl WindowBuilderExt for WindowBuilder {
    fn build_context<T, C>(self, event_loop: &EventLoopWindowTarget<T>) -> (Window, C::Context) where C: GraphicsContextBuilder {
        C::initialize(self, event_loop)
    }
}

fn make_dynamic<C: GraphicsContext + 'static>((window, context): (Window, C)) -> (Window, Box<dyn GraphicsContext>) {
    (window, Box::new(context))
}