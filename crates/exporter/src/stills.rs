use crate::control::{CompositionInfo, ExportCancellation, ExportProgress, ExportedFile};
use crate::error::{ExportError, FrameFailure};
use crate::exporter::Exporter;
use crate::options::{BitDepth, ColorConversion, VideoFormat};
use crate::project::{absolutize_fonts, absolutize_layers, visual_only_project};
use crate::range::{ExportRange, resolve_window};
use crate::react::{CompanionProject, ReactRuntimeOptions};
use crate::render::{
    ReportedFontWarnings, ensure_not_cancelled, export_renderer, frame_count, report_font_fallbacks,
};
use celesta_composition::{Rational, Scene, Time};
use celesta_evaluator::{EvaluationError, Evaluator};
use celesta_gpu_renderer::{Color, GpuFrame, GpuRenderOptions, GpuRenderer};
use celesta_project::Project;
use celesta_react_bridge::ProjectFrame;
use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder};
use std::collections::{BTreeMap, HashSet};
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{fs, io};
use strum::VariantArray;

use crate::contact_sheet::Sheet;

/// Which composition frames a still export renders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameSelection {
    /// Exact zero-based composition frames, in this order. Cannot be combined
    /// with an export range.
    Frames(Vec<u64>),
    /// Exact composition frames where negative numbers count back from the
    /// end (`-1` is the last frame). Frames that name the same frame once
    /// resolved are rendered once, in first-seen order. Cannot be combined
    /// with an export range.
    FromEnd(Vec<i64>),
    /// Every `n`th frame of the export range (the whole composition without
    /// one) counted from its first frame, plus its last frame when that is
    /// not already on the interval. Frame numbers stay composition frames.
    Every(u64),
}

/// Lays the selected frames out on one image instead of one image per frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactSheet {
    /// Tiles per row; fewer selected frames than this use one row.
    pub columns: u32,
    /// Tile width in pixels; the height keeps the composition's aspect ratio.
    pub tile_width: u32,
}

impl ContactSheet {
    pub const DEFAULT_COLUMNS: u32 = 5;
    pub const DEFAULT_TILE_WIDTH: u32 = 320;
    /// Neither side of a sheet may exceed this many pixels.
    pub const MAX_SIDE: u32 = 16_384;
    /// Most frames one sheet holds.
    pub const MAX_FRAMES: usize = 400;
}

impl Default for ContactSheet {
    fn default() -> Self {
        Self {
            columns: Self::DEFAULT_COLUMNS,
            tile_width: Self::DEFAULT_TILE_WIDTH,
        }
    }
}

/// What a still export renders and how it is written. The output's extension
/// picks the image format (see [`StillFormat`]).
///
/// Stills are drawn over the same opaque background as an MP4 export, so a
/// frame has no transparent pixels; a JPEG, which has no alpha channel, is
/// flattened onto that background all the same.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StillExport {
    pub frames: FrameSelection,
    /// `None` writes one image per frame.
    pub contact_sheet: Option<ContactSheet>,
    /// Draws a React entry's debug guides (`<DebugOverlay>`, `<DebugBounds>`,
    /// `useDebugGuides()`) into the frames, which otherwise only the editor
    /// preview shows. A JSON project has none to draw.
    pub debug_guides: bool,
    /// JPEG or WebP quality, 1 to 100; higher is sharper and larger. `None`
    /// uses [`Self::DEFAULT_QUALITY`]. PNG is lossless and takes none.
    pub quality: Option<u8>,
}

impl StillExport {
    pub const DEFAULT_QUALITY: u8 = 90;
    pub const MAX_QUALITY: u8 = 100;
}

/// Most frames one export writes as separate image files.
pub const MAX_STILL_FILES: usize = 1_000;

/// The image format a still export writes, chosen by the output's extension.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    strum::Display,
    strum::IntoStaticStr,
    strum::VariantArray,
)]
#[strum(serialize_all = "lowercase")]
pub enum StillFormat {
    /// Lossless 8-bit RGBA.
    #[default]
    Png,
    /// Baseline JPEG, `.jpg` or `.jpeg`.
    Jpeg,
    /// Lossy WebP (VP8).
    Webp,
}

impl StillFormat {
    /// The name `--output-format` takes.
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    /// The format whose extension `path` has, ignoring case.
    pub fn from_extension(path: &Path) -> Option<Self> {
        let extension = path.extension().and_then(OsStr::to_str)?;
        Self::VARIANTS.iter().copied().find(|format| {
            format.as_str().eq_ignore_ascii_case(extension)
                || (*format == Self::Jpeg && extension.eq_ignore_ascii_case("jpg"))
        })
    }

    /// Whether the format takes a quality.
    pub const fn is_lossy(self) -> bool {
        !matches!(self, Self::Png)
    }
}

