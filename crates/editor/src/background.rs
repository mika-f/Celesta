use crate::audio::{AudioMixRequest, AudioMixResult};
use crate::export_worker::ExportEvent;
use crate::helpers::export_loudness_label;
use crate::layer_select::source_lookup;
use crate::preview::{AudioPreview, PreviewResult};
use crate::problems::{Source, error_chain};
use crate::react_audio::ReactAudioResult;
use crate::refresh::LookupFate;
use crate::view::EditorView;
use celesta_exporter::ExportProgress;
use std::collections::HashMap;

impl EditorView {
    pub(crate) fn poll_background_work(&mut self) {
        self.poll_preview_results();
        // The reloaded code is on screen: a sweep held back for it starts.
        self.start_due_occurrence_sweep();
        self.poll_media_probe_results();
        self.poll_component_schema_results();
        self.poll_react_audio_results();
        self.poll_occurrence_results();
        self.poll_audio_mix_results();
        self.poll_export_events();
    }

    fn poll_preview_results(&mut self) {
        let polled = self
            .preview_worker
            .poll_newer_than(self.preview_shown_generation);
        for (generation, result) in polled.results {
            self.preview_shown_generation = generation;
            self.show_preview_result(generation, result);
        }
        if let Some(error) = polled.stopped {
            self.problems.fail(Source::Preview, error);
            // No lookup answers again: the held row commits are dropped on
            // the next render (`settle_row_requests`).
            self.element_edit.preview_stopped();
        }
    }

    fn show_preview_result(&mut self, generation: u64, result: PreviewResult) {
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
            match self.selection_keep.lookup_arrived(generation, absent) {
                LookupFate::Stale => {}
                LookupFate::Gone => self.clear_layer_selection(false),
                // An added layer selected by its place under the parent
                // shows some other element: not the one added after all.
                LookupFate::Show if !self.added_lookup_holds(&id, &source) => {
                    self.clear_layer_selection(false);
                }
                LookupFate::Show => {
                    self.source_lookup = Some(source_lookup(source));
                    self.source_lookup_frame = result.timeline_frame;
                    self.source_lookup_stale = false;
                    self.carry_scope_choice();
                }
            }
        }
        // After the lookup, so an added layer found here replaces the
        // selection the lookup was for.
        self.take_add_answers(generation, result.parent_space, result.added);
    }

    fn poll_media_probe_results(&mut self) {
        let polled = self.media_probe_worker.poll();
        for assets in polled.results {
            self.media_cache.extend(assets);
        }
        if let Some(error) = polled.stopped {
            self.problems.fail(Source::Media, error);
        }
    }

    fn poll_component_schema_results(&mut self) {
        let polled = self.component_schema_worker.poll();
        for result in polled.results {
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
        if let Some(error) = polled.stopped {
            self.problems.fail(Source::Schemas, error);
        }
    }

    fn poll_react_audio_results(&mut self) {
        let polled = self.react_audio_worker.poll();
        for media in polled.results {
            self.take_react_audio(media);
        }
        if let Some(error) = polled.stopped {
            self.problems.fail(Source::Audio, error);
        }
    }

    /// Mixes a React entry's swept audio graph, or clears the audio when it
    /// has no clips or the sweep failed.
    fn take_react_audio(&mut self, media: ReactAudioResult) {
        let media = match media {
            Ok(media) if media.graph.clips.is_empty() => {
                self.clear_audio_preview();
                self.problems.clear(Source::Audio);
                return;
            }
            Ok(media) => media,
            Err(error) => {
                self.clear_audio_preview();
                self.problems.fail(Source::Audio, error);
                return;
            }
        };
        self.set_react_audio_clips(media.graph.clips.clone(), &media.videos);
        if let Err(error) = self.audio_mix_worker.request(AudioMixRequest {
            graph: media.graph,
            asset_root: self.document.asset_root().to_owned(),
            duration: self.document.duration(),
        }) {
            self.problems.fail(Source::Audio, error);
        }
    }

    fn clear_audio_preview(&mut self) {
        self.audio_preview = None;
        self.clip_waveforms.clear();
        self.clip_levels.clear();
        self.set_react_audio_clips(Vec::new(), &HashMap::new());
    }

    fn poll_audio_mix_results(&mut self) {
        let polled = self.audio_mix_worker.poll();
        for output in polled.results {
            self.take_audio_mix(output);
        }
        if let Some(error) = polled.stopped {
            self.clip_waveforms.clear();
            self.clip_levels.clear();
            self.audio_preview = None;
            self.problems.fail(Source::Audio, error);
        }
    }

    fn take_audio_mix(&mut self, output: AudioMixResult) {
        match output.and_then(|output| {
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

    fn poll_export_events(&mut self) {
        let polled = self.export_worker.poll();
        for event in polled.results {
            self.take_export_event(event);
        }
        if let Some(error) = polled.stopped
            && self.export_cancellation.is_some()
        {
            self.export_progress = None;
            self.export_path = None;
            self.export_cancellation = None;
            self.export_cancelling = false;
            self.problems.fail(Source::Export, error);
        }
    }

    fn take_export_event(&mut self, event: ExportEvent) {
        match event {
            // The preview already lists font fallbacks; `Finished` reports
            // the output, and the label stays on the last stage until then.
            ExportEvent::Progress(
                ExportProgress::Warning(_)
                | ExportProgress::Composition(_)
                | ExportProgress::ReactCompositions { .. }
                | ExportProgress::Wrote(_),
            ) => {}
            ExportEvent::Progress(progress) => {
                self.export_progress = Some(progress);
            }
            ExportEvent::Finished {
                output,
                result,
                cancelled,
                loudness,
            } => {
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
                        // The progress disappearing shows it ended; a toast
                        // would only repeat what the user did.
                        self.problems
                            .info_without_toast(Source::Export, "Export cancelled", "");
                    }
                    Err(error) => {
                        self.export_path = None;
                        self.problems.fail(Source::Export, error);
                    }
                }
            }
        }
    }
}
