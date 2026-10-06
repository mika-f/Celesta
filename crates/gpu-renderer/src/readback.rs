use crate::BYTES_PER_PIXEL;
use crate::error::GpuRenderError;
use crate::types::{GpuFrame, ReadbackFormat};
use crate::yuv::YuvConverter;
use std::sync::mpsc;
use std::thread::JoinHandle;

#[derive(Clone, Copy)]
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
/// ring.
pub(crate) struct ReadbackSlot {
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) readback: SlotReadback,
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
        })
    }

    pub(crate) const fn format(&self) -> ReadbackFormat {
        match self.readback {
            SlotReadback::Rgba8(_) => ReadbackFormat::Rgba8,
            SlotReadback::Yuv420p(_) => ReadbackFormat::Yuv420p,
        }
    }

    /// The readback of the frame just submitted to it, for the
    /// [`ReadbackWorker`], which maps its buffer once `submission` is done.
    pub(crate) fn readback(&self, submission: wgpu::SubmissionIndex) -> Readback {
        // Each plane's padded rows, packed one after the other.
        let planes = match &self.readback {
            SlotReadback::Rgba8(layout) => vec![(0, *layout, self.height)],
            SlotReadback::Yuv420p(yuv) => yuv
                .planes
                .iter()
                .map(|plane| (plane.offset, plane.layout, plane.height))
                .collect(),
        };
        Readback {
            buffer: self.buffer.clone(),
            submission,
            planes,
            width: self.width,
            height: self.height,
            format: self.format(),
        }
    }
}

/// A submitted frame waiting in its slot's readback buffer.
pub(crate) struct Readback {
    buffer: wgpu::Buffer,
    submission: wgpu::SubmissionIndex,
    /// Each plane's offset in `buffer`, its rows, and its height, in the
    /// order they are packed into the frame.
    planes: Vec<(u64, ReadbackLayout, u32)>,
    width: u32,
    height: u32,
    format: ReadbackFormat,
}

impl Readback {
    /// Waits for the frame, copies its tightly packed rows out of the
    /// mapped buffer, and unmaps it for the slot's next frame.
    fn finish(self, device: &wgpu::Device) -> Result<GpuFrame, GpuRenderError> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let slice = self.buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        // Wait for this frame's own submission only. Waiting for the most
        // recent one (`wait_indefinitely`) would also block on every
        // younger frame still in flight.
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(self.submission),
                timeout: None,
            })
            .map_err(GpuRenderError::Poll)?;
        receiver
            .recv()
            .map_err(|_| GpuRenderError::MapCallbackDropped)?
            .map_err(GpuRenderError::Map)?;
        let mapped = slice.get_mapped_range().map_err(GpuRenderError::MapRange)?;
        let capacity = self
            .planes
            .iter()
            .map(|(_, layout, height)| layout.unpadded_bytes_per_row as usize * *height as usize)
            .sum();
        let mut pixels = Vec::with_capacity(capacity);
        for (offset, layout, _) in &self.planes {
            let start = *offset as usize;
            layout.unpad_into(
                &mapped[start..start + layout.buffer_size as usize],
                &mut pixels,
            );
        }
        drop(mapped);
        self.buffer.unmap();
        Ok(GpuFrame {
            width: self.width,
            height: self.height,
            format: self.format,
            pixels,
        })
    }
}

/// Finishes `GpuRenderer::submit`'s frames on a thread of its own, in the
/// order they were sent: waits for each, then copies it out of its mapped
/// readback buffer. A frame's copy (8 MB of RGBA at 1080p, into memory the
/// system has to fault in page by page) then overlaps with the caller
/// preparing the next frames instead of adding to each of them.
pub(crate) struct ReadbackWorker {
    readbacks: Option<mpsc::Sender<Readback>>,
    frames: mpsc::Receiver<Result<GpuFrame, GpuRenderError>>,
    thread: Option<JoinHandle<()>>,
}

impl ReadbackWorker {
    pub(crate) fn new(device: wgpu::Device) -> Self {
        let (readbacks, pending) = mpsc::channel::<Readback>();
        let (finished, frames) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("celesta-readback".to_owned())
            .spawn(move || {
                for readback in pending {
                    if finished.send(readback.finish(&device)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawning the readback thread");
        Self {
            readbacks: Some(readbacks),
            frames,
            thread: Some(thread),
        }
    }

    pub(crate) fn send(&self, readback: Readback) {
        self.readbacks
            .as_ref()
            .expect("only dropping takes the sender")
            .send(readback)
            .expect("the readback thread runs until dropped");
    }

    /// The oldest sent frame not yet received, once it is ready.
    pub(crate) fn receive(&self) -> Result<GpuFrame, GpuRenderError> {
        self.frames
            .recv()
            .expect("the readback thread answers every readback")
    }
}

impl Drop for ReadbackWorker {
    fn drop(&mut self) {
        // Ends the thread's loop once it has finished what it was sent.
        self.readbacks = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