fn invalid(message: impl Into<String>) -> ExportError {
    ExportError::InvalidSelection(message.into())
}

/// Checks the selection and output name shared by every still layout.
fn validate(frames: &[u64], total: u64, output: &Path) -> Result<(), ExportError> {
    if frames.is_empty() {
        return Err(invalid("select at least one frame"));
    }
    if StillFormat::from_extension(output).is_none() {
        return Err(invalid(
            "still output must use the .png, .jpg, .jpeg or .webp extension",
        ));
    }
    let mut seen = HashSet::new();
    for &frame in frames {
        if frame >= total || frame > i64::MAX as u64 {
            return Err(invalid(format!(
                "frame {frame} is out of range; composition has {total} frames (zero-based)"
            )));
        }
        if !seen.insert(frame) {
            return Err(invalid(format!("duplicate frame {frame}")));
        }
    }
    Ok(())
}

/// The most frames one export renders: one contact sheet's tiles, or
/// separate image files.
fn frame_limit(contact_sheet: Option<ContactSheet>) -> (usize, &'static str) {
    match contact_sheet {
        Some(_) => (ContactSheet::MAX_FRAMES, "a contact sheet holds"),
        None => (MAX_STILL_FILES, "one export writes as separate images"),
    }
}

/// Resolves `selection` to composition frames, at most `limit` of them.
/// `Every` steps through the export range (or the whole composition) and
/// always ends on its last frame.
fn select(
    selection: &FrameSelection,
    range: Option<&ExportRange>,
    duration: Time,
    frame_rate: Rational,
    contact_sheet: Option<ContactSheet>,
) -> Result<Vec<u64>, ExportError> {
    let (limit, holder) = frame_limit(contact_sheet);
    let too_many = |count: u64| {
        invalid(format!(
            "{count} frames selected; {holder} at most {limit} \
             (choose a larger interval or a shorter range)"
        ))
    };
    match selection {
        FrameSelection::Frames(frames) => {
            if range.is_some() {
                return Err(invalid(
                    "a frame selection cannot be combined with an export range",
                ));
            }
            if frames.len() > limit {
                return Err(too_many(frames.len() as u64));
            }
            Ok(frames.clone())
        }
        FrameSelection::FromEnd(frames) => {
            if range.is_some() {
                return Err(invalid(
                    "a frame selection cannot be combined with an export range",
                ));
            }
            let total = frame_count(duration, frame_rate)?;
            let mut resolved = Vec::with_capacity(frames.len());
            for &frame in frames {
                let index = if frame < 0 {
                    i128::from(total) + i128::from(frame)
                } else {
                    i128::from(frame)
                };
                let index = u64::try_from(index)
                    .ok()
                    .filter(|&index| index < total)
                    .ok_or_else(|| {
                        invalid(format!(
                            "frame {frame} is out of range; composition has {total} frames (zero-based)"
                        ))
                    })?;
                if !resolved.contains(&index) {
                    resolved.push(index);
                }
            }
            if resolved.len() > limit {
                return Err(too_many(resolved.len() as u64));
            }
            Ok(resolved)
        }
        FrameSelection::Every(0) => Err(invalid("the frame interval must be at least 1")),
        &FrameSelection::Every(step) => {
            let window = resolve_window(
                range.unwrap_or(&ExportRange::from(Time::ZERO)),
                duration,
                frame_rate,
            )?;
            let first =
                u64::try_from(window.start_frame).map_err(|_| ExportError::TimelineTooLong)?;
            let last = first + window.frames - 1;
            let on_interval = (last - first) / step + 1;
            let count = on_interval + u64::from((last - first) % step != 0);
            if count > limit as u64 {
                return Err(too_many(count));
            }
            let mut frames: Vec<u64> = (0..on_interval).map(|index| first + index * step).collect();
            if frames.last() != Some(&last) {
                frames.push(last);
            }
            Ok(frames)
        }
    }
}

/// Where rendered frames go, and the format they are written in.
struct Destination {
    encoder: Encoder,
    layout: Layout,
}

/// One image per frame, or tiles of one contact sheet.
enum Layout {
    Files(Vec<PathBuf>),
    Sheet(Box<Sheet>),
}

