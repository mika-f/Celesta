use super::*;
use crate::contact_sheet::Sheet;

/// Which composition frames a PNG export renders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameSelection {
    /// Exact zero-based composition frames, in this order. Cannot be combined
    /// with an export range.
    Frames(Vec<u64>),
    /// Every `n`th frame of the export range (the whole composition without
    /// one) counted from its first frame, plus its last frame when that is
    /// not already on the interval. Frame numbers stay composition frames.
    Every(u64),
}

/// Lays the selected frames out on one PNG instead of one PNG per frame.
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

/// What a PNG export renders and how it is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PngExport {
    pub frames: FrameSelection,
    /// `None` writes one PNG per frame.
    pub contact_sheet: Option<ContactSheet>,
}

/// Most frames one export writes as separate PNG files.
pub const MAX_PNG_FRAMES: usize = 1_000;

fn invalid(message: impl Into<String>) -> ExportError {
    ExportError::Io {
        operation: "export PNG frames",
        source: io::Error::new(io::ErrorKind::InvalidInput, message.into()),
    }
}

/// Checks the selection and output name shared by every PNG layout.
fn validate(frames: &[u64], total: u64, output: &Path) -> Result<(), ExportError> {
    if frames.is_empty() {
        return Err(invalid("select at least one frame"));
    }
    if output.extension() != Some(OsStr::new("png")) {
        return Err(invalid("PNG output must use the .png extension"));
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
/// separate PNG files.
fn frame_limit(contact_sheet: Option<ContactSheet>) -> (usize, &'static str) {
    match contact_sheet {
        Some(_) => (ContactSheet::MAX_FRAMES, "a contact sheet holds"),
        None => (MAX_PNG_FRAMES, "one export writes as separate PNGs"),
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
                    "PNG frame selection cannot be combined with an export range",
                ));
            }
            if frames.len() > limit {
                return Err(too_many(frames.len() as u64));
            }
            Ok(frames.clone())
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

/// Where rendered frames go: one PNG each, or tiles of one contact sheet.
enum Destination {
    Files(Vec<PathBuf>),
    Sheet(Box<Sheet>),
}

impl Destination {
    /// Validates the selection, its size limit, and every output path
    /// before anything renders.
    fn prepare(
        frames: &[u64],
        total: u64,
        contact_sheet: Option<ContactSheet>,
        (width, height): (u32, u32),
        frame_rate: Rational,
        output: &Path,
        overwrite: bool,
    ) -> Result<Self, ExportError> {
        let Some(contact_sheet) = contact_sheet else {
            return outputs(frames, total, output, overwrite).map(Self::Files);
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
        Sheet::new(
            width,
            height,
            frames.len(),
            contact_sheet.columns,
            contact_sheet.tile_width,
            ContactSheet::MAX_SIDE,
            frame_rate,
        )
        .map(|sheet| Self::Sheet(Box::new(sheet)))
        .map_err(invalid)
    }

    fn accept(
        &mut self,
        index: usize,
        frame: u64,
        rendered: &celesta_gpu_renderer::GpuFrame,
        overwrite: bool,
    ) -> Result<(), ExportError> {
        match self {
            Self::Files(paths) => publish(rendered, &paths[index], overwrite),
            Self::Sheet(sheet) => sheet
                .place(
                    index,
                    frame,
                    rendered.pixels(),
                    (rendered.width(), rendered.height()),
                )
                .map_err(invalid),
        }
    }

    fn finish(self, output: &Path, overwrite: bool) -> Result<(), ExportError> {
        match self {
            Self::Files(_) => Ok(()),
            Self::Sheet(sheet) => write_png(
                sheet.width(),
                sheet.height(),
                sheet.pixels(),
                output,
                overwrite,
            ),
        }
    }
}

/// Resolves deterministic PNG paths and validates all selections before rendering.
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
            name.push(format!("-{frame:06}.png"));
            output.with_file_name(name)
        };
        if !overwrite && path.exists() {
            return Err(ExportError::OutputExists(path));
        }
        paths.push(path);
    }
    Ok(paths)
}

fn publish(
    frame: &celesta_gpu_renderer::GpuFrame,
    path: &Path,
    overwrite: bool,
) -> Result<(), ExportError> {
    write_png(
        frame.width(),
        frame.height(),
        frame.pixels(),
        path,
        overwrite,
    )
}

