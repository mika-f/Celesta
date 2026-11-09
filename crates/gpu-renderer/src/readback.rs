use crate::BYTES_PER_PIXEL;
use crate::error::GpuRenderError;
use crate::quantize::QuantizeOutput;
use crate::texture::{CanvasTexture, canvas_texture};
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
    /// What the frame is drawn onto, in `canvas_format`.
    pub(crate) canvas: CanvasTexture,
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) readback: SlotReadback,
    pub(crate) format: ReadbackFormat,
}

/// How a slot's rendered frame reaches its mapped `buffer`.
pub(crate) enum SlotReadback {
    /// Quantized into an RGBA texture (`Quantizer`), whose rows are then
    /// copied, padded to the copy alignment.
    Rgba(Box<RgbaReadback>),
    /// Converted into Y, U, and V plane textures by two render passes, whose
    /// rows are then copied, padded, one plane after the other.
    Yuv(Box<YuvReadback>),
}

/// The texture one RGBA slot quantizes its frame into: `Rgba8Unorm`, or
/// `Rgba16Uint` for [`ReadbackFormat::Rgba16`] and
/// [`ReadbackFormat::Rgba16Straight`].
pub(crate) struct RgbaReadback {
    pub(crate) texture: wgpu::Texture,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) view: wgpu::TextureView,
    pub(crate) layout: ReadbackLayout,
    /// Whether the color is premultiplied or divided by alpha.
    pub(crate) output: QuantizeOutput,
}

/// The plane textures one yuv420p slot converts into, and where each lands
/// in the slot's readback buffer.
pub(crate) struct YuvReadback {
    /// Samples the slot's canvas.
    pub(crate) bind_group: wgpu::BindGroup,
    /// Y, then U and V, or the interleaved UV, in the order they are packed.
    pub(crate) planes: Vec<YuvPlane>,
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
    /// Total size of the padded planes.
    pub(crate) fn buffer_size(&self) -> u64 {
        let last = self.planes.last().expect("a slot has planes");
        last.offset + last.layout.buffer_size
    }
}

impl ReadbackSlot {
    /// A slot that reads frames back in `format`, drawn onto a canvas of
    /// `canvas_format` bound through `texture_layout`. A YUV `format` takes
    /// its converter, `yuv_converter`.
    pub(crate) fn new(
        device: &wgpu::Device,
        texture_layout: &wgpu::BindGroupLayout,
        canvas_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        format: ReadbackFormat,
        yuv_converter: Option<&YuvConverter>,
    ) -> Result<Self, GpuRenderError> {
        let canvas = canvas_texture(device, texture_layout, canvas_format, width, height);
        let (readback, buffer_size) = match yuv_converter {
            None => {
                let (texture_format, output) = match format {
                    ReadbackFormat::Rgba16 => (
                        wgpu::TextureFormat::Rgba16Uint,
                        QuantizeOutput::Premultiplied,
                    ),
                    ReadbackFormat::Rgba16Straight => {
                        (wgpu::TextureFormat::Rgba16Uint, QuantizeOutput::Straight)
                    }
                    _ => (
                        wgpu::TextureFormat::Rgba8Unorm,
                        QuantizeOutput::Premultiplied,
                    ),
                };
                let bytes_per_pixel = texture_format
                    .block_copy_size(None)
                    .unwrap_or(BYTES_PER_PIXEL);
                let layout = ReadbackLayout::with_bytes_per_pixel(width, height, bytes_per_pixel)?;
                let texture = readback_texture(device, texture_format, width, height);
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let size = layout.buffer_size;
                let rgba = RgbaReadback {
                    texture,
                    format: texture_format,
                    view,
                    layout,
                    output,
                };
                (SlotReadback::Rgba(Box::new(rgba)), size)
            }
            Some(converter) => {
                let yuv = converter.readback(device, &canvas.view, width, height)?;
                let size = yuv.buffer_size();
                (SlotReadback::Yuv(Box::new(yuv)), size)
            }
        };
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Celesta pipelined readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Self {
            canvas,
            buffer,
            width,
            height,
            readback,
            format,
        })
    }

    pub(crate) const fn format(&self) -> ReadbackFormat {
        self.format
    }

    /// The readback of the frame just submitted to it, for the
    /// [`ReadbackWorker`], which maps its buffer once `submission` is done.
    pub(crate) fn readback(&self, submission: wgpu::SubmissionIndex) -> Readback {
        // Each plane's padded rows, packed one after the other.
        let planes = match &self.readback {
            SlotReadback::Rgba(rgba) => vec![(0, rgba.layout, self.height)],
            SlotReadback::Yuv(yuv) => yuv
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

/// A `width`x`height` texture a frame is quantized into for RGBA readback.
pub(crate) fn rgba_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    readback_texture(device, wgpu::TextureFormat::Rgba8Unorm, width, height)
}

/// A `width`x`height` texture of `format` a frame is quantized into for
/// readback. Its texels are copied out in the device's byte order, little
/// endian on every platform wgpu runs on.
fn readback_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Celesta RGBA readback frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
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
    /// mapped buffer, and unmaps it for the slot's next frame, also when
    /// reading it failed once the mapping was asked for.
    fn finish(self, device: &wgpu::Device) -> Result<GpuFrame, GpuRenderError> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let slice = self.buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        // Wait for this frame's own submission only. Waiting for the most
        // recent one (`wait_indefinitely`) would also block on every
        // younger frame still in flight.
        let waited = device.poll(wgpu::PollType::Wait {
            submission_index: Some(self.submission.clone()),
            timeout: None,
        });
        if let Err(error) = waited {
            // Cancels the mapping still pending.
            self.buffer.unmap();
            return Err(GpuRenderError::Poll(error));
        }
        // A mapping that failed leaves the buffer unmapped.
        receiver
            .recv()
            .map_err(|_| GpuRenderError::MapCallbackDropped)?
            .map_err(GpuRenderError::Map)?;
        let pixels = self.unpack(&slice);
        self.buffer.unmap();
        Ok(GpuFrame {
            width: self.width,
            height: self.height,
            format: self.format,
            pixels: pixels?,
        })
    }

    /// The tightly packed rows of the mapped buffer's planes.
    fn unpack(&self, slice: &wgpu::BufferSlice<'_>) -> Result<Vec<u8>, GpuRenderError> {
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
        Ok(pixels)
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
    /// How many readbacks the thread has finished.
    #[cfg(test)]
    pub(crate) finished: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl ReadbackWorker {
    pub(crate) fn new(device: wgpu::Device) -> Result<Self, GpuRenderError> {
        let (readbacks, pending) = mpsc::channel::<Readback>();
        let (sender, frames) = mpsc::channel();
        #[cfg(test)]
        let finished = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        #[cfg(test)]
        let counted = finished.clone();
        let thread = std::thread::Builder::new()
            .name("celesta-readback".to_owned())
            .spawn(move || {
                for readback in pending {
                    let frame = readback.finish(&device);
                    #[cfg(test)]
                    counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if sender.send(frame).is_err() {
                        break;
                    }
                }
            })
            .map_err(GpuRenderError::ReadbackThread)?;
        Ok(Self {
            readbacks: Some(readbacks),
            frames,
            thread: Some(thread),
            #[cfg(test)]
            finished,
        })
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