impl Destination {
    /// Validates the selection, its size limit, the quality, and every output
    /// path before anything renders.
    #[allow(clippy::too_many_arguments)]
    fn prepare(
        frames: &[u64],
        total: u64,
        contact_sheet: Option<ContactSheet>,
        (width, height): (u32, u32),
        frame_rate: Rational,
        output: &Path,
        quality: Option<u8>,
        overwrite: bool,
    ) -> Result<Self, ExportError> {
        let encoder = Encoder::new(output, quality)?;
        let Some(contact_sheet) = contact_sheet else {
            encoder.check_size(width, height)?;
            let paths = outputs(frames, total, output, overwrite)?;
            return Ok(Self {
                encoder,
                layout: Layout::Files(paths),
            });
        };
        validate(frames, total, output)?;
        if contact_sheet.columns == 0 || contact_sheet.tile_width == 0 {
            return Err(invalid(
                "contact sheet columns and tile width must be at least 1",
            ));
        }
        if !overwrite && output.exists() {
            return Err(ExportError::OutputExists(output.to_owned()));
        }
        let sheet = Sheet::new(
            width,
            height,
            frames.len(),
            contact_sheet.columns,
            contact_sheet.tile_width,
            ContactSheet::MAX_SIDE,
            frame_rate,
        )
        .map_err(invalid)?;
        encoder.check_size(sheet.width(), sheet.height())?;
        Ok(Self {
            encoder,
            layout: Layout::Sheet(Box::new(sheet)),
        })
    }

    /// Writes or places the rendered `frame`; returns the file it wrote, if any.
    fn accept(
        &mut self,
        index: usize,
        frame: u64,
        rendered: &GpuFrame,
        overwrite: bool,
    ) -> Result<Option<ExportedFile>, ExportError> {
        match &mut self.layout {
            Layout::Files(paths) => {
                self.encoder.write(
                    rendered.width(),
                    rendered.height(),
                    rendered.pixels(),
                    &paths[index],
                    overwrite,
                )?;
                Ok(Some(ExportedFile::Frame {
                    path: paths[index].clone(),
                    frame,
                }))
            }
            Layout::Sheet(sheet) => sheet
                .place(
                    index,
                    frame,
                    rendered.pixels(),
                    (rendered.width(), rendered.height()),
                )
                .map(|()| None)
                .map_err(invalid),
        }
    }

    /// Marks tile `index` as composition frame `frame`, which failed. Only a
    /// contact sheet has a place for it; separate files return false.
    fn place_failed(&mut self, index: usize, frame: u64) -> bool {
        match &mut self.layout {
            Layout::Files(_) => false,
            Layout::Sheet(sheet) => {
                sheet.place_failed(index, frame);
                true
            }
        }
    }

    /// Writes the contact sheet of `frames`; returns the file it wrote, if any.
    fn finish(
        self,
        frames: &[u64],
        output: &Path,
        overwrite: bool,
    ) -> Result<Option<ExportedFile>, ExportError> {
        match self.layout {
            Layout::Files(_) => Ok(None),
            Layout::Sheet(sheet) => {
                self.encoder.write(
                    sheet.width(),
                    sheet.height(),
                    sheet.pixels(),
                    output,
                    overwrite,
                )?;
                Ok(Some(ExportedFile::ContactSheet {
                    path: output.to_owned(),
                    frames: frames.to_vec(),
                    columns: sheet.columns(),
                }))
            }
        }
    }
}

/// Resolves deterministic image paths and validates all selections before rendering.
fn outputs(
    frames: &[u64],
    total: u64,
    output: &Path,
    overwrite: bool,
) -> Result<Vec<PathBuf>, ExportError> {
    validate(frames, total, output)?;
    let mut paths = Vec::new();
    for &frame in frames {
        let path = if frames.len() == 1 {
            output.to_owned()
        } else {
            let mut name = output
                .file_stem()
                .ok_or_else(|| invalid("missing output filename"))?
                .to_os_string();
            name.push(format!("-{frame:06}."));
            name.push(output.extension().unwrap_or_default());
            output.with_file_name(name)
        };
        if !overwrite && path.exists() {
            return Err(ExportError::OutputExists(path));
        }
        paths.push(path);
    }
    Ok(paths)
}

/// Encodes frames in the output's [`StillFormat`].
#[derive(Debug)]
struct Encoder {
    format: StillFormat,
    /// JPEG or WebP quality; PNG ignores it.
    quality: u8,
}

impl Encoder {
    /// WebP stores each side in 14 bits.
    const MAX_WEBP_SIDE: u32 = 16_383;

    /// The encoder for `output`'s extension, checking `quality` against it.
    fn new(output: &Path, quality: Option<u8>) -> Result<Self, ExportError> {
        let format = StillFormat::from_extension(output).ok_or_else(|| {
            invalid("still output must use the .png, .jpg, .jpeg or .webp extension")
        })?;
        let quality = match quality {
            Some(_) if !format.is_lossy() => {
                return Err(invalid(
                    "quality applies to JPEG and WebP output; PNG is lossless",
                ));
            }
            Some(quality) if !(1..=StillExport::MAX_QUALITY).contains(&quality) => {
                return Err(invalid(format!(
                    "quality must be 1 to {}, not {quality}",
                    StillExport::MAX_QUALITY
                )));
            }
            quality => quality.unwrap_or(StillExport::DEFAULT_QUALITY),
        };
        Ok(Self { format, quality })
    }

