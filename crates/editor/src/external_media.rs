use super::*;
use crate::source::{PropertyArgs, WatchStamp, watched_stamp};
use celesta_gpu_renderer::hit_test;
use celesta_renderer::image_source::ImageSources;
use gpui_kit::{DragMoveEvent, ExternalPaths, Window};
use std::io::Read;
use std::time::SystemTime;

const PREVIEW_ONLY: &str = "Drop images or videos onto the React preview";
const NO_GROUP: &str = "No writable Group at this position";
const CHECKING: &str = "Checking the dropped media…";
const PREPARING: &str = "Preparing the drop target…";
const CANCELLED: &str = "Media not added because the preview changed";

fn drop_point(
    bounds: gpui_kit::Bounds<Pixels>,
    frame: (f32, f32),
    position: gpui_kit::Point<Pixels>,
) -> Option<Point> {
    scene_point(bounds, frame, position)
}

#[derive(Clone, Debug, PartialEq)]
struct Hit {
    layer: String,
    frame: i64,
    reload: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct DropRequest {
    hit: Hit,
    chapters: Vec<ReactChapter>,
    end: u64,
}

impl DropRequest {
    pub(crate) fn layer(&self) -> &str {
        &self.hit.layer
    }
}

pub(crate) struct DropLookup {
    hit: Hit,
    facts: Result<DropFacts, String>,
}

#[derive(Debug)]
struct DropFacts {
    report: LayerSourceReport,
    groups: Vec<GroupFacts>,
    spaces: HashMap<String, ChildrenSpace>,
    texts: HashMap<PathBuf, Arc<String>>,
}

pub(crate) fn read_drop_lookup(
    request: DropRequest,
    entry: &Path,
    source: LookupResult,
    children_space: impl Fn(&str) -> ChildrenSpace,
) -> DropLookup {
    let facts = source.and_then(|report| {
        let report = report.ok_or_else(|| NO_GROUP.to_owned())?;
        let root = entry.parent().and_then(|dir| fs::canonicalize(dir).ok());
        let texts = RefCell::new(HashMap::new());
        let groups = parent_facts(
            |path| {
                let text = fs::read_to_string(path).ok()?;
                texts
                    .borrow_mut()
                    .insert(path.to_owned(), Arc::new(text.clone()));
                Some(text)
            },
            |file| {
                root.as_ref().is_some_and(|root| {
                    fs::canonicalize(file).is_ok_and(|file| inside_project(&file, root))
                })
            },
            &report,
            &request.chapters,
            request.end,
            u64::try_from(request.hit.frame).unwrap_or(0),
        );
        let spaces = report
            .chain
            .iter()
            .filter_map(|layer| {
                let id = layer.layer_id.as_ref()?;
                groups
                    .iter()
                    .any(|group| group.at == SourcePos::from(layer))
                    .then(|| (id.clone(), children_space(id)))
            })
            .collect();
        Ok(DropFacts {
            report,
            groups,
            spaces,
            texts: texts.into_inner(),
        })
    });
    DropLookup {
        hit: request.hit,
        facts,
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Target {
    parent: AddParent,
    tag: String,
    timing: Option<Timing>,
    space: [f64; 6],
    text: Arc<String>,
    label: String,
}

pub(super) struct ExpectedSource {
    text: Arc<String>,
    entry: PathBuf,
    props: PropertyArgs,
    loaded: WatchStamp,
}

impl ExpectedSource {
    pub(super) fn matches(&self, text: Option<&String>) -> bool {
        text == Some(self.text.as_ref())
            && watched_stamp(&self.entry, &self.props)
                .is_some_and(|current| self.loaded.same_source_inputs(&current))
    }
}

impl DropFacts {
    fn target(
        &self,
        kind: NewKind,
        scope: ParentScope,
        shared: impl Fn(&LayerSource) -> Result<Sharing, String>,
    ) -> Result<Target, String> {
        let group = |entry: &LayerSource| {
            self.groups
                .iter()
                .find(|group| group.at == SourcePos::from(entry))
        };
        let parent = pick_parent(
            Some(&self.report),
            &scope,
            |entry| group(entry).is_some(),
            &shared,
        )
        .map_err(|reason| {
            if reason == NO_PARENT {
                NO_GROUP.to_owned()
            } else {
                reason
            }
        })?;
        let group = group(&parent.entry).expect("picked group has facts");
        let timing = group.timing.clone()?;
        if let Some((_, Some(reason))) = group.locks.iter().find(|(of, _)| *of == kind) {
            return Err(reason.clone());
        }
        let space = self.spaces.get(&parent.layer_id).copied();
        if let Some(reason) = space_reason(space, false) {
            return Err(reason.to_owned());
        }
        let Some(ChildrenSpace::Map(space)) = space else {
            unreachable!()
        };
        let sharing = shared(&parent.entry)?;
        let suffix = if sharing.calls {
            " · Shared value (all instances)"
        } else if sharing.scenes {
            " · Shared across scenes"
        } else {
            ""
        };
        let file = parent
            .entry
            .file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let label = format!(
            "{} ({file}:{}){suffix}",
            parent_label(&parent, group.component.as_deref()),
            parent.entry.line,
        );
        let text = self
            .texts
            .get(&parent.entry.file)
            .ok_or_else(|| SOURCE_CHANGED.to_owned())?;
        Ok(Target {
            parent,
            tag: group.tag.clone(),
            timing,
            space,
            label,
            text: Arc::clone(text),
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct Media {
    kind: NewKind,
    aspect: f64,
    stamp: (u64, SystemTime),
}

fn file_stamp(path: &Path) -> Option<(u64, SystemTime)> {
    let meta = path.metadata().ok()?;
    meta.is_file()
        .then_some((meta.len(), meta.modified().ok()?))
}

fn check_media(path: &Path) -> Result<Media, String> {
    let stamp = file_stamp(path).ok_or_else(|| "Drop a readable media file".to_owned())?;
    let unreadable = "Unsupported or unreadable image or video";
    let mut head = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(4096).read_to_end(&mut head))
        .map_err(|_| unreadable.to_owned())?;
    let image = image::guess_format(&head).is_ok()
        || path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
        || head.windows(4).any(|bytes| bytes == b"<svg");
    let (kind, width, height) = if image {
        let (width, height) = ImageSources::default()
            .natural_size("drop", path)
            .map_err(|_| unreadable.to_owned())?;
        (NewKind::Image, width, height)
    } else {
        let mut backend = MediaBackend::new();
        let info = backend.probe(path).map_err(|_| unreadable.to_owned())?;
        let Some(video) = info.video.as_ref() else {
            return Err(if !info.audio.is_empty() {
                "Dropping audio onto the timeline is not supported yet"
            } else {
                unreadable
            }
            .to_owned());
        };
        (
            NewKind::Video,
            f64::from(video.width),
            f64::from(video.height),
        )
    };
    let aspect = width / height;
    if !aspect.is_finite() || aspect <= 0.0 || file_stamp(path) != Some(stamp) {
        return Err("Couldn’t read a stable image or video size".to_owned());
    }
    Ok(Media {
        kind,
        aspect,
        stamp,
    })
}

fn one_file(paths: &[PathBuf]) -> Result<&Path, String> {
    match paths {
        [path] => Ok(path),
        _ => Err("Drop one image or video at a time".to_owned()),
    }
}

#[derive(Debug, Default)]
pub(crate) struct ExternalMedia {
    tracking: bool,
    hover: Option<Vec<PathBuf>>,
    checked: Option<Result<Media, String>>,
    hover_serial: u64,
    asked: Option<DropRequest>,
    lookup: Option<(Hit, Result<DropFacts, String>)>,
    importing: bool,
    import_serial: u64,
}

impl EditorView {
    pub(crate) fn tracking_external_media(&self) -> bool {
        self.add.external.tracking
    }

    fn drop_allowed(&self) -> Result<(), String> {
        let react = self
            .react_preview
            .as_ref()
            .ok_or_else(|| PREVIEW_ONLY.to_owned())?;
        if self.playing {
            return Err("Pause playback before dropping media".to_owned());
        }
        if self.react_reloads.is_running() {
            return Err(READING.to_owned());
        }
        if react.loaded.is_none() || react.loaded != react.watched {
            return Err("The React source couldn’t be loaded".to_owned());
        }
        if self.add.mode != AddMode::Off
            || self.edit_history.is_busy()
            || self.property_panel.is_writing()
            || self.transform_drag.is_some()
            || self.row_scrub_active()
            || self.inline_text.is_some()
        {
            return Err("Finish the current edit before dropping media".to_owned());
        }
        Ok(())
    }

    fn drop_hit(&self, position: gpui_kit::Point<Pixels>) -> Result<(Hit, Point), String> {
        self.drop_allowed()?;
        let point = self
            .viewer_bounds
            .get()
            .and_then(|bounds| drop_point(bounds, self.frame_size(), position))
            .ok_or_else(|| PREVIEW_ONLY.to_owned())?;
        if self.preview_worker.generation() != self.preview_shown_generation {
            return Err(PREPARING.to_owned());
        }
        let layer = hit_test(&self.hit_regions, point).ok_or_else(|| NO_GROUP.to_owned())?;
        Ok((
            Hit {
                layer: layer.to_owned(),
                frame: self.clock.frame(),
                reload: self.react_reload_generation,
            },
            point,
        ))
    }

    fn drop_target(&self, hit: &Hit, kind: NewKind) -> Result<Target, String> {
        let (_, facts) = self
            .add
            .external
            .lookup
            .as_ref()
            .filter(|(of, _)| of == hit)
            .ok_or_else(|| PREPARING.to_owned())?;
        let facts = facts.as_ref().map_err(Clone::clone)?;
        let scope = facts
            .report
            .chain
            .first()
            .and_then(|layer| self.scope_choices.get(&SourcePos::from(layer)))
            .map_or(ParentScope::NoControl, ParentScope::Chosen);
        let occurrences = self.occurrences.as_ref().map(|(_, state)| state);
        if matches!(occurrences, Some(OccurrenceState::Refreshing(_))) {
            return Err("The scene scan is still being updated".to_owned());
        }
        let chapters = self.react_chapters().unwrap_or_default();
        facts.target(kind, scope, |entry| {
            is_shared(occurrences, chapters, &SourcePos::from(entry))
        })
    }

    pub(crate) fn external_drop_request(&self) -> Option<DropRequest> {
        self.add
            .external
            .asked
            .as_ref()
            .filter(|request| {
                request.hit.frame == self.clock.frame()
                    && request.hit.reload == self.react_reload_generation
            })
            .cloned()
    }

    pub(crate) fn take_external_drop_lookup(&mut self, result: Option<DropLookup>) {
        if let Some(DropLookup { hit, facts }) = result
            && self
                .add
                .external
                .asked
                .as_ref()
                .is_some_and(|request| request.hit == hit)
        {
            self.add.external.asked = None;
            self.add.external.lookup = Some((hit, facts));
        }
    }

    pub(crate) fn move_external_media(
        &mut self,
        event: &DragMoveEvent<ExternalPaths>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = event.drag(cx).paths();
        if self.add.external.hover.as_deref() != Some(paths) {
            let path = one_file(paths).map(Path::to_owned);
            let external = &mut self.add.external;
            external.hover = Some(paths.to_vec());
            external.checked = None;
            external.asked = None;
            external.lookup = None;
            external.hover_serial += 1;
            let serial = external.hover_serial;
            match path {
                Ok(path) if self.is_react_preview() => {
                    cx.spawn(async move |view, cx| {
                        let checked = cx
                            .background_executor()
                            .spawn(async move { check_media(&path) })
                            .await;
                        view.update(cx, |this, cx| {
                            if this.add.external.hover_serial == serial {
                                this.add.external.checked = Some(checked);
                                cx.notify();
                            }
                        })
                        .ok();
                    })
                    .detach();
                }
                Err(reason) => self.add.external.checked = Some(Err(reason)),
                Ok(_) => {}
            }
        }
        cx.notify();
    }

    pub(crate) fn settle_external_media(&mut self, window: &Window, active_drag: bool) {
        let external = &mut self.add.external;
        if !active_drag {
            external.hover = None;
            external.checked = None;
            external.asked = None;
            if !external.importing {
                external.lookup = None;
            }
        }
        let tracking = (active_drag
            && self.react_preview.is_some()
            && external.hover.is_some()
            && !self.playing
            && !external.checked.as_ref().is_some_and(Result::is_err))
            || external.importing;
        if std::mem::replace(&mut external.tracking, tracking) != tracking && !self.picks_layers() {
            self.refresh_preview();
        }
        let external = &self.add.external;
        if !active_drag
            || external.hover.is_none()
            || external.checked.as_ref().is_some_and(Result::is_err)
        {
            return;
        }
        let Ok((hit, _)) = self.drop_hit(window.mouse_position()) else {
            return;
        };
        if self
            .add
            .external
            .lookup
            .as_ref()
            .is_some_and(|(of, _)| of == &hit)
            || self
                .add
                .external
                .asked
                .as_ref()
                .is_some_and(|request| request.hit == hit)
        {
            return;
        }
        self.add.external.asked = Some(DropRequest {
            hit,
            chapters: self.react_chapters().unwrap_or_default().to_vec(),
            end: u64::try_from(self.clock.end_frame()).unwrap_or(0),
        });
        self.reuse_scene = true;
        self.refresh_preview();
    }

    fn ready_external_drop(
        &self,
        paths: &[PathBuf],
        position: gpui_kit::Point<Pixels>,
    ) -> Result<(Hit, Point, Media, Target), String> {
        if self.add.external.importing {
            return Err("Finishing the previous media drop…".to_owned());
        }
        self.drop_allowed()?;
        one_file(paths)?;
        let (hit, point) = self.drop_hit(position)?;
        let media = *self
            .add
            .external
            .checked
            .as_ref()
            .filter(|_| self.add.external.hover.as_deref() == Some(paths))
            .ok_or_else(|| CHECKING.to_owned())?
            .as_ref()
            .map_err(Clone::clone)?;
        let target = self.drop_target(&hit, media.kind)?;
        Ok((hit, point, media, target))
    }

    pub(crate) fn media_drop_feedback(
        &self,
        window: &Window,
        active_drag: bool,
    ) -> Option<(bool, String)> {
        if active_drag && let Some(paths) = self.add.external.hover.as_deref() {
            return Some(
                match self.ready_external_drop(paths, window.mouse_position()) {
                    Ok((_, _, media, target)) => (
                        true,
                        format!("Drop {} into {}", media.kind.tag(), target.label),
                    ),
                    Err(reason) => (false, reason),
                },
            );
        }
        self.add
            .external
            .importing
            .then(|| (false, "Importing dropped media… Esc to cancel".to_owned()))
    }

    pub(crate) fn drop_external_media(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self.ready_external_drop(paths.paths(), window.mouse_position());
        let (hit, point, media, target) = match result {
            Ok(ready) => ready,
            Err(reason) => {
                self.add.notice = Some(reason);
                cx.notify();
                return;
            }
        };
        let path = paths.paths()[0].clone();
        let dir = self.entry_dir();
        let react = self.react_preview.as_ref().expect("checked React preview");
        let source = ExpectedSource {
            text: Arc::clone(&target.text),
            entry: react.entry.clone(),
            props: react.property_args.clone(),
            loaded: react.loaded.clone().expect("checked loaded source"),
        };
        let entry = source.entry.clone();
        let reload = self.react_reloads.next();
        let generation = self.preview_worker.generation();
        let serial = self.add.external.import_serial;
        self.add.external.importing = true;
        self.add.notice = None;
        cx.notify();
        let expected = target.clone();
        let still_current = move |this: &EditorView| {
            this.react_entry() == Some(entry.as_path())
                && this.react_reloads.next() == reload
                && this.preview_worker.generation() == generation
                && this.drop_allowed().is_ok()
                && this
                    .drop_target(&hit, media.kind)
                    .is_ok_and(|current| current == expected)
        };
        cx.spawn(async move |view, cx| {
            let staged = cx
                .background_executor()
                .spawn(async move {
                    if file_stamp(&path) != Some(media.stamp) {
                        return Err("The media file changed during the drop".to_owned());
                    }
                    let staged = stage_import(&dir, &dir, &path)
                        .map_err(|error| format!("Couldn’t import media: {error}"))?;
                    if file_stamp(&path) != Some(media.stamp) {
                        return Err("The media file changed during the drop".to_owned());
                    }
                    Ok(staged)
                })
                .await;
            if !view
                .update(cx, |this, cx| {
                    this.external_import_current(serial, &still_current, cx)
                })
                .unwrap_or(false)
            {
                return;
            }
            let src = match staged {
                Ok(staged) => {
                    cx.background_executor()
                        .spawn(async move {
                            staged
                                .publish()
                                .map_err(|error| format!("Couldn’t import media: {error}"))
                        })
                        .await
                }
                Err(reason) => Err(reason),
            };
            view.update(cx, |this, cx| {
                if !this.external_import_current(serial, &still_current, cx) {
                    return;
                }
                this.add.external.importing = false;
                match src.and_then(|src| {
                    drawn_geometry(
                        media.kind,
                        Some(&src),
                        (point, point),
                        target.space,
                        Some(media.aspect),
                        1.0,
                    )
                    .ok_or_else(|| SCALED_TO_ZERO.to_owned())
                }) {
                    Ok(new) => this.write_child(
                        target.parent,
                        target.tag,
                        target.timing,
                        Addition::Element(new),
                        Some(source),
                        cx,
                    ),
                    Err(reason) => this.add.notice = Some(reason),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn external_import_current(
        &mut self,
        serial: u64,
        current: &impl Fn(&Self) -> bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.add.external.import_serial != serial || !self.add.external.importing {
            return false;
        }
        if !current(self) {
            self.add.external.importing = false;
            self.add.notice = Some(CANCELLED.to_owned());
            cx.notify();
            return false;
        }
        true
    }

    pub(crate) fn cancel_external_media(&mut self) -> bool {
        if !std::mem::take(&mut self.add.external.importing) {
            return false;
        }
        self.add.external.import_serial += 1;
        self.add.notice = Some("Media drop cancelled".to_owned());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_gpu_renderer::HitRegion;
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn letterbox_margins_are_not_drop_targets() {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(100.), px(100.)));
        assert!(drop_point(bounds, (100., 50.), point(px(40.), px(10.))).is_none());
        assert!(drop_point(bounds, (100., 50.), point(px(40.), px(30.))).is_some());
    }

    #[test]
    fn the_topmost_hit_does_not_fall_back_to_another_branch() {
        let region = |layer: &str| HitRegion {
            layer_id: layer.to_owned(),
            corners: [
                Point { x: 0.0, y: 0.0 },
                Point { x: 20.0, y: 0.0 },
                Point { x: 20.0, y: 20.0 },
                Point { x: 0.0, y: 20.0 },
            ],
            hole: None,
            anchor_box: None,
            text_box: None,
            clips: Vec::new(),
        };
        let point = Point { x: 10.0, y: 10.0 };
        let regions = [region("under"), region("over")];
        assert_eq!(hit_test(&regions, point), Some("over"));
        assert!(hit_test(&regions, Point { x: 25.0, y: 10.0 }).is_none());
        let facts = DropFacts {
            report: LayerSourceReport {
                chain: Vec::new(),
                local_frame: 0.0,
                candidate_frames: Vec::new(),
                props: Default::default(),
                render_count: 1,
                windows: Vec::new(),
                span: None,
            },
            groups: Vec::new(),
            spaces: HashMap::new(),
            texts: HashMap::new(),
        };
        assert_eq!(
            facts.target(NewKind::Image, ParentScope::NoControl, |_| unreachable!()),
            Err(NO_GROUP.to_owned())
        );
    }

    #[test]
    fn only_one_external_file_can_be_added() {
        let path = PathBuf::from("clip.mp4");
        assert_eq!(one_file(std::slice::from_ref(&path)), Ok(path.as_path()));
        assert!(one_file(&[]).is_err());
        assert!(one_file(&[path, PathBuf::from("still.png")]).is_err());
    }

    #[test]
    fn validates_svg_content_and_keeps_its_aspect() {
        let base = std::env::temp_dir().join(format!("celesta-drop-svg-{}", std::process::id()));
        fs::create_dir(&base).unwrap();
        let path = base.join("test.SVG");
        fs::write(&path, "not an image").unwrap();
        let malformed = check_media(&path);
        fs::write(&path, r#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="20"><rect width="80" height="20"/></svg>"#).unwrap();
        let valid = check_media(&path);
        fs::remove_dir_all(base).unwrap();
        assert!(malformed.is_err());
        let valid = valid.unwrap();
        assert_eq!(valid.kind, NewKind::Image);
        assert_eq!(valid.aspect, 4.0);
    }
}
