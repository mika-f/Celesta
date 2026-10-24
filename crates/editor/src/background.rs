use crate::audio::AudioMixRequest;
use crate::export_worker::ExportEvent;
use crate::helpers::export_loudness_label;
use crate::layer_select::source_lookup;
use crate::preview::AudioPreview;
use crate::problems::{Source, error_chain};
use crate::refresh::LookupFate;
use crate::view::EditorView;
use celesta_exporter::ExportProgress;
use std::collections::HashMap;
use std::sync::mpsc;

impl EditorView {
    pub(crate) fn poll_background_work(&mut self) {
        loop {
            match self.preview_worker.results.try_recv() {
                Ok(result) => {
                    // Accept every result whose generation moves forward, not
                    // just the newest request's: while a render is slower than
                    // the frame interval the newest request is always still in
                    // flight, and requiring an exact match would drop every
                    // frame until playback stopped.
                    if result.generation <= self.preview_shown_generation {
                        continue;
                    }
                    self.preview_shown_generation = result.generation;
                    if result.generation == self.preview_generation {
                        self.preview_pending = false;
                    }
                    match result.frame {
                        Ok(preview) => {
                            self.preview = Some(preview);
                            self.problems.clear(Source::Preview);
                        }
                        Err(error) => {
                            self.problems.fail(Source::Preview, error);
                        }
                    }
                    self.problems.set_preview_warnings(result.warnings);
                    self.hit_regions = result.hit_regions;
                    self.layer_frame = result.layer_frame;
                    self.text_layout = result.text_layout;
                    self.refit_inline_text();
                    // An answer for a layer that is no longer selected is stale.
                    if let Some((id, source)) = result.layer_source
                        && self.selected_layer.as_ref() == Some(&id)
                    {
                        let absent = matches!(source, Ok(None));
                        match self
                            .selection_keep
                            .lookup_arrived(result.generation, absent)
                        {
                            LookupFate::Stale => {}
                            LookupFate::Gone => self.clear_layer_selection(false),
                            LookupFate::Show => {
                                self.source_lookup = Some(source_lookup(source));
                                self.source_lookup_frame = result.timeline_frame;
                                self.source_lookup_stale = false;
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.preview_pending {
                        self.preview_pending = false;
                        self.problems
                            .fail(Source::Preview, "preview worker stopped unexpectedly");
                    }
                    // No lookup answers again: the held row commits are
                    // dropped on the next render (`settle_row_requests`).
                    self.element_edit.preview_stopped();
                    break;
                }
            }
        }
        // The reloaded code is on screen: a sweep held back for it starts.
        self.start_due_occurrence_sweep();

        loop {
            match self.media_probe_worker.results.try_recv() {
                Ok(result) => {
                    if result.generation != self.media_generation {
                        continue;
                    }
                    self.media_pending = false;
                    self.media_cache.extend(result.assets);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.media_pending {
                        self.media_pending = false;
                        self.problems
                            .fail(Source::Media, "media probe worker stopped unexpectedly");
                    }
                    break;
                }
            }
        }

        loop {
            match self.component_schema_worker.results.try_recv() {
                Ok(result) => {
                    if result.generation != self.component_schema_generation {
                        continue;
                    }
                    self.component_schema_pending = false;
                    match result.schemas {
                        Ok(schemas) => {
                            self.component_schemas = schemas;
                            self.project_property_schema = result.project_property_schema;
                            self.problems.clear(Source::Schemas);
                        }
                        Err(error) => {
                            self.component_schemas.clear();
                            self.project_property_schema = None;
                            self.problems.fail(Source::Schemas, error);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.component_schema_pending {
                        self.component_schema_pending = false;
                        self.problems.fail(
                            Source::Schemas,
                            "component schema worker stopped unexpectedly",
                        );
                    }
                    break;
                }
            }
        }

        loop {
            match self.react_audio_worker.results.try_recv() {
                Ok(result) => {
                    if result.generation != self.audio_generation {
                        continue;
                    }
                    match result.media {
                        Ok(media) if media.graph.clips.is_empty() => {
                            self.audio_pending = false;
                            self.audio_preview = None;
                            self.clip_waveforms.clear();
                            self.clip_levels.clear();
                            self.set_react_audio_clips(Vec::new(), &HashMap::new());
                            self.problems.clear(Source::Audio);
                        }
                        Ok(media) => {
                            self.set_react_audio_clips(media.graph.clips.clone(), &media.videos);
                            if let Err(error) = self.audio_mix_worker.request(AudioMixRequest {
                                generation: self.audio_generation,
                                graph: media.graph,
                                asset_root: self.document.asset_root().to_owned(),
                                duration: self.document.duration(),
                            }) {
                                self.audio_pending = false;
                                self.problems.fail(Source::Audio, error);
                            }
                        }
                        Err(error) => {
                            self.audio_pending = false;
                            self.audio_preview = None;
                            self.clip_waveforms.clear();
                            self.clip_levels.clear();
                            self.set_react_audio_clips(Vec::new(), &HashMap::new());
                            self.problems.fail(Source::Audio, error);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.audio_pending {
                        self.audio_pending = false;
                        self.problems
                            .fail(Source::Audio, "react audio worker stopped unexpectedly");
                    }
                    break;
                }
            }
        }

        self.poll_occurrence_results();

        loop {
            match self.audio_mix_worker.results.try_recv() {
                Ok(result) => {
                    if result.generation != self.audio_generation {
                        continue;
                    }
                    self.audio_pending = false;
                    match result.output.and_then(|output| {
                        self.clip_waveforms = output.clip_waveforms;
                        self.clip_levels = output.clip_levels;
                        self.master_levels = Some(output.master_levels);
                        AudioPreview::from_buffer(output.buffer, self.monitor_volume)
                            .map_err(|error| error_chain(error.as_ref()))
                    }) {
                        Ok(mut preview) => {
                            if let Err(error) = preview.seek(self.current_time(), self.playing) {
                                self.audio_preview = None;
                                self.problems
                                    .fail(Source::Audio, error_chain(error.as_ref()));
                            } else {
                                self.audio_preview = Some(preview);
                                self.problems.clear(Source::Audio);
                            }
                        }
                        Err(error) => {
                            self.clip_waveforms.clear();
                            self.clip_levels.clear();
                            self.master_levels = None;
                            self.audio_preview = None;
                            self.problems.fail(Source::Audio, error);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.audio_pending {
                        self.audio_pending = false;
                        self.clip_waveforms.clear();
                        self.clip_levels.clear();
                        self.audio_preview = None;
                        self.problems
                            .fail(Source::Audio, "audio mix worker stopped unexpectedly");
                    }
                    break;
                }
            }
        }

        loop {
            match self.export_worker.events.try_recv() {
                // The preview already lists font fallbacks; `Finished` reports
                // the output, and the label stays on the last stage until then.
                Ok(ExportEvent::Progress(
                    ExportProgress::Warning(_)
                    | ExportProgress::Composition(_)
                    | ExportProgress::ReactCompositions { .. }
                    | ExportProgress::Wrote(_),
                )) => {}
                Ok(ExportEvent::Progress(progress)) => {
                    self.export_progress = Some(progress);
                }
                Ok(ExportEvent::Finished {
                    output,
                    result,
                    cancelled,
                    loudness,
                }) => {
                    self.export_progress = None;
                    self.export_cancellation = None;
                    self.export_cancelling = false;
                    match result {
                        Ok(()) => {
                            self.problems.clear(Source::Export);
                            let mut detail = output.display().to_string();
                            if let Some(loudness) = &loudness {
                                detail.push_str(" · ");
                                detail.push_str(&export_loudness_label(loudness));
                            }
                            self.problems
                                .info(Source::Export, "Export finished", detail);
                            self.export_path = Some(output);
                        }
                        Err(_) if cancelled => {
                            self.export_path = None;
                            self.problems.clear(Source::Export);
                            // The progress disappearing shows it ended; a
                            // toast would only repeat what the user did.
                            self.problems.info_without_toast(
                                Source::Export,
                                "Export cancelled",
                                "",
                            );
                        }
                        Err(error) => {
                            self.export_path = None;
                            self.problems.fail(Source::Export, error);
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.export_cancellation.is_some() {
                        self.export_progress = None;
                        self.export_path = None;
                        self.export_cancellation = None;
                        self.export_cancelling = false;
                        self.problems
                            .fail(Source::Export, "export worker stopped unexpectedly");
                    }
                    break;
                }
            }
        }
    }
}