    /// Fails before rendering when the format cannot hold a `width`×`height` image.
    fn check_size(&self, width: u32, height: u32) -> Result<(), ExportError> {
        let max = Self::MAX_WEBP_SIDE;
        if self.format == StillFormat::Webp && (width > max || height > max) {
            return Err(invalid(format!(
                "a {width}x{height} image is too large for WebP, which holds at most \
                 {max} pixels on each side (write PNG or JPEG, or make the contact sheet smaller)"
            )));
        }
        Ok(())
    }

    /// Writes straight-alpha RGBA `pixels` to `path` through a temporary
    /// file beside it, so a failed export never leaves a partial image.
    fn write(
        &self,
        width: u32,
        height: u32,
        pixels: &[u8],
        path: &Path,
        overwrite: bool,
    ) -> Result<(), ExportError> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| ExportError::Io {
            operation: "create output directory",
            source,
        })?;
        let mut file =
            tempfile::NamedTempFile::new_in(parent).map_err(|source| ExportError::Io {
                operation: "create temporary image",
                source,
            })?;
        self.encode(file.as_file_mut(), width, height, pixels)?;
        if overwrite {
            file.persist(path)
        } else {
            file.persist_noclobber(path)
        }
        .map_err(|e| {
            if e.error.kind() == io::ErrorKind::AlreadyExists {
                ExportError::OutputExists(path.to_owned())
            } else {
                ExportError::Io {
                    operation: "publish image",
                    source: e.error,
                }
            }
        })?;
        Ok(())
    }

    fn encode(
        &self,
        file: &mut fs::File,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<(), ExportError> {
        let failed = |operation: &'static str| {
            move |error: Box<dyn std::error::Error + Send + Sync>| ExportError::Io {
                operation,
                source: io::Error::other(error),
            }
        };
        match self.format {
            StillFormat::Png => {
                let mut encoder = png::Encoder::new(file, width, height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder
                    .write_header()
                    .map_err(|e| failed("encode PNG")(e.into()))?;
                writer
                    .write_image_data(pixels)
                    .map_err(|e| failed("encode PNG")(e.into()))?;
                writer.finish().map_err(|e| failed("finish PNG")(e.into()))
            }
            StillFormat::Jpeg => {
                let rgb = flatten(pixels, GpuRenderOptions::default().background);
                let mut writer = io::BufWriter::new(file);
                JpegEncoder::new_with_quality(&mut writer, self.quality)
                    .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
                    .map_err(|e| failed("encode JPEG")(e.into()))?;
                writer.flush().map_err(|source| ExportError::Io {
                    operation: "write JPEG",
                    source,
                })
            }
            StillFormat::Webp => {
                let encoded = webp::Encoder::from_rgba(pixels, width, height)
                    .encode_simple(false, f32::from(self.quality))
                    .map_err(|e| failed("encode WebP")(format!("{e:?}").into()))?;
                file.write_all(&encoded).map_err(|source| ExportError::Io {
                    operation: "write WebP",
                    source,
                })
            }
        }
    }
}

/// Straight-alpha RGBA `pixels` composited over `background`, as RGB.
fn flatten(pixels: &[u8], background: Color) -> Vec<u8> {
    let behind = [background.red, background.green, background.blue];
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| {
            let alpha = u16::from(pixel[3]);
            std::array::from_fn::<u8, 3, _>(|channel| {
                ((u16::from(pixel[channel]) * alpha
                    + u16::from(behind[channel]) * (255 - alpha)
                    + 127)
                    / 255) as u8
            })
        })
        .collect()
}

/// Renders the scene `scene_at` builds for each of `frames` into
/// `destination`, then finishes it. Every failure names its frame. A contact
/// sheet carries on past a frame that cannot be evaluated or rendered, with a
/// failed tile in its place, and once the sheet is written fails with
/// [`ExportError::FramesFailed`] listing each one; separate files stop at the
/// first failure.
#[allow(clippy::too_many_arguments)]
fn render_stills(
    frames: &[u64],
    mut destination: Destination,
    renderer: &mut GpuRenderer,
    output: &Path,
    overwrite: bool,
    cancellation: &ExportCancellation,
    progress: &mut impl FnMut(ExportProgress),
    mut scene_at: impl FnMut(u64) -> Result<Scene, ExportError>,
) -> Result<(), ExportError> {
    let mut fallbacks = ReportedFontWarnings::default();
    let mut failures = Vec::new();
    for (index, &frame) in frames.iter().enumerate() {
        ensure_not_cancelled(cancellation)?;
        progress(ExportProgress::Rendering {
            frame: index as u64 + 1,
            total: frames.len() as u64,
        });
        let rendered =
            scene_at(frame).and_then(|scene| renderer.render(&scene).map_err(ExportError::Render));
        report_font_fallbacks(renderer, &mut fallbacks, progress);
        let file = match rendered {
            Ok(rendered) => destination.accept(index, frame, &rendered, overwrite),
            Err(error) if error.is_frame_scoped() && destination.place_failed(index, frame) => {
                failures.push(FrameFailure { frame, error });
                Ok(None)
            }
            Err(error) => Err(error),
        }
        .map_err(|error| error.at_frame(frame))?;
        if let Some(file) = file {
            progress(ExportProgress::Wrote(file));
        }
    }
    if let Some(file) = destination.finish(frames, output, overwrite)? {
        progress(ExportProgress::Wrote(file));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(ExportError::FramesFailed {
            path: output.to_owned(),
            frames: frames.len(),
            failures,
        })
    }
}

