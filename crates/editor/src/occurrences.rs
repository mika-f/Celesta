use crate::audio::take_latest;
use crate::problems::error_chain;
use crate::view::EditorView;
use celesta_react_bridge::runtime_paths as react_runtime_paths;
use celesta_react_bridge::{OccurrenceIndex, PropertyInputs, ReactBridge, SpawnOptions};
use gpui_kit::Context;
use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Frames asked of the bridge per `collectOccurrences` request; a reload
/// abandons the running sweep at the next chunk boundary.
pub(crate) const CHUNK_FRAMES: u64 = 240;

/// How often a running sweep's result is looked for. A sweep takes seconds,
/// so the result shows at most this much later than it lands.
const RESULT_POLL: Duration = Duration::from_millis(200);

pub(crate) struct OccurrenceRequest {
    pub(crate) generation: u64,
    pub(crate) node: PathBuf,
    pub(crate) cli_script: PathBuf,
    pub(crate) entry: PathBuf,
    pub(crate) properties: PropertyInputs,
    pub(crate) composition_id: String,
    pub(crate) duration_in_frames: u64,
}

pub(crate) struct OccurrenceResult {
    pub(crate) generation: u64,
    pub(crate) index: Result<OccurrenceIndex, String>,
}

/// Where each element of a standalone entry renders, swept over every frame
/// of the composition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OccurrenceState {
    Scanning,
    Ready(OccurrenceIndex),
    /// A new sweep runs after the editor's own write; the last index stays
    /// in use, read like `Ready`, until its result replaces it.
    Refreshing(OccurrenceIndex),
    Failed(String),
}

impl OccurrenceState {
    /// Whether a sweep is running, whose result is still to land.
    pub(crate) const fn is_sweeping(&self) -> bool {
        matches!(self, Self::Scanning | Self::Refreshing(_))
    }
}

/// The state a new sweep starts in: after the editor's own write (`keep`)
/// the last index stays in use until the sweep lands; anything else scans
/// afresh.
pub(crate) fn rescan_state(previous: Option<OccurrenceState>, keep: bool) -> OccurrenceState {
    match previous {
        Some(OccurrenceState::Ready(index) | OccurrenceState::Refreshing(index)) if keep => {
            OccurrenceState::Refreshing(index)
        }
        _ => OccurrenceState::Scanning,
    }
}

/// Spawns a transient `@celesta/cli` process with source locations and
/// sweeps the whole composition for where each element renders. Runs on its
/// own thread because `ReactBridge::spawn` and the sweep both block; a newer
/// request abandons the running sweep between chunks.
pub(crate) struct OccurrenceWorker {
    pub(crate) requests: mpsc::Sender<OccurrenceRequest>,
    pub(crate) results: mpsc::Receiver<OccurrenceResult>,
}

