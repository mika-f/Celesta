use crate::control::{ExportCancellation, ExportProgress};
use crate::error::ExportError;
use crate::exporter::{Exporter, project_plugins};
use crate::options::ExportOptions;
use crate::react::{CompanionProject, ReactRuntimeOptions};
use crate::stills::StillExport;
use celesta_project::Project;
use std::path::{Path, PathBuf};

/// One export as `Celesta-export` and the MCP server describe it: a JSON
/// project or a React entry, written as still images (PNG, JPEG or WebP), a
/// video (MP4, MOV or WebM), or only subtitle files.
#[derive(Clone, Debug)]
pub struct ExportJob {
    pub source: PathBuf,
    /// Whether `source` is a React entry rather than a JSON project.
    pub react: bool,
    /// Root composition ID; absent selects the first root.
    pub composition_id: Option<String>,
    /// A `.celesta.json` exported alongside a React entry.
    pub companion: Option<PathBuf>,
    /// The React entry's property file; `None` uses its `props.json`.
    pub props_file: Option<PathBuf>,
    /// Property values as a JSON object, over `props_file`'s.
    pub props: Option<String>,
    /// The video or still image to write. `None` writes only the subtitle
    /// files `options.subtitles` names, rendering nothing.
    pub output: Option<PathBuf>,
    /// `Some` writes still images (PNG, JPEG or WebP) instead of a video.
    pub stills: Option<StillExport>,
    pub options: ExportOptions,
}

impl ExportJob {
    /// Runs the export, checking `cancellation` between frames. Without
    /// `options.plugins`, the plugins are those [`project_plugins`] finds
    /// for `source`, so a project whose lock is not satisfied fails here,
    /// before anything starts.
    pub fn run(
        &self,
        cancellation: &ExportCancellation,
        progress: &mut dyn FnMut(ExportProgress),
    ) -> Result<(), ExportError> {
        let exporter = self.exporter()?;
        let load = |path: &Path| Project::load(path).map_err(ExportError::Project);
        let companion = self
            .companion
            .as_deref()
            .map(|path| load(path).map(|project| (project, path)))
            .transpose()?;
        let companion = companion.as_ref().map(|(project, path)| CompanionProject {
            project,
            project_asset_root: asset_root(path),
        });
        let runtime = || self.runtime();
        let Some(output) = &self.output else {
            if self.stills.is_some() {
                return Err(ExportError::InvalidSelection(
                    "still images need an output path".into(),
                ));
            }
            return if self.react {
                exporter.export_react_subtitles(
                    &self.source,
                    &runtime()?,
                    companion,
                    cancellation,
                    progress,
                )
            } else {
                exporter.export_project_subtitles(&load(&self.source)?, progress)
            };
        };
        match (&self.stills, self.react) {
            (Some(stills), true) => exporter.export_react_stills_with(
                &self.source,
                &runtime()?,
                companion,
                stills,
                output,
                cancellation,
                progress,
            ),
            (Some(stills), false) => exporter.export_project_stills_with(
                &load(&self.source)?,
                asset_root(&self.source),
                stills,
                output,
                cancellation,
                progress,
            ),
            (None, true) => {
                let runtime = runtime()?;
                match companion {
                    Some(companion) => exporter.export_react_entry_with_project_cancellable(
                        &self.source,
                        &runtime,
                        companion,
                        output,
                        cancellation,
                        progress,
                    ),
                    None => exporter.export_react_entry_cancellable(
                        &self.source,
                        &runtime,
                        output,
                        cancellation,
                        progress,
                    ),
                }
            }
            (None, false) => exporter.export_project_cancellable(
                &load(&self.source)?,
                asset_root(&self.source),
                output,
                cancellation,
                progress,
            ),
        }
    }

    /// Reports the composition — its size, rate, and length, and a React
    /// entry's roots — as [`Self::run`] does before it renders, then stops.
    /// `output`, `stills`, and `options` other than `plugins` (as in
    /// [`Self::run`]) are not used.
    pub fn describe(&self, progress: &mut dyn FnMut(ExportProgress)) -> Result<(), ExportError> {
        let exporter = self.exporter()?;
        if !self.react {
            let project = Project::load(&self.source).map_err(ExportError::Project)?;
            return exporter.describe_project(&project, progress);
        }
        let companion = self
            .companion
            .as_deref()
            .map(|path| Project::load(path).map(|project| (project, path)))
            .transpose()
            .map_err(ExportError::Project)?;
        exporter.describe_react_entry(
            &self.source,
            &self.runtime()?,
            companion.as_ref().map(|(project, path)| CompanionProject {
                project,
                project_asset_root: asset_root(path),
            }),
            progress,
        )
    }

    /// An exporter with `options`, and `plugins` found for `source`'s
    /// project when it gives none.
    fn exporter(&self) -> Result<Exporter, ExportError> {
        let mut options = self.options.clone();
        if options.plugins.is_none() {
            options.plugins = Some(project_plugins(&self.source, &[], &[])?);
        }
        Ok(Exporter::new(options))
    }

    fn runtime(&self) -> Result<ReactRuntimeOptions, ExportError> {
        ReactRuntimeOptions::for_entry(
            &self.source,
            self.props_file.as_deref(),
            self.props.as_deref(),
        )
        .map(|runtime| runtime.with_composition(self.composition_id.clone()))
    }
}

/// Where a project's relative asset paths resolve from.
fn asset_root(project_path: &Path) -> &Path {
    project_path.parent().unwrap_or_else(|| Path::new("."))
}