fn write_png(
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
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|source| ExportError::Io {
        operation: "create temporary PNG",
        source,
    })?;
    {
        let mut encoder = png::Encoder::new(file.as_file_mut(), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| ExportError::Io {
            operation: "encode PNG",
            source: io::Error::other(e),
        })?;
        writer
            .write_image_data(pixels)
            .map_err(|e| ExportError::Io {
                operation: "encode PNG",
                source: io::Error::other(e),
            })?;
        writer.finish().map_err(|e| ExportError::Io {
            operation: "finish PNG",
            source: io::Error::other(e),
        })?;
    }
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
                operation: "publish PNG",
                source: e.error,
            }
        }
    })?;
    Ok(())
}

impl Exporter {
    /// Exports exact zero-based project frames without video encoding or audio mixing.
    /// Multiple selections append `-000090` style frame numbers to the output stem.
    pub fn export_project_png(
        &self,
        project: &Project,
        asset_root: &Path,
        frames: &[u64],
        output: &Path,
        progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        self.export_project_png_with(
            project,
            asset_root,
            &PngExport {
                frames: FrameSelection::Frames(frames.to_vec()),
                contact_sheet: None,
            },
            output,
            progress,
        )
    }

    /// Exports project frames chosen by `request` as separate PNGs or one
    /// contact sheet. An export range is only allowed with
    /// [`FrameSelection::Every`].
    pub fn export_project_png_with(
        &self,
        project: &Project,
        asset_root: &Path,
        request: &PngExport,
        output: &Path,
        mut progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let duration = project.effective_duration().map_err(ExportError::Time)?;
        let frame_rate = project.settings.frame_rate;
        let total = frame_count(duration, frame_rate)?;
        let frames = select(
            &request.frames,
            self.options.range.as_ref(),
            duration,
            frame_rate,
            request.contact_sheet,
        )?;
        let mut destination = Destination::prepare(
            &frames,
            total,
            request.contact_sheet,
            (project.settings.width, project.settings.height),
            frame_rate,
            output,
            self.options.overwrite,
        )?;
        let evaluator = Evaluator::new(project).map_err(ExportError::Evaluation)?;
        let mut renderer = export_renderer(
            asset_root,
            frame_rate,
            ColorConversion::Encoder,
            self.options.render_quality,
        )?;
        let mut fallbacks = ReportedFontWarnings::default();
        for (index, &frame) in frames.iter().enumerate() {
            progress(ExportProgress::Rendering {
                frame: index as u64 + 1,
                total: frames.len() as u64,
            });
            let time = Time::frames(frame as i64, frame_rate).map_err(ExportError::Time)?;
            let scene = evaluator.scene_at(time).map_err(ExportError::Evaluation)?;
            let rendered = renderer.render(&scene).map_err(ExportError::Render)?;
            report_font_fallbacks(&renderer, &mut fallbacks, &mut progress);
            destination.accept(index, frame, &rendered, self.options.overwrite)?;
        }
        destination.finish(output, self.options.overwrite)
    }

    /// Exports selected React frames with one runtime/prepare call and shared font/asset caches.
    pub fn export_react_png(
        &self,
        entry: &Path,
        runtime: &ReactRuntimeOptions,
        companion: Option<CompanionProject<'_>>,
        frames: &[u64],
        output: &Path,
        progress: impl FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        self.export_react_png_with(
            entry,
            runtime,
            companion,
            &PngExport {
                frames: FrameSelection::Frames(frames.to_vec()),
                contact_sheet: None,
            },
            output,
            progress,
        )
    }

    /// [`Self::export_react_png`] for frames chosen by `request`, as separate
    /// PNGs or one contact sheet.
    pub fn export_react_png_with(
        &self,
        entry: &Path,
        runtime: &ReactRuntimeOptions,
        companion: Option<CompanionProject<'_>>,
        request: &PngExport,
        output: &Path,
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
        let mut bridge = ReactBridge::spawn(&runtime.node, &runtime.cli_script, entry)
            .map_err(ExportError::React)?;
        let metadata = bridge.metadata().clone();
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
        let mut destination = Destination::prepare(
            &frames,
            metadata.duration_in_frames,
            request.contact_sheet,
            (metadata.width, metadata.height),
            metadata.frame_rate,
            output,
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

        let mut renderer = export_renderer(
            asset_root,
            metadata.frame_rate,
            ColorConversion::Encoder,
            self.options.render_quality,
        )?;
        let mut fallbacks = ReportedFontWarnings::default();
        for (index, &frame) in frames.iter().enumerate() {
            progress(ExportProgress::Rendering {
                frame: index as u64 + 1,
                total: frames.len() as u64,
            });
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

            let rendered = renderer.render(&scene).map_err(ExportError::Render)?;
            report_font_fallbacks(&renderer, &mut fallbacks, &mut progress);
            destination.accept(index, frame, &rendered, self.options.overwrite)?;
        }
        destination.finish(output, self.options.overwrite)
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
