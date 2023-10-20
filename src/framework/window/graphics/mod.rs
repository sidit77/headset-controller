mod opengl;
mod d3d11;

use egui::{ClippedPrimitive, TextureId};
use egui::epaint::ImageDelta;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::{Window, WindowBuilder};

pub use opengl::OpenGLContext;
pub use d3d11::D3D11Context;

pub trait GuiPainter {
    fn paint_primitives(&mut self, screen_size_px: [u32; 2], pixels_per_point: f32, clipped_primitives: &[ClippedPrimitive]);

    fn set_texture(&mut self, tex_id: TextureId, delta: &ImageDelta);
    fn free_texture(&mut self, tex_id: TextureId);

    fn destroy(&mut self);
}

pub trait GraphicsContext {
    type Painter: GuiPainter;

    fn initialize<T>(window_builder: WindowBuilder, event_loop: &EventLoopWindowTarget<T>) -> (Window, Self);

    fn clear(&self);
    fn swap_buffers(&self);

    fn resize(&self, physical_size: PhysicalSize<u32>);

    fn make_painter(&self) -> Self::Painter;
}

pub trait WindowBuilderExt {
    fn build_context<T, C>(self, event_loop: &EventLoopWindowTarget<T>) -> (Window, C) where C: GraphicsContext;
}

impl WindowBuilderExt for WindowBuilder {
    fn build_context<T, C>(self, event_loop: &EventLoopWindowTarget<T>) -> (Window, C) where C: GraphicsContext {
        C::initialize(self, event_loop)
    }
}