impl Exporter {
    /// Exports exact zero-based project frames without video encoding or audio mixing.
    /// Multiple selections append `-000090` style frame numbers to the output stem.
    pub fn export_project_stills(
        &self,
        project: &Project,
        asset_root: &Path,
        frames: &[u64],
        output: &Path,
        progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        self.export_project_stills_with(
            project,
            asset_root,
            &StillExport {
                frames: FrameSelection::Frames(frames.to_vec()),
                contact_sheet: None,
                debug_guides: false,
                quality: None,
            },
            output,
            &ExportCancellation::default(),
            progress,
        )
    }

    /// Exports project frames chosen by `request` as separate images or one
    /// contact sheet. An export range is only allowed with
    /// [`FrameSelection::Every`]. `cancellation` is checked before each frame;
    /// stills already written stay in place.
    pub fn export_project_stills_with(
        &self,
        project: &Project,
        asset_root: &Path,
        request: &StillExport,
        output: &Path,
        cancellation: &ExportCancellation,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let duration = project.effective_duration().map_err(ExportError::Time)?;
        let frame_rate = project.settings.frame_rate;
        let total = frame_count(duration, frame_rate)?;
        progress(ExportProgress::Composition(CompositionInfo {
            width: project.settings.width,
            height: project.settings.height,
            frame_rate,
            frames: total,
        }));
        let frames = select(
            &request.frames,
            self.options.range.as_ref(),
            duration,
            frame_rate,
            request.contact_sheet,
        )?;
        let destination = Destination::prepare(
            &frames,
            total,
            request.contact_sheet,
            (project.settings.width, project.settings.height),
            frame_rate,
            output,
            request.quality,
            self.options.overwrite,
        )?;
        let evaluator = Evaluator::new(project).map_err(ExportError::Evaluation)?;
        // Stills keep an MP4 export's opaque background, read back as
        // 8-bit RGBA.
        let mut renderer = export_renderer(
            asset_root,
            frame_rate,
            VideoFormat::Mp4,
            ColorConversion::Encoder,
            BitDepth::Eight,
            self.options.render_quality,
            self.options.driver,
        )?;
        render_stills(
            &frames,
            destination,
            &mut renderer,
            output,
            self.options.overwrite,
            cancellation,
            &mut progress,
            |frame| {
                let time = Time::frames(frame as i64, frame_rate).map_err(ExportError::Time)?;
                evaluator.scene_at(time).map_err(ExportError::Evaluation)
            },
        )
    }

    /// Exports selected React frames with one runtime/prepare call and shared font/asset caches.
    pub fn export_react_stills(
        &self,
        entry: &Path,
        runtime: &ReactRuntimeOptions,
        companion: Option<CompanionProject<'_>>,
        frames: &[u64],
        output: &Path,
        progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        self.export_react_stills_with(
            entry,
            runtime,
            companion,
            &StillExport {
                frames: FrameSelection::Frames(frames.to_vec()),
                contact_sheet: None,
                debug_guides: false,
                quality: None,
            },
            output,
            &ExportCancellation::default(),
            progress,
        )
    }

