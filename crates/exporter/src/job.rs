use crate::control::{ExportCancellation, ExportProgress};
use crate::error::ExportError;
use crate::exporter::{Exporter, project_plugins};
use crate::options::ExportOptions;
use crate::react::{CompanionProject, ReactRuntimeOptions};
use crate::stills::StillExport;
use celesta_media::MediaRegistry;
use celesta_project::Project;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    /// The registry the export reads and writes media through, whatever
    /// registry is chosen meanwhile ([`Exporter::with_media`]); `None`
    /// takes the one [`Exporter::new`] does.
    pub media: Option<Arc<MediaRegistry>>,
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
        Ok(match &self.media {
            Some(media) => Exporter::with_media(options, media.clone()),
            None => Exporter::new(options),
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExportCancellation, with_builtin};

    /// What `job` reports the muxers of its registry write, exporting to an
    /// extension none writes: output resolution goes through the export's
    /// registry before anything renders, so this needs no GPU.
    fn written_extensions(job: &ExportJob) -> Vec<String> {
        let error = job
            .run(&ExportCancellation::default(), &mut |_| {})
            .expect_err("no muxer writes .unwritten");
        match error {
            ExportError::UnsupportedOutput { extensions, .. } => extensions,
            error => panic!("{error}"),
        }
    }

    /// Another project's plugins becoming the chosen registry, as each MCP
    /// tool call makes its project's, does not change the registry an
    /// export reads and writes through: its job's own. No other unit test
    /// reads the chosen registry.
    #[test]
    fn two_projects_get_their_own_media_registry() {
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/minimal.celesta.json");
        let output = tempfile::tempdir().unwrap();
        // Project A has only the built-in components; project B also the
        // test codec, whose muxer alone writes `.ctst`.
        let a = with_builtin(Vec::new());
        let b = with_builtin(vec![celesta_codec_test::registered()]);
        let job = |media: &Arc<MediaRegistry>| ExportJob {
            source: source.clone(),
            react: false,
            composition_id: None,
            companion: None,
            props_file: None,
            props: None,
            output: Some(output.path().join("film.unwritten")),
            stills: None,
            options: ExportOptions {
                plugins: Some(celesta_plugin_host::InstalledPlugins::default()),
                ..ExportOptions::default()
            },
            media: Some(media.clone()),
        };

        celesta_media::set_registry(Some(a.clone()));
        let extensions = written_extensions(&job(&b));
        assert!(
            extensions.iter().any(|extension| extension == "ctst"),
            "{extensions:?}"
        );

        celesta_media::set_registry(Some(b));
        let extensions = written_extensions(&job(&a));
        assert!(
            !extensions.iter().any(|extension| extension == "ctst"),
            "{extensions:?}"
        );
        celesta_media::set_registry(None);
    }
}
