//! Pictures of the editor's windows for a look by eye, drawn with no window open: each
//! frame's egui output painted by egui-wgpu into a texture on the GPU, the last one read back
//! and written as a PNG. For the desktop-only windows the browser checks can't reach (the
//! Requests window and its Bones window). Tests only, and ignored by default: they need a GPU.
//!     cargo test -p engine-game -- --ignored shot

use std::path::Path;

use eframe::egui_wgpu::{self, wgpu};

/// A future that's ready at once (wgpu's are, on the desktop) run to its end.
fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}

pub struct Shooter {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
}

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

impl Shooter {
    pub fn new() -> Result<Shooter, String> {
        let instance = wgpu::Instance::default();
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).map_err(|e| format!("no GPU: {e}"))?;
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| e.to_string())?;
        let renderer = egui_wgpu::Renderer::new(&device, FORMAT, egui_wgpu::RendererOptions::default());
        Ok(Shooter { device, queue, renderer })
    }

    /// A frame's textures (every frame's: a picture read in one is drawn in the next), handled.
    pub fn textures(&mut self, delta: &mut egui::TexturesDelta) {
        for (id, images) in &delta.set {
            for image in images {
                self.renderer.update_texture(&self.device, &self.queue, *id, image);
            }
        }
        for id in &delta.free {
            self.renderer.free_texture(id);
        }
        delta.clear();
    }

    /// A frame's shapes painted on `size` pixels (a point each), written to `path` as a PNG.
    pub fn shoot(&mut self, ctx: &egui::Context, shapes: Vec<egui::epaint::ClippedShape>, size: [u32; 2], path: &Path) -> Result<(), String> {
        let jobs = ctx.tessellate(shapes, 1.0);
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: size, pixels_per_point: 1.0 };
        let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shot"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut buffers = self.renderer.update_buffers(&self.device, &self.queue, &mut encoder, &jobs, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shot"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.09, g: 0.1, b: 0.11, a: 1.0 }), store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            self.renderer.render(&mut pass.forget_lifetime(), &jobs, &screen);
        }
        // rows padded as copies want them
        let row = (size[0] * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let read = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("shot"), size: (row * size[1]) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &read, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(size[1]) } },
            extent,
        );
        buffers.push(encoder.finish());
        self.queue.submit(buffers);
        read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
        let mut rgba = Vec::with_capacity((size[0] * size[1] * 4) as usize);
        {
            let data = read.slice(..).get_mapped_range().map_err(|e| format!("{e:?}"))?;
            for y in 0..size[1] as usize {
                let at = y * row as usize;
                rgba.extend_from_slice(&data[at..at + size[0] as usize * 4]);
            }
        }
        read.unmap();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        image::save_buffer(path, &rgba, size[0], size[1], image::ColorType::Rgba8).map_err(|e| e.to_string())
    }
}
