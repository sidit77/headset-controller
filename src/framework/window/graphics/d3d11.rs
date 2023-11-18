use std::cell::Cell;
use egui::{ClippedPrimitive, TextureId};
use egui::epaint::ImageDelta;
use egui_d3d11::{Device, DeviceContext, Painter};
use tracing::instrument;
use windows::Win32::Foundation::{FALSE, HWND};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoopWindowTarget;
use winit::platform::windows::WindowExtWindows;
use winit::window::{Window, WindowBuilder};
use crate::framework::window::graphics::{GraphicsContext, GraphicsContextBuilder};

pub struct D3D11Context {
    device: Device,
    context: DeviceContext,
    swap_chain: IDXGISwapChain1,
    render_target: Cell<Option<ID3D11RenderTargetView>>,
    painter: Painter
}

impl D3D11Context {
    #[instrument(skip_all)]
    fn render_target(&self) -> ID3D11RenderTargetView {
        let target = self.render_target.take().unwrap_or_else(|| unsafe {
            let buffer: ID3D11Texture2D = self
                .swap_chain
                .GetBuffer(0)
                .expect("Can not get a valid back buffer");
            let mut target = None;
            self.device
                .CreateRenderTargetView(&buffer, None, Some(&mut target))
                .expect("Can not create a render target");
            target.expect("Render target is none")
        });
        self.render_target.set(Some(target.clone()));
        target
    }
}

impl GraphicsContextBuilder for D3D11Context {
    type Context = Self;

    #[instrument(skip_all)]
    fn initialize<T>(window_builder: WindowBuilder, event_loop: &EventLoopWindowTarget<T>) -> (Window, Self::Context) {
        let window = window_builder
            .build(event_loop)
            .expect("Failed to create window");

        let (device, context) = unsafe {
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_FLAG::default(),
                Some(&[D3D_FEATURE_LEVEL_11_1]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context)
            )
                .expect("Failed to create d3d11 device");
            (device.unwrap(), context.unwrap())
        };

        let dxgi_factory: IDXGIFactory2 = unsafe { CreateDXGIFactory1().expect("Failed to create dxgi factory") };
        let window_size = window.inner_size();
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: window_size.width,
            Height: window_size.height,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            Stereo: FALSE,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_NONE,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            Flags: 0
        };

        let window_handle = HWND(window.hwnd() as _);
        let swap_chain = unsafe {
            dxgi_factory
                .CreateSwapChainForHwnd(&device, window_handle, &desc, None, None)
                .expect("Failed to create swapchain")
        };

        unsafe {
            swap_chain
                .SetBackgroundColor(&get_background_color())
                .unwrap_or_else(|err| tracing::warn!("Failed to set swapchain color: {}", err));
        }

        let painter = Painter::new(device.clone(), context.clone());

        (window, Self {
            device,
            context,
            swap_chain,
            render_target: Cell::new(None),
            painter,
        })
    }
}

impl GraphicsContext for D3D11Context {

    #[instrument(skip(self))]
    fn resize(&self, size: PhysicalSize<u32>) {
        unsafe {
            self.render_target.set(None);
            self.context.ClearState();
            self.swap_chain
                .ResizeBuffers(0, size.width, size.height, DXGI_FORMAT_UNKNOWN, 0)
                .expect("Failed to resize swapchain");
        }
    }

    #[instrument(skip(self))]
    fn clear(&self) {
        let render_target = self.render_target();
        unsafe {
            self.context
                .OMSetRenderTargets(Some(&[Some(render_target)]), None);
        }
    }

    #[instrument(skip(self))]
    fn swap_buffers(&self) {
        unsafe {
            self.swap_chain
                .Present(1, 0)
                .ok()
                .expect("Could not present swapchain");
        }
    }

    fn paint_primitives(&mut self, screen_size_px: [u32; 2], pixels_per_point: f32, clipped_primitives: &[ClippedPrimitive]) {
        self.painter.paint_primitives(screen_size_px, pixels_per_point, clipped_primitives)
    }

    fn set_texture(&mut self, tex_id: TextureId, delta: &ImageDelta) {
        self.painter.set_texture(tex_id, delta)
    }

    fn free_texture(&mut self, tex_id: TextureId) {
        self.painter.free_texture(tex_id)
    }
}

fn get_background_color() -> DXGI_RGBA {
    let [r, g, b, a] = egui::Visuals::light().window_fill.to_normalized_gamma_f32();
    DXGI_RGBA { r, g, b, a }
}