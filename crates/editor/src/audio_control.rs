use crate::audio::{AudioMixRequest, MasterVolumeDrag};
use crate::react_audio::ReactAudioRequest;
use crate::view::EditorView;
use crate::waveform::master_volume_from_drag;
use celesta_react_bridge::runtime_paths as react_runtime_paths;
use gpui_kit::{Context, KeyDownEvent, MouseDownEvent, MouseMoveEvent, Window};

impl EditorView {
    pub(crate) fn refresh_audio_preview(&mut self) {
        self.audio_generation = self.audio_generation.wrapping_add(1);
        let generation = self.audio_generation;
        self.audio_mix_worker.cancel_before(generation);
        self.audio_pending = false;
        self.audio_preview = None;
        self.master_levels = None;
        self.clip_levels.clear();

        if let Some(react) = &self.react_preview {
            // No tracks to evaluate: sweep the React entry for its `<Audio>`
            // declarations on the dedicated worker, then feed the resulting
            // graph into the shared mix pipeline (see `poll_background_work`).
            let (node, cli_script) = react_runtime_paths();
            self.clip_waveforms.clear();
            self.audio_pending = true;
            self.audio_error = None;
            if let Err(error) = self.react_audio_worker.request(ReactAudioRequest {
                generation,
                node,
                cli_script,
                entry: react.entry.clone(),
                properties: react.properties.clone(),
                sample_rate: self.document.project().settings.sample_rate,
                master_volume: self.document.master_volume(),
            }) {
                self.audio_pending = false;
                self.audio_error = Some(error.into());
            }
            return;
        }

        match self.document.audio_graph() {
            Ok(graph) if graph.clips.is_empty() => {
                self.clip_waveforms.clear();
                self.clip_levels.clear();
                self.audio_error = None;
            }
            Ok(graph) => {
                self.audio_pending = true;
                self.audio_error = None;
                if let Err(error) = self.audio_mix_worker.request(AudioMixRequest {
                    generation,
                    graph,
                    asset_root: self.document.asset_root().to_owned(),
                    duration: self.document.duration(),
                }) {
                    self.audio_pending = false;
                    self.audio_error = Some(error.into());
                }
            }
            Err(error) => {
                self.clip_waveforms.clear();
                self.clip_levels.clear();
                self.audio_error = Some(error.to_string().into());
            }
        }
    }

    pub(crate) fn toggle_track_mute(&mut self, track_id: &str, cx: &mut Context<Self>) {
        match self.document.toggle_track_muted(track_id) {
            Ok(_) => {
                self.tracks = self.document.tracks();
                self.refresh_audio_preview();
            }
            Err(error) => self.audio_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    pub(crate) fn toggle_track_solo(&mut self, track_id: &str, cx: &mut Context<Self>) {
        match self.document.toggle_track_solo(track_id) {
            Ok(_) => {
                self.tracks = self.document.tracks();
                self.refresh_audio_preview();
            }
            Err(error) => self.audio_error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    pub(crate) fn adjust_master_volume(&mut self, delta_percent: i32, cx: &mut Context<Self>) {
        let current_percent = (self.monitor_volume * 100.0).round() as i32;
        let next_percent = current_percent.saturating_add(delta_percent).clamp(0, 200);
        self.set_master_volume(f64::from(next_percent) / 100.0, cx);
    }

    pub(crate) fn set_master_volume(&mut self, volume: f64, cx: &mut Context<Self>) {
        self.monitor_volume = volume.clamp(0.0, 2.0);
        if let Some(audio) = &mut self.audio_preview {
            audio.set_volume(self.monitor_volume);
        }
        cx.notify();
    }

    pub(crate) fn begin_master_volume_drag(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(focus) = &self.master_volume_focus {
            focus.focus(window, cx);
        }
        self.master_volume_drag = Some(MasterVolumeDrag {
            pointer_x: event.position.x,
            start_volume: self.monitor_volume,
        });
        cx.notify();
    }

    pub(crate) fn continue_master_volume_drag(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            return;
        }
        let Some(drag) = &self.master_volume_drag else {
            return;
        };
        let volume = master_volume_from_drag(
            drag.start_volume,
            f64::from(event.position.x - drag.pointer_x),
        );
        if (volume - self.monitor_volume).abs() >= f64::EPSILON {
            self.set_master_volume(volume, cx);
        }
    }

    pub(crate) fn master_volume_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let step = if event.keystroke.modifiers.shift {
            10
        } else {
            5
        };
        match event.keystroke.key.as_str() {
            "left" | "down" => self.adjust_master_volume(-step, cx),
            "right" | "up" => self.adjust_master_volume(step, cx),
            "home" => self.set_master_volume(0.0, cx),
            "end" => self.set_master_volume(2.0, cx),
            _ => return,
        }
        cx.stop_propagation();
    }
}