    /// [`Self::export_react_stills`] for frames chosen by `request`, as separate
    /// images or one contact sheet. `cancellation` is checked before each
    /// frame, as in [`Self::export_project_stills_with`].
    #[allow(clippy::too_many_arguments)]
    pub fn export_react_stills_with(
        &self,
        entry: &Path,
        runtime: &ReactRuntimeOptions,
        companion: Option<CompanionProject<'_>>,
        request: &StillExport,
        output: &Path,
        cancellation: &ExportCancellation,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let asset_root = entry.parent().unwrap_or_else(|| Path::new("."));
        let asset_root = &fs::canonicalize(asset_root).unwrap_or_else(|_| asset_root.to_owned());
        let project_owned = companion.map(|c| {
            (
                c.project,
                fs::canonicalize(c.project_asset_root)
                    .unwrap_or_else(|_| c.project_asset_root.to_owned()),
            )
        });
        let project = project_owned.as_ref().map(|(p, root)| (*p, root.as_path()));
        let mut bridge = runtime.spawn(entry, project)?;
        bridge.set_debug_guides(request.debug_guides);
        let metadata = bridge.metadata().clone();
        progress(ExportProgress::ReactCompositions {
            selected: metadata.composition_id.clone(),
            compositions: metadata.compositions.clone(),
        });
        progress(ExportProgress::Composition(CompositionInfo {
            width: metadata.width,
            height: metadata.height,
            frame_rate: metadata.frame_rate,
            frames: metadata.duration_in_frames,
        }));
        let duration = Time::frames(
            i64::try_from(metadata.duration_in_frames).map_err(|_| ExportError::TimelineTooLong)?,
            metadata.frame_rate,
        )
        .map_err(ExportError::Time)?;
        let frames = select(
            &request.frames,
            self.options.range.as_ref(),
            duration,
            metadata.frame_rate,
            request.contact_sheet,
        )?;
        let destination = Destination::prepare(
            &frames,
            metadata.duration_in_frames,
            request.contact_sheet,
            (metadata.width, metadata.height),
            metadata.frame_rate,
            output,
            request.quality,
            self.options.overwrite,
        )?;
        let filtered_project = project.map(|(project, _)| visual_only_project(project));
        let project_evaluator = filtered_project
            .as_ref()
            .map(Evaluator::new)
            .transpose()
            .map_err(ExportError::Evaluation)?;
        let project_asset_root = project.map(|(_, asset_root)| asset_root);
        let project_fonts = project_evaluator
            .as_ref()
            .map(|evaluator| -> Result<_, EvaluationError> {
                let mut fonts = evaluator.scene_at(Time::ZERO)?.fonts;
                if let Some(asset_root) = project_asset_root {
                    absolutize_fonts(&mut fonts, asset_root);
                }
                Ok(fonts)
            })
            .transpose()
            .map_err(ExportError::Evaluation)?
            .unwrap_or_default();

        // Stills keep an MP4 export's opaque background, read back as
        // 8-bit RGBA.
        let mut renderer = export_renderer(
            asset_root,
            metadata.frame_rate,
            VideoFormat::Mp4,
            ColorConversion::Encoder,
            BitDepth::Eight,
            self.options.render_quality,
            self.options.driver,
        )?;
        render_stills(
            &frames,
            destination,
            &mut renderer,
            output,
            self.options.overwrite,
            cancellation,
            &mut progress,
            |frame| {
                let time =
                    Time::frames(frame as i64, metadata.frame_rate).map_err(ExportError::Time)?;
                let project_frame = project_evaluator
                    .as_ref()
                    .zip(filtered_project.as_ref())
                    .map(
                        |(evaluator, filtered_project)| -> Result<_, EvaluationError> {
                            let mut scene = evaluator.scene_at(time)?;
                            let mut tracks = BTreeMap::new();
                            for track in &filtered_project.tracks {
                                let mut layers = evaluator.layers_for_track(&track.id, time)?;
                                if let Some(asset_root) = project_asset_root {
                                    absolutize_layers(&mut layers, asset_root);
                                }
                                tracks.insert(track.id.clone(), layers);
                            }
                            if let Some(asset_root) = project_asset_root {
                                absolutize_layers(&mut scene.layers, asset_root);
                            }
                            Ok((scene.layers, tracks))
                        },
                    )
                    .transpose()
                    .map_err(ExportError::Evaluation)?;
                let evaluation = bridge
                    .evaluate_at(
                        time,
                        project_frame.as_ref().map(|(layers, tracks)| ProjectFrame {
                            layers: layers.as_slice(),
                            tracks,
                        }),
                    )
                    .map_err(ExportError::React)?;
                let mut scene = evaluation.scene;
                scene.fonts.extend(project_fonts.iter().cloned());
                Ok(scene)
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_first_last_and_out_of_range_frames() {
        assert_eq!(
            outputs(&[0, 89], 90, Path::new("check.png"), false).unwrap(),
            vec![
                PathBuf::from("check-000000.png"),
                PathBuf::from("check-000089.png")
            ]
        );
        assert!(
            outputs(&[90], 90, Path::new("check.png"), false)
                .unwrap_err()
                .to_string()
                .contains("frame 90 is out of range")
        );
        assert!(outputs(&[0], 0, Path::new("check.png"), false).is_err());
        assert!(outputs(&[u64::MAX], u64::MAX, Path::new("check.png"), false).is_err());
    }

    #[test]
    fn rejects_empty_duplicate_and_non_png_selections() {
        for frames in [&[][..], &[1, 1][..]] {
            assert!(outputs(frames, 90, Path::new("check.png"), false).is_err());
        }
        assert!(outputs(&[0], 90, Path::new("check.mp4"), false).is_err());
    }

    #[test]
    fn every_steps_through_the_range_and_ends_on_its_last_frame() {
        let rate = Rational::new(10, 1);
        let duration = Time::new(10, 1); // frames 0..=99
        let every = |step, range: Option<ExportRange>| {
            select(
                &FrameSelection::Every(step),
                range.as_ref(),
                duration,
                rate,
                None,
            )
            .unwrap()
        };
        assert_eq!(every(30, None), [0, 30, 60, 90, 99]);
        assert_eq!(every(33, None), [0, 33, 66, 99]);
        assert_eq!(every(1_000, None), [0, 99]);
        // --from 2.05 snaps down to frame 20; --to 5 is exclusive (last frame 49).
        assert_eq!(
            every(
                10,
                Some(ExportRange::new(Time::new(205, 100), Time::new(5, 1)))
            ),
            [20, 30, 40, 49]
        );
        assert_eq!(
            every(
                10,
                Some(ExportRange::new(Time::new(2, 1), Time::new(21, 10)))
            ),
            [20]
        );
        assert!(select(&FrameSelection::Every(0), None, duration, rate, None).is_err());
        assert!(
            select(
                &FrameSelection::Frames(vec![0]),
                Some(&ExportRange::from(Time::ZERO)),
                duration,
                rate,
                None
            )
            .is_err()
        );
    }

    fn from_end(frames: &[i64], total: i64) -> Result<Vec<u64>, ExportError> {
        let rate = Rational::new(30, 1);
        select(
            &FrameSelection::FromEnd(frames.to_vec()),
            None,
            Time::frames(total, rate).unwrap(),
            rate,
            None,
        )
    }

    #[test]
    fn from_end_counts_back_from_the_last_frame() {
        assert_eq!(from_end(&[0, -1, -30], 90).unwrap(), [0, 89, 60]);
    }

    #[test]
    fn from_end_rejects_frames_before_the_start() {
        let error = from_end(&[-91], 90).unwrap_err();
        assert!(matches!(error, ExportError::InvalidSelection(_)), "{error}");
        assert!(error.to_string().contains("out of range"), "{error}");
        assert!(
            from_end(&[90], 90)
                .unwrap_err()
                .to_string()
                .contains("out of range")
        );
    }

    #[test]
    fn from_end_drops_repeats() {
        assert_eq!(from_end(&[89, -1], 90).unwrap(), [89]);
        assert_eq!(from_end(&[0, -1], 1).unwrap(), [0]);
    }

    #[test]
    fn from_end_cannot_combine_with_a_range() {
        let rate = Rational::new(30, 1);
        let error = select(
            &FrameSelection::FromEnd(vec![-1]),
            Some(&ExportRange::from(Time::ZERO)),
            Time::frames(90, rate).unwrap(),
            rate,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("cannot be combined"), "{error}");
    }

    #[test]
    fn limits_how_many_frames_one_export_renders() {
        let rate = Rational::new(30, 1);
        let duration = Time::new(120, 1); // 3600 frames
        let error = select(&FrameSelection::Every(1), None, duration, rate, None).unwrap_err();
        assert!(
            error.to_string().contains("3600 frames selected"),
            "{error}"
        );
        assert!(select(&FrameSelection::Every(3), None, duration, rate, None).is_err());
        // 0, 4, …, 3596 and the last frame 3599.
        assert_eq!(
            select(&FrameSelection::Every(4), None, duration, rate, None)
                .unwrap()
                .len(),
            901
        );
        let sheet = Some(ContactSheet::default());
        assert!(select(&FrameSelection::Every(10), None, duration, rate, sheet).is_ok());
        let error = select(&FrameSelection::Every(9), None, duration, rate, sheet).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("401 frames selected; a contact sheet holds at most 400"),
            "{error}"
        );
    }

    #[test]
    fn the_output_extension_selects_the_still_format() {
        let format = |name: &str| StillFormat::from_extension(Path::new(name));
        assert_eq!(format("a.png"), Some(StillFormat::Png));
        assert_eq!(format("a.JPG"), Some(StillFormat::Jpeg));
        assert_eq!(format("a.jpeg"), Some(StillFormat::Jpeg));
        assert_eq!(format("a.WebP"), Some(StillFormat::Webp));
        assert_eq!(format("a.gif"), None);
        assert_eq!(format("a"), None);
    }

    #[test]
    fn separate_files_keep_the_output_extension() {
        assert_eq!(
            outputs(&[0, 89], 90, Path::new("thumb.jpg"), false).unwrap(),
            [
                PathBuf::from("thumb-000000.jpg"),
                PathBuf::from("thumb-000089.jpg")
            ]
        );
        assert_eq!(
            outputs(&[3, 4], 90, Path::new("thumb.WEBP"), false).unwrap(),
            [
                PathBuf::from("thumb-000003.WEBP"),
                PathBuf::from("thumb-000004.WEBP")
            ]
        );
        let error = outputs(&[0], 90, Path::new("check.gif"), false).unwrap_err();
        assert!(
            error.to_string().contains(".png, .jpg, .jpeg or .webp"),
            "{error}"
        );
    }

    #[test]
    fn quality_applies_to_jpeg_and_webp_only() {
        let encoder = |name: &str, quality| Encoder::new(Path::new(name), quality);
        assert_eq!(
            encoder("a.jpg", None).unwrap().quality,
            StillExport::DEFAULT_QUALITY
        );
        assert_eq!(encoder("a.webp", Some(40)).unwrap().quality, 40);
        assert!(encoder("a.jpg", Some(1)).is_ok());
        assert!(encoder("a.jpg", Some(100)).is_ok());
        for quality in [0, 101] {
            let error = encoder("a.jpg", Some(quality)).unwrap_err();
            assert!(error.to_string().contains("1 to 100"), "{error}");
        }
        let error = encoder("a.png", Some(80)).unwrap_err();
        assert!(error.to_string().contains("PNG is lossless"), "{error}");
        assert!(encoder("a.png", None).is_ok());
    }

    #[test]
    fn webp_rejects_images_wider_than_it_can_hold() {
        let encoder = |name: &str| Encoder::new(Path::new(name), None).unwrap();
        assert!(encoder("a.webp").check_size(16_383, 16_383).is_ok());
        let error = encoder("a.webp").check_size(16_384, 9).unwrap_err();
        assert!(error.to_string().contains("too large for WebP"), "{error}");
        assert!(encoder("a.jpg").check_size(16_384, 9).is_ok());
        assert!(encoder("a.png").check_size(16_384, 9).is_ok());
    }

    #[test]
    fn jpeg_flattens_transparency_onto_the_export_background() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.jpg");
        // An opaque red pixel and a fully transparent white one, 8x8 each so
        // chroma subsampling keeps them apart.
        let mut pixels = Vec::new();
        for _ in 0..8 {
            for x in 0..16 {
                pixels.extend_from_slice(if x < 8 {
                    &[255, 0, 0, 255]
                } else {
                    &[255, 255, 255, 0]
                });
            }
        }
        Encoder::new(&path, Some(100))
            .unwrap()
            .write(16, 8, &pixels, &path, false)
            .unwrap();
        let decoded = image::load_from_memory(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(decoded.color(), image::ColorType::Rgb8);
        let decoded = decoded.to_rgb8();
        let near = |pixel: &image::Rgb<u8>, expected: [u8; 3]| {
            pixel
                .0
                .iter()
                .zip(expected)
                .all(|(&value, expected)| value.abs_diff(expected) <= 4)
        };
        let background = GpuRenderOptions::default().background;
        assert!(near(decoded.get_pixel(2, 4), [255, 0, 0]));
        assert!(
            near(
                decoded.get_pixel(13, 4),
                [background.red, background.green, background.blue]
            ),
            "{:?}",
            decoded.get_pixel(13, 4)
        );
    }

    #[test]
    fn webp_is_lossy_at_the_requested_quality() {
        let dir = tempfile::tempdir().unwrap();
        // A noisy frame, so lower quality visibly saves bytes.
        let pixels: Vec<u8> = (0u32..64 * 64)
            .flat_map(|index| {
                let value = (index.wrapping_mul(2_654_435_761) >> 24) as u8;
                [value, value.wrapping_mul(3), 255 - value, 255]
            })
            .collect();
        let write = |quality| {
            let path = dir.path().join(format!("q{quality}.webp"));
            Encoder::new(&path, Some(quality))
                .unwrap()
                .write(64, 64, &pixels, &path, false)
                .unwrap();
            fs::read(path).unwrap()
        };
        let (low, high) = (write(10), write(100));
        for bytes in [&low, &high] {
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WEBP");
            let image = webp::Decoder::new(bytes).decode().unwrap();
            assert_eq!((image.width(), image.height()), (64, 64));
        }
        // VP8 (lossy), not VP8L (lossless).
        assert_eq!(&high[12..16], b"VP8 ");
        assert!(low.len() < high.len(), "{} >= {}", low.len(), high.len());
    }

    #[test]
    fn checks_every_output_before_rendering() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("check.png");
        let existing = dir.path().join("check-000089.png");
        fs::write(&existing, b"keep").unwrap();
        assert!(
            matches!(outputs(&[0, 89], 90, &output, false), Err(ExportError::OutputExists(path)) if path == existing)
        );
        assert!(outputs(&[0, 89], 90, &output, true).is_ok());
        assert_eq!(fs::read(existing).unwrap(), b"keep");
    }
}