impl OccurrenceWorker {
    pub(crate) fn spawn() -> Result<Self, Box<dyn Error>> {
        let (request_tx, request_rx) = mpsc::channel::<OccurrenceRequest>();
        let (result_tx, result_rx) = mpsc::channel::<OccurrenceResult>();
        thread::Builder::new()
            .name("celesta-occurrences".to_owned())
            .spawn(move || {
                // A request that arrived during a sweep, which it abandoned.
                let mut pending: Option<OccurrenceRequest> = None;
                loop {
                    let first = match pending.take() {
                        Some(request) => request,
                        None => match request_rx.recv() {
                            Ok(request) => request,
                            Err(_) => break,
                        },
                    };
                    let request = take_latest(first, &request_rx);
                    let index = match ReactBridge::spawn_with_options(
                        &request.node,
                        &request.cli_script,
                        &request.entry,
                        &request.properties,
                        SpawnOptions {
                            composition_id: Some(request.composition_id),
                            source_locations: true,
                        },
                    ) {
                        Ok(mut bridge) => {
                            bridge.set_preview(true);
                            let swept = sweep(
                                request.duration_in_frames,
                                |from, frames| {
                                    bridge
                                        .collect_occurrences(from, frames)
                                        .map_err(|error| error_chain(&error))
                                },
                                || match request_rx.try_recv() {
                                    Ok(next) => {
                                        pending = Some(next);
                                        true
                                    }
                                    Err(_) => false,
                                },
                            );
                            match swept {
                                Some(index) => index,
                                None => continue,
                            }
                        }
                        Err(error) => Err(error_chain(&error)),
                    };
                    if result_tx
                        .send(OccurrenceResult {
                            generation: request.generation,
                            index,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests: request_tx,
            results: result_rx,
        })
    }

    pub(crate) fn request(&self, request: OccurrenceRequest) -> Result<(), String> {
        self.requests
            .send(request)
            .map_err(|_| "occurrence worker stopped unexpectedly".to_owned())
    }
}

/// Asks `chunk` for `[from, from + frames)` in [`CHUNK_FRAMES`] pieces over
/// `duration` frames and merges what each finds. `None` when `cancelled()`
/// turned true between chunks; the first chunk's error ends the sweep.
pub(crate) fn sweep(
    duration: u64,
    mut chunk: impl FnMut(u64, u64) -> Result<OccurrenceIndex, String>,
    mut cancelled: impl FnMut() -> bool,
) -> Option<Result<OccurrenceIndex, String>> {
    let mut index = OccurrenceIndex::default();
    let mut from = 0;
    while from < duration {
        if from > 0 && cancelled() {
            return None;
        }
        let frames = CHUNK_FRAMES.min(duration - from);
        match chunk(from, frames) {
            Ok(found) => index.merge(found),
            Err(error) => return Some(Err(error)),
        }
        from += frames;
    }
    Some(Ok(index))
}

/// Whether a sweep for reload `result` lands: it belongs to the `current`
/// reload and an index is still `wanted` (Select layers was not turned off
/// while it ran).
pub(crate) const fn accepts(current: u64, result: u64, wanted: bool) -> bool {
    wanted && current == result
}

/// Whether a sweep held back until preview frame `after` may start, now
/// that frame `shown` is drawn.
pub(crate) fn sweep_due(after: Option<u64>, shown: u64) -> bool {
    after.is_some_and(|after| shown >= after)
}

/// Puts a sweep's `result` in `slot` if it lands (see [`accepts`]), for
/// reload `current`; whether it did.
pub(crate) fn land(
    slot: &mut Option<(u64, OccurrenceState)>,
    current: u64,
    result: OccurrenceResult,
) -> bool {
    if !accepts(current, result.generation, slot.is_some()) {
        return false;
    }
    let state = match result.index {
        Ok(index) => OccurrenceState::Ready(index),
        Err(error) => OccurrenceState::Failed(error),
    };
    *slot = Some((result.generation, state));
    true
}

/// Whether `slot` holds a sweep whose result is still to land.
fn sweeping(slot: Option<&(u64, OccurrenceState)>) -> bool {
    slot.is_some_and(|(_, state)| state.is_sweeping())
}

impl EditorView {
    /// Starts sweeping the React entry for where each element renders,
    /// tagged with the current reload. Only a preview that picks layers
    /// (Select layers on) can select anything to show the impact of; any
    /// other state has nothing to sweep. `keep` (the editor's own write)
    /// keeps the last index in use, and holds the sweep back until the
    /// preview draws the reloaded code, so the two do not compete while the
    /// written values are on their way to the screen.
    pub(crate) fn refresh_occurrences(&mut self, keep: bool) {
        if self.react_preview.is_none() || !self.picks_layers() {
            self.occurrences = None;
            self.occurrence_sweep_after = None;
            return;
        }
        let state = rescan_state(self.occurrences.take().map(|(_, state)| state), keep);
        self.occurrences = Some((self.react_reload_generation, state));
        if keep {
            self.occurrence_sweep_after = Some(self.preview_generation);
        } else {
            self.occurrence_sweep_after = None;
            self.request_occurrence_sweep();
        }
    }

    /// Starts a sweep held back by `refresh_occurrences` once the preview has
    /// drawn the frame it waited for.
    pub(crate) fn start_due_occurrence_sweep(&mut self) {
        if sweep_due(self.occurrence_sweep_after, self.preview_shown_generation) {
            self.occurrence_sweep_after = None;
            self.request_occurrence_sweep();
        }
    }

    /// Takes in the sweep results the worker sent; whether the state changed.
    pub(crate) fn poll_occurrence_results(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.occurrence_worker.results.try_recv() {
                Ok(result) => {
                    changed |= land(&mut self.occurrences, self.react_reload_generation, result);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if let Some((_, state)) = self
                        .occurrences
                        .as_mut()
                        .filter(|(_, state)| state.is_sweeping())
                    {
                        *state = OccurrenceState::Failed(
                            "occurrence worker stopped unexpectedly".to_owned(),
                        );
                        changed = true;
                    }
                    break;
                }
            }
        }
        changed
    }

    /// While a sweep runs, looks for its result every [`RESULT_POLL`] and
    /// redraws only when it lands, instead of redrawing every frame for the
    /// whole sweep. One watch runs at a time; it ends with the sweep.
    pub(crate) fn watch_occurrence_sweep(&mut self, cx: &mut Context<Self>) {
        if self.occurrence_watch || !sweeping(self.occurrences.as_ref()) {
            return;
        }
        self.occurrence_watch = true;
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor().timer(RESULT_POLL).await;
                let watching = view.update(cx, |this, cx| {
                    if this.poll_occurrence_results() {
                        cx.notify();
                    }
                    this.occurrence_watch = sweeping(this.occurrences.as_ref());
                    this.occurrence_watch
                });
                if !matches!(watching, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    fn request_occurrence_sweep(&mut self) {
        let Some(react) = &self.react_preview else {
            return;
        };
        let generation = self.react_reload_generation;
        let (node, cli_script) = react_runtime_paths();
        if let Err(error) = self.occurrence_worker.request(OccurrenceRequest {
            generation,
            node,
            cli_script,
            entry: react.entry.clone(),
            properties: react.properties.clone(),
            composition_id: react.composition_id.clone(),
            duration_in_frames: u64::try_from(self.clock.end_frame()).unwrap_or(0),
        }) {
            self.occurrences = Some((generation, OccurrenceState::Failed(error)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_react_bridge::{ElementOccurrences, InstanceOccurrences, SourcePos};

    /// An index in which one element renders over `[from, to)`.
    fn rendered(from: u64, to: u64) -> OccurrenceIndex {
        let element = SourcePos {
            file: "/a/title.tsx".into(),
            line: 3,
            column: 4,
        };
        OccurrenceIndex {
            elements: vec![ElementOccurrences {
                element: element.clone(),
                instances: vec![InstanceOccurrences {
                    chain: vec![element],
                    ranges: vec![(from, to)],
                    max_per_frame: 1,
                }],
            }],
        }
    }

    #[test]
    fn sweep_asks_for_every_chunk_and_merges() {
        let mut calls = Vec::new();
        let index = sweep(
            500,
            |from, frames| {
                calls.push((from, frames));
                Ok(rendered(from, from + frames))
            },
            || false,
        );
        assert_eq!(calls, [(0, 240), (240, 240), (480, 20)]);
        assert_eq!(index, Some(Ok(rendered(0, 500))));
    }

    #[test]
    fn sweep_stops_between_chunks_when_cancelled() {
        let mut calls = 0;
        let index = sweep(
            500,
            |from, frames| {
                calls += 1;
                Ok(rendered(from, from + frames))
            },
            || true,
        );
        assert_eq!(index, None);
        assert_eq!(calls, 1);
    }

    #[test]
    fn sweep_reports_the_first_chunk_error() {
        let index = sweep(
            500,
            |from, frames| {
                if from == 240 {
                    Err("frame 400: boom".to_owned())
                } else {
                    Ok(rendered(from, from + frames))
                }
            },
            || false,
        );
        assert_eq!(index, Some(Err("frame 400: boom".to_owned())));
    }

    #[test]
    fn a_result_for_an_older_reload_is_dropped() {
        assert!(!accepts(3, 2, true));
        assert!(accepts(3, 3, true));
    }

    #[test]
    fn a_result_after_select_layers_turned_off_is_dropped() {
        assert!(!accepts(3, 3, false));
    }

    #[test]
    fn an_own_write_keeps_the_last_scan_until_the_new_one_lands() {
        let last = rendered(0, 30);
        assert_eq!(
            rescan_state(Some(OccurrenceState::Ready(last.clone())), true),
            OccurrenceState::Refreshing(last.clone())
        );
        // A second write during the re-sweep still keeps it.
        assert_eq!(
            rescan_state(Some(OccurrenceState::Refreshing(last.clone())), true),
            OccurrenceState::Refreshing(last)
        );
    }

    #[test]
    fn an_own_write_without_a_ready_scan_scans_afresh() {
        assert_eq!(rescan_state(None, true), OccurrenceState::Scanning);
        assert_eq!(
            rescan_state(Some(OccurrenceState::Scanning), true),
            OccurrenceState::Scanning
        );
        assert_eq!(
            rescan_state(Some(OccurrenceState::Failed("boom".to_owned())), true),
            OccurrenceState::Scanning
        );
    }

    #[test]
    fn a_held_back_sweep_waits_for_the_reloaded_frame() {
        assert!(!sweep_due(None, 9));
        // The reload asked for preview frame 7; frame 6 is the old code's.
        assert!(!sweep_due(Some(7), 6));
        assert!(sweep_due(Some(7), 7));
        // A frame drawn later than the one waited for also lets it start.
        assert!(sweep_due(Some(7), 8));
    }

    #[test]
    fn a_result_for_the_current_reload_lands_and_ends_the_watch() {
        let result = |generation, index| OccurrenceResult { generation, index };
        let mut slot = Some((3, OccurrenceState::Scanning));
        assert!(sweeping(slot.as_ref()));

        // An older reload's result changes nothing, so nothing is redrawn.
        assert!(!land(&mut slot, 3, result(2, Ok(rendered(0, 30)))));
        assert_eq!(slot, Some((3, OccurrenceState::Scanning)));

        assert!(land(&mut slot, 3, result(3, Ok(rendered(0, 30)))));
        assert_eq!(slot, Some((3, OccurrenceState::Ready(rendered(0, 30)))));
        assert!(!sweeping(slot.as_ref()));

        let mut failed = Some((3, OccurrenceState::Refreshing(rendered(0, 30))));
        assert!(land(&mut failed, 3, result(3, Err("boom".to_owned()))));
        assert_eq!(
            failed,
            Some((3, OccurrenceState::Failed("boom".to_owned())))
        );
    }

    #[test]
    fn a_result_after_select_layers_turned_off_does_not_land() {
        let mut slot = None;
        assert!(!land(
            &mut slot,
            3,
            OccurrenceResult {
                generation: 3,
                index: Ok(rendered(0, 30)),
            }
        ));
        assert_eq!(slot, None);
        assert!(!sweeping(slot.as_ref()));
    }

    #[test]
    fn a_kept_scan_is_still_sweeping() {
        assert!(OccurrenceState::Scanning.is_sweeping());
        assert!(OccurrenceState::Refreshing(rendered(0, 30)).is_sweeping());
        assert!(!OccurrenceState::Ready(rendered(0, 30)).is_sweeping());
        assert!(!OccurrenceState::Failed("boom".to_owned()).is_sweeping());
    }

    #[test]
    fn an_outside_reload_scans_afresh() {
        assert_eq!(
            rescan_state(Some(OccurrenceState::Ready(rendered(0, 30))), false),
            OccurrenceState::Scanning
        );
        assert_eq!(
            rescan_state(Some(OccurrenceState::Refreshing(rendered(0, 30))), false),
            OccurrenceState::Scanning
        );
    }
}
