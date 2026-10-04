use crate::audio::AudioMixRequest;
use crate::export_worker::ExportEvent;
use crate::preview::AudioPreview;
use crate::view::EditorView;
use celesta_exporter::ExportProgress;
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
                            self.preview_error = None;
                        }
                        Err(error) => {
                            self.preview_error = Some(error.into());
                        }
                    }
                    self.preview_warnings = result.warnings.into_iter().map(Into::into).collect();
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.preview_pending {
                        self.preview_pending = false;
                        self.preview_error = Some("preview worker stopped unexpectedly".into());
                    }
                    break;
                }
            }
        }

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
                        self.media_error = Some("media probe worker stopped unexpectedly".into());
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
                            self.component_schema_error = None;
                        }
                        Err(error) => {
                            self.component_schemas.clear();
                            self.project_property_schema = None;
                            self.component_schema_error = Some(error.into());
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.component_schema_pending {
                        self.component_schema_pending = false;
                        self.component_schema_error =
                            Some("component schema worker stopped unexpectedly".into());
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
                    match result.graph {
                        Ok(graph) if graph.clips.is_empty() => {
                            self.audio_pending = false;
                            self.audio_preview = None;
                            self.clip_waveforms.clear();
                            self.clip_levels.clear();
                            self.audio_error = None;
                        }
                        Ok(graph) => {
                            if let Err(error) = self.audio_mix_worker.request(AudioMixRequest {
                                generation: self.audio_generation,
                                graph,
                                asset_root: self.document.asset_root().to_owned(),
                                duration: self.document.duration(),
                            }) {
                                self.audio_pending = false;
                                self.audio_error = Some(error.into());
                            }
                        }
                        Err(error) => {
                            self.audio_pending = false;
                            self.audio_preview = None;
                            self.clip_waveforms.clear();
                            self.clip_levels.clear();
                            self.audio_error = Some(error.into());
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.audio_pending {
                        self.audio_pending = false;
                        self.audio_error = Some("react audio worker stopped unexpectedly".into());
                    }
                    break;
                }
            }
        }

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
                            .map_err(|error| error.to_string())
                    }) {
                        Ok(mut preview) => {
                            if let Err(error) = preview.seek(self.current_time(), self.playing) {
                                self.audio_preview = None;
                                self.audio_error = Some(error.to_string().into());
                            } else {
                                self.audio_preview = Some(preview);
                                self.audio_error = None;
                            }
                        }
                        Err(error) => {
                            self.clip_waveforms.clear();
                            self.clip_levels.clear();
                            self.master_levels = None;
                            self.audio_preview = None;
                            self.audio_error = Some(error.into());
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
                        self.audio_error = Some("audio mix worker stopped unexpectedly".into());
                    }
                    break;
                }
            }
        }

        loop {
            match self.export_worker.events.try_recv() {
                // The preview already lists font fallbacks.
                Ok(ExportEvent::Progress(ExportProgress::Warning(_))) => {}
                Ok(ExportEvent::Progress(progress)) => {
                    self.export_progress = Some(progress);
                }
                Ok(ExportEvent::Finished {
                    output,
                    result,
                    cancelled,
                }) => {
                    self.export_progress = None;
                    self.export_cancellation = None;
                    self.export_cancelling = false;
                    match result {
                        Ok(()) => {
                            self.export_error = None;
                            self.export_path = Some(output.clone());
                            self.export_message =
                                Some(format!("Exported {}", output.display()).into());
                        }
                        Err(_) if cancelled => {
                            self.export_path = None;
                            self.export_error = None;
                            self.export_message = Some("Export cancelled".into());
                        }
                        Err(error) => {
                            self.export_path = None;
                            self.export_message = None;
                            self.export_error = Some(error.into());
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
                        self.export_message = None;
                        self.export_error = Some("export worker stopped unexpectedly".into());
                    }
                    break;
                }
            }
        }
    }
}
