mod opengl;
//mod d3d11;

use egui::{ClippedPrimitive, TextureId};
use egui::epaint::ImageDelta;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::{Window, WindowBuilder};

pub use opengl::OpenGLContext;
//pub use d3d11::D3D11Context;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum GraphicsBackend {
    OpenGL
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
            GraphicsBackend::OpenGL => make_dynamic(self.build_context::<T, OpenGLContext>(event_loop))
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