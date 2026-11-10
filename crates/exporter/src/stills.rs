use crate::control::{CompositionInfo, ExportCancellation, ExportProgress, ExportedFile};
use crate::encode::{IMAGE_STAGE, encoder_pixels, readback_for};
use crate::error::{ExportError, FrameFailure};
use crate::exporter::Exporter;
use crate::media::with_builtin;
use crate::options::OutputChoice;
use crate::project::{absolutize_fonts, absolutize_layers};
use crate::range::{ExportRange, resolve_window};
use crate::react::{CompanionProject, ReactRuntimeOptions};
use crate::render::{
    ReportedWarnings, configure_renderer, ensure_not_cancelled, frame_count, report_render_warnings,
};
use crate::still_output::{StillFiles, StillOutput};
use celesta_composition::{Rational, Scene, Time};
use celesta_evaluator::{EvaluationError, Evaluator};
use celesta_gpu_renderer::{GpuFrame, GpuRenderer, ReadbackFormat};
use celesta_media::{MediaError, MediaRegistry, VideoEncoderOpened};
use celesta_plugin_host::RESERVED_PLUGIN_ID;
use celesta_project::Project;
use celesta_react_bridge::ProjectFrame;
use std::collections::{BTreeMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
/// picks the image format (see [`StillFormat`]): the built-in image encoder
/// for it, unless [`ExportOptions::output`](crate::ExportOptions::output)
/// names another, with that choice's options.
///
/// Stills are drawn over the same opaque background as an MP4 export, so a
/// frame has no transparent pixels, unless the encoder keeps alpha (PNG and
/// WebP with `transparent`); a JPEG has no alpha channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StillExport {
    pub frames: FrameSelection,
    /// `None` writes one image per frame.
    pub contact_sheet: Option<ContactSheet>,
    /// Draws a React entry's debug guides (`<DebugOverlay>`, `<DebugBounds>`,
    /// `useDebugGuides()`) into the frames, which otherwise only the editor
    /// preview shows. A JSON project has none to draw.
    pub debug_guides: bool,
    /// JPEG or WebP quality, 1 to 100; higher is sharper and larger: the
    /// encoder's `quality` option, which the output choice must not also
    /// give. `None` leaves it to the choice, or the encoder's default,
    /// [`Self::DEFAULT_QUALITY`] for the built-ins. PNG is lossless and
    /// takes none: a quality for it, or one out of range, is
    /// [`ExportError::InvalidOption`] before anything renders.
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
    /// Lossless PNG: 8-bit by default, 16-bit with the built-in encoder's
    /// `depth=16`; RGBA, opaque unless `transparent=true`.
    #[default]
    Png,
    /// Baseline JPEG, `.jpg` or `.jpeg`.
    Jpeg,
    /// WebP: lossy (VP8) by default, lossless (VP8L) with the built-in
    /// encoder's `lossless=true`.
    Webp,
}

impl StillFormat {
    /// Its lowercase name, such as `jpeg`, which is also its extension.
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

/// Checks the selection shared by every still layout. The output's
/// extension is checked once the components are named
/// ([`crate::still_output::still_choice`]).
fn validate(frames: &[u64], total: u64) -> Result<(), ExportError> {
    if frames.is_empty() {
        return Err(invalid("select at least one frame"));
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

/// Where rendered frames go, and the components that write them.
struct Destination {
    output: StillOutput,
    layout: Layout,
}

/// One image per frame, or tiles of one contact sheet.
enum Layout {
    Files,
    Sheet(Box<Sheet>),
}

/// Where and how a still export writes: the registry its components come
/// from, the export's output, its choice of components and quality,
/// whether a file there may be replaced, and its cancellation.
struct Target<'a> {
    registry: Arc<MediaRegistry>,
    output: &'a Path,
    choice: &'a OutputChoice,
    quality: Option<u8>,
    overwrite: bool,
    cancellation: &'a ExportCancellation,
}

impl Destination {
    /// Validates the selection, its size limit, and every output path, then
    /// opens the components that write them ([`StillOutput::open`]): all
    /// before anything renders. Returns the resolution's warnings too.
    fn prepare(
        frames: &[u64],
        total: u64,
        contact_sheet: Option<ContactSheet>,
        (width, height): (u32, u32),
        frame_rate: Rational,
        target: Target<'_>,
    ) -> Result<(Self, Vec<String>), ExportError> {
        let Target {
            registry,
            output,
            choice,
            quality,
            overwrite,
            cancellation,
        } = target;
        let open = |files, sheet, size| {
            StillOutput::open(
                registry,
                choice,
                quality,
                files,
                output,
                sheet,
                size,
                frame_rate,
                overwrite,
                cancellation,
            )
        };
        let Some(contact_sheet) = contact_sheet else {
            let paths = outputs(frames, total, output, overwrite)?;
            let files = match paths.as_slice() {
                [path] => StillFiles::One(path.clone()),
                _ => StillFiles::Sequence(frames.iter().copied().zip(paths).collect()),
            };
            let (still, warnings) = open(files, false, (width, height))?;
            let destination = Self {
                output: still,
                layout: Layout::Files,
            };
            return Ok((destination, warnings));
        };
        validate(frames, total)?;
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
        let size = (sheet.width(), sheet.height());
        let (still, warnings) = open(StillFiles::One(output.to_owned()), true, size)?;
        let destination = Self {
            output: still,
            layout: Layout::Sheet(Box::new(sheet)),
        };
        Ok((destination, warnings))
    }

    /// Writes or places the rendered `frame`, adding the files it wrote to
    /// `files`, even when it then fails (one file is written by
    /// [`Self::finish`]).
    fn accept(
        &mut self,
        index: usize,
        frame: u64,
        rendered: GpuFrame,
        files: &mut Vec<ExportedFile>,
    ) -> Result<(), ExportError> {
        let size = (rendered.width(), rendered.height());
        let pixels = encoder_pixels(
            rendered.format(),
            rendered.into_pixels(),
            size.0,
            self.output.opened().input,
        );
        match &mut self.layout {
            Layout::Files => {
                let mut published = Vec::new();
                let written = self.output.write(frame, pixels, &mut published);
                files.extend(frame_files(published));
                written
            }
            Layout::Sheet(sheet) => sheet.place(index, frame, &pixels, size).map_err(invalid),
        }
    }

    /// Marks tile `index` as composition frame `frame`, which failed. Only a
    /// contact sheet has a place for it; separate files return false.
    fn place_failed(&mut self, index: usize, frame: u64) -> bool {
        match &mut self.layout {
            Layout::Files => false,
            Layout::Sheet(sheet) => {
                sheet.place_failed(index, frame);
                true
            }
        }
    }

    /// Writes the contact sheet of `frames`, or the one file of one frame,
    /// and finishes the components, adding the files it wrote to `files`,
    /// even when it then fails. A failure aborts them.
    fn finish(mut self, frames: &[u64], files: &mut Vec<ExportedFile>) -> Result<(), ExportError> {
        let mut published = Vec::new();
        let columns = match &self.layout {
            Layout::Files => None,
            Layout::Sheet(sheet) => {
                if let Err(error) = self
                    .output
                    .write(0, sheet.pixels().to_vec(), &mut published)
                {
                    self.output.abort();
                    return Err(error);
                }
                Some(sheet.columns())
            }
        };
        let finished = self.output.finish(&mut published);
        files.extend(match columns {
            Some(columns) => published
                .into_iter()
                .map(|(_, path)| ExportedFile::ContactSheet {
                    path,
                    frames: frames.to_vec(),
                    columns,
                })
                .collect(),
            None => frame_files(published),
        });
        finished
    }

    /// Ends the components; files already published stay.
    fn abort(self) {
        self.output.abort();
    }
}

/// Published frame files as the files an export reports.
fn frame_files(published: Vec<(u64, PathBuf)>) -> Vec<ExportedFile> {
    published
        .into_iter()
        .map(|(frame, path)| ExportedFile::Frame { path, frame })
        .collect()
}

/// Resolves deterministic image paths and validates all selections before rendering.
fn outputs(
    frames: &[u64],
    total: u64,
    output: &Path,
    overwrite: bool,
) -> Result<Vec<PathBuf>, ExportError> {
    validate(frames, total)?;
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

/// Renders the scene `scene_at` builds for each of `frames` into
/// `destination`, then finishes it. Every failure names its frame. A contact
/// sheet carries on past a frame that cannot be evaluated or rendered, with a
/// failed tile in its place, and once the sheet is written fails with
/// [`ExportError::FramesFailed`] listing each one; separate files stop at the
/// first failure, keeping the files written before it.
fn render_stills(
    frames: &[u64],
    mut destination: Destination,
    renderer: &mut GpuRenderer,
    output: &Path,
    cancellation: &ExportCancellation,
    progress: &mut impl FnMut(ExportProgress),
    scene_at: impl FnMut(u64) -> Result<Scene, ExportError>,
) -> Result<(), ExportError> {
    let failures = match render_frames(
        frames,
        &mut destination,
        renderer,
        cancellation,
        progress,
        scene_at,
    ) {
        Ok(failures) => failures,
        Err(error) => {
            destination.abort();
            return Err(error);
        }
    };
    let mut files = Vec::new();
    let finished = destination.finish(frames, &mut files);
    for file in files {
        progress(ExportProgress::Wrote(file));
    }
    finished?;
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

/// [`render_stills`]' frames, up to writing the contact sheet: the frames
/// a sheet shows as failed tiles.
fn render_frames(
    frames: &[u64],
    destination: &mut Destination,
    renderer: &mut GpuRenderer,
    cancellation: &ExportCancellation,
    progress: &mut impl FnMut(ExportProgress),
    mut scene_at: impl FnMut(u64) -> Result<Scene, ExportError>,
) -> Result<Vec<FrameFailure>, ExportError> {
    let mut reported_warnings = ReportedWarnings::default();
    let mut failures = Vec::new();
    let opened = *destination.output.opened();
    for (index, &frame) in frames.iter().enumerate() {
        ensure_not_cancelled(cancellation)?;
        progress(ExportProgress::Rendering {
            frame: index as u64 + 1,
            total: frames.len() as u64,
        });
        let rendered = scene_at(frame).and_then(|scene| render_frame(renderer, &scene, &opened));
        report_render_warnings(renderer, &mut reported_warnings, progress);
        let mut files = Vec::new();
        let accepted = match rendered {
            Ok(rendered) => destination.accept(index, frame, rendered, &mut files),
            Err(error) if error.is_frame_scoped() && destination.place_failed(index, frame) => {
                failures.push(FrameFailure { frame, error });
                Ok(())
            }
            Err(error) => Err(error),
        };
        // Files published before a failure are reported too.
        for file in files {
            progress(ExportProgress::Wrote(file));
        }
        accepted.map_err(|error| cancellation.settle(error).at_frame(frame))?;
    }
    // A frame a cancel killed failed as frame-scoped; write no sheet of it.
    ensure_not_cancelled(cancellation)?;
    Ok(failures)
}

/// Renders `scene` in the readback the encoder's answer `opened` needs
/// ([`readback_for`]): an opaque `rgba8` as stills have always been
/// rendered, any other through the renderer's export readback.
fn render_frame(
    renderer: &mut GpuRenderer,
    scene: &Scene,
    opened: &VideoEncoderOpened,
) -> Result<GpuFrame, ExportError> {
    if readback_for(opened)? == ReadbackFormat::Rgba8 {
        return renderer.render(scene).map_err(ExportError::Render);
    }
    // Nothing else is in flight, so the frame is the one `drain` returns.
    let submitted = renderer.submit(scene).map_err(ExportError::Render)?;
    let mut frames = renderer.drain().map_err(ExportError::Render)?;
    submitted
        .or_else(|| frames.pop())
        .ok_or_else(|| ExportError::Codec {
            stage: IMAGE_STAGE,
            source: MediaError::Codec("the renderer read back no frame".to_owned()),
        })
}

impl Exporter {
    /// The registry stills write through: the export's, with the built-in
    /// components added when it has none (a test's own registry).
    fn still_registry(&self) -> Arc<MediaRegistry> {
        crate::media::ensure_media(self.plugins());
        let registry = crate::media::current();
        if registry.plugin_index(RESERVED_PLUGIN_ID).is_some() {
            registry
        } else {
            with_builtin(registry.plugins().to_vec())
        }
    }

    /// Validates the selection and opens the components ([`Destination::prepare`]),
    /// reporting the resolution's warnings, then the components
    /// ([`ExportProgress::Output`]), before anything renders.
    #[allow(clippy::too_many_arguments)]
    fn still_destination(
        &self,
        frames: &[u64],
        total: u64,
        request: &StillExport,
        size: (u32, u32),
        frame_rate: Rational,
        output: &Path,
        cancellation: &ExportCancellation,
        progress: &mut impl FnMut(ExportProgress),
    ) -> Result<Destination, ExportError> {
        let (destination, warnings) = Destination::prepare(
            frames,
            total,
            request.contact_sheet,
            size,
            frame_rate,
            Target {
                registry: self.still_registry(),
                output,
                choice: &self.options.output,
                quality: request.quality,
                overwrite: self.options.overwrite,
                cancellation,
            },
        )?;
        for warning in warnings {
            progress(ExportProgress::Warning(warning));
        }
        progress(ExportProgress::Output(destination.output.plan()));
        Ok(destination)
    }

    /// The renderer stills draw with, set to what `destination`'s encoder
    /// takes.
    fn still_renderer(
        &self,
        asset_root: &Path,
        frame_rate: Rational,
        destination: &Destination,
        cancellation: &ExportCancellation,
    ) -> Result<GpuRenderer, ExportError> {
        let mut renderer = self.export_renderer(asset_root, frame_rate, cancellation)?;
        configure_renderer(&mut renderer, destination.output.opened())?;
        Ok(renderer)
    }

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
        let destination = self.still_destination(
            &frames,
            total,
            request,
            (project.settings.width, project.settings.height),
            frame_rate,
            output,
            cancellation,
            &mut progress,
        )?;
        let prepared = Evaluator::new(project)
            .map_err(ExportError::Evaluation)
            .and_then(|evaluator| {
                let renderer =
                    self.still_renderer(asset_root, frame_rate, &destination, cancellation)?;
                Ok((evaluator, renderer))
            });
        let (evaluator, mut renderer) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                destination.abort();
                return Err(error);
            }
        };
        render_stills(
            &frames,
            destination,
            &mut renderer,
            output,
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
        let mut bridge = runtime.spawn(entry, project, self.plugins())?;
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
        for legacy in &metadata.legacy_imports {
            progress(ExportProgress::Warning(legacy.to_string()));
        }
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
        let destination = self.still_destination(
            &frames,
            metadata.duration_in_frames,
            request,
            (metadata.width, metadata.height),
            metadata.frame_rate,
            output,
            cancellation,
            &mut progress,
        )?;
        let prepared = (|| {
            let project_evaluator = project
                .map(|(project, _)| Evaluator::new(project))
                .transpose()
                .map_err(ExportError::Evaluation)?;
            let project_fonts = project_evaluator
                .as_ref()
                .map(|evaluator| -> Result<_, EvaluationError> {
                    let mut fonts = evaluator.scene_at(Time::ZERO)?.fonts;
                    if let Some((_, asset_root)) = project {
                        absolutize_fonts(&mut fonts, asset_root);
                    }
                    Ok(fonts)
                })
                .transpose()
                .map_err(ExportError::Evaluation)?
                .unwrap_or_default();
            let renderer =
                self.still_renderer(asset_root, metadata.frame_rate, &destination, cancellation)?;
            Ok((project_evaluator, project_fonts, renderer))
        })();
        let (project_evaluator, project_fonts, mut renderer) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                destination.abort();
                return Err(error);
            }
        };
        let project_asset_root = project.map(|(_, asset_root)| asset_root);
        render_stills(
            &frames,
            destination,
            &mut renderer,
            output,
            cancellation,
            &mut progress,
            |frame| {
                let time =
                    Time::frames(frame as i64, metadata.frame_rate).map_err(ExportError::Time)?;
                let project_frame = project_evaluator
                    .as_ref()
                    .zip(project)
                    .map(|(evaluator, (project, _))| -> Result<_, EvaluationError> {
                        let mut scene = evaluator.scene_at(time)?;
                        let mut tracks = BTreeMap::new();
                        for track in &project.tracks {
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
                    })
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
        // The extension is checked when the components are named
        // (still_output's `names_the_built_ins_by_extension`).
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
