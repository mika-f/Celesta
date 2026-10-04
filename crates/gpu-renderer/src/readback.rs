use crate::BYTES_PER_PIXEL;
use crate::error::GpuRenderError;
use crate::types::ReadbackFormat;
use crate::yuv::YuvConverter;
use std::sync::mpsc;

pub(crate) struct ReadbackLayout {
    pub(crate) unpadded_bytes_per_row: u32,
    pub(crate) padded_bytes_per_row: u32,
    pub(crate) buffer_size: u64,
}

impl ReadbackLayout {
    pub(crate) fn new(width: u32, height: u32) -> Result<Self, GpuRenderError> {
        Self::with_bytes_per_pixel(width, height, BYTES_PER_PIXEL)
    }

    pub(crate) fn with_bytes_per_pixel(
        width: u32,
        height: u32,
        bytes_per_pixel: u32,
    ) -> Result<Self, GpuRenderError> {
        let unpadded_bytes_per_row = width
            .checked_mul(bytes_per_pixel)
            .ok_or(GpuRenderError::SurfaceTooLarge { width, height })?;
        let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row
            .checked_add(alignment - 1)
            .map(|value| value / alignment * alignment)
            .ok_or(GpuRenderError::SurfaceTooLarge { width, height })?;
        let buffer_size = u64::from(padded_bytes_per_row)
            .checked_mul(u64::from(height))
            .ok_or(GpuRenderError::SurfaceTooLarge { width, height })?;
        Ok(Self {
            unpadded_bytes_per_row,
            padded_bytes_per_row,
            buffer_size,
        })
    }

    /// Copies a mapped readback buffer into tightly packed rows. When
    /// the row width is already aligned (e.g. 1920 or 1280 pixels wide) the
    /// buffer has no padding and is copied in one go.
    pub(crate) fn unpad(
        &self,
        mapped: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, GpuRenderError> {
        let capacity = (self.unpadded_bytes_per_row as usize)
            .checked_mul(height as usize)
            .ok_or(GpuRenderError::SurfaceTooLarge { width, height })?;
        let mut pixels = Vec::with_capacity(capacity);
        self.unpad_into(mapped, &mut pixels);
        Ok(pixels)
    }

    /// Appends the tightly packed rows of `mapped`, a region of exactly
    /// `buffer_size` bytes, to `pixels`.
    pub(crate) fn unpad_into(&self, mapped: &[u8], pixels: &mut Vec<u8>) {
        let mapped = &mapped[..self.buffer_size as usize];
        if self.padded_bytes_per_row == self.unpadded_bytes_per_row {
            pixels.extend_from_slice(mapped);
            return;
        }
        let row = self.unpadded_bytes_per_row as usize;
        for padded in mapped.chunks_exact(self.padded_bytes_per_row as usize) {
            pixels.extend_from_slice(&padded[..row]);
        }
    }
}

/// One reusable texture/readback-buffer pair behind `GpuRenderer::submit`'s
/// ring, plus the receiver for its currently outstanding `map_async` call
/// (`None` when the slot is free).
pub(crate) struct ReadbackSlot {
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) readback: SlotReadback,
    pub(crate) pending: Option<(
        wgpu::SubmissionIndex,
        mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    )>,
}

/// How a slot's rendered texture reaches its mapped `buffer`.
pub(crate) enum SlotReadback {
    /// Copied as is, rows padded to the copy alignment.
    Rgba8(ReadbackLayout),
    /// Converted into Y, U, and V plane textures by two render passes, whose
    /// rows are then copied, padded, one plane after the other.
    Yuv420p(Box<YuvReadback>),
}

/// The plane textures one yuv420p slot converts into, and where each lands
/// in the slot's readback buffer.
pub(crate) struct YuvReadback {
    /// Samples the slot's RGBA texture.
    pub(crate) bind_group: wgpu::BindGroup,
    /// Y, U, and V, in the order they are packed.
    pub(crate) planes: [YuvPlane; 3],
}

pub(crate) struct YuvPlane {
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) height: u32,
    pub(crate) layout: ReadbackLayout,
    /// Byte offset of this plane's padded rows in the readback buffer.
    pub(crate) offset: u64,
}

impl YuvReadback {
    /// Total size of the three padded planes.
    pub(crate) fn buffer_size(&self) -> u64 {
        let last = &self.planes[2];
        last.offset + last.layout.buffer_size
    }

    /// Packs the three padded planes of a mapped readback buffer into one
    /// tightly packed I420 frame.
    pub(crate) fn unpad(&self, mapped: &[u8]) -> Vec<u8> {
        let capacity = self
            .planes
            .iter()
            .map(|plane| plane.layout.unpadded_bytes_per_row as usize * plane.height as usize)
            .sum();
        let mut pixels = Vec::with_capacity(capacity);
        for plane in &self.planes {
            let start = plane.offset as usize;
            let region = &mapped[start..start + plane.layout.buffer_size as usize];
            plane.layout.unpad_into(region, &mut pixels);
        }
        pixels
    }
}

impl ReadbackSlot {
    pub(crate) fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        yuv_converter: Option<&YuvConverter>,
    ) -> Result<Self, GpuRenderError> {
        let layout = ReadbackLayout::new(width, height)?;
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
        if yuv_converter.is_some() {
            usage |= wgpu::TextureUsages::TEXTURE_BINDING;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Celesta pipelined offscreen frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (readback, buffer_size) = match yuv_converter {
            None => {
                let size = layout.buffer_size;
                (SlotReadback::Rgba8(layout), size)
            }
            Some(converter) => {
                let yuv = converter.readback(device, &view, width, height)?;
                let size = yuv.buffer_size();
                (SlotReadback::Yuv420p(Box::new(yuv)), size)
            }
        };
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Celesta pipelined readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Self {
            texture,
            view,
            buffer,
            width,
            height,
            readback,
            pending: None,
        })
    }

    pub(crate) const fn format(&self) -> ReadbackFormat {
        match self.readback {
            SlotReadback::Rgba8(_) => ReadbackFormat::Rgba8,
            SlotReadback::Yuv420p(_) => ReadbackFormat::Yuv420p,
        }
    }
}
