//! `MediaBackend` routing probes and decodes to the decoders of a
//! [`MediaRegistry`], with in-process fake plugins that journal what they
//! are asked.

use crate::backend::MediaBackend;
use crate::decoder::{AudioDecoder, VideoFrameDecoder};
use crate::error::MediaError;
use crate::provider::{
    ComponentDescription, Container, EncoderId, EncoderSettings, MediaProvider, MuxRequest,
    PixelFormat, VideoRequest,
};
use crate::registry::{
    ComponentRef, DecoderUse, MediaRegistry, RegisteredPlugin, registry, set_registry,
};
use crate::types::{AudioBuffer, MediaProbe, VideoFrame, VideoStream};
use crate::{ProviderFactory, VideoColor};
use celesta_composition::{Rational, Time};
use celesta_plugin_host::{MediaDecoderEntry, MediaOrigin, PluginKillSwitch};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// What every fake plugin was asked, in order: `start <plugin>`, then
/// `probe`, `video`, `audio`, and `open` naming the decoder or plugin asked.
#[derive(Clone, Default)]
struct Journal(Arc<Mutex<Vec<String>>>);

impl Journal {
    fn push(&self, entry: String) {
        self.0.lock().unwrap().push(entry);
    }

    fn entries(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }

    fn count(&self, entry: &str) -> usize {
        self.entries().iter().filter(|seen| *seen == entry).count()
    }
}

/// How a fake plugin answers `probe`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Answer {
    /// Reads the file, and decodes audio in the format asked for.
    Reads,
    /// Reads the file, but decodes audio at this sample rate whatever is
    /// asked.
    ReadsAudioAt(u32),
    Unsupported,
    Stopped,
    /// Every process answers as one that died would.
    Unavailable,
    /// The first process answers as one that died would; later ones read.
    UnavailableOnce,
    Broken,
    /// Reads the file and finds no video in it.
    NoVideoStream,
    /// Finds no file at the path.
    NotFound,
    /// Reads the file, but fails to decode its frames.
    ReadsButNotItsFrames,
    /// Reads the file, but answers that its frames are not ones it reads,
    /// which a decoder should only say to a probe.
    ReadsButNotItsFramesUnsupported,
    /// No process starts: the factory fails.
    CannotStart,
}

struct Fake {
    plugin: String,
    answer: Answer,
    journal: Journal,
}

/// The probe plugin `plugin` answers: its video's codec names it.
fn probe_of(plugin: &str) -> MediaProbe {
    MediaProbe {
        duration: Some(Time::new(2, 1)),
        video: Some(VideoStream {
            index: 0,
            codec: Some(plugin.to_owned()),
            width: 2,
            height: 1,
            frame_rate: Some(Rational::new(30, 1)),
            duration: Some(Time::new(3, 2)),
            color: VideoColor::default(),
        }),
        audio: Vec::new(),
    }
}

impl MediaProvider for Fake {
    fn describe(&mut self) -> Result<Vec<ComponentDescription>, MediaError> {
        Ok(Vec::new())
    }

    fn probe(&mut self, decoder: &str, path: &Path) -> Result<MediaProbe, MediaError> {
        self.journal
            .push(format!("probe {}/{decoder}", self.plugin));
        match self.answer {
            Answer::Reads
            | Answer::ReadsAudioAt(_)
            | Answer::ReadsButNotItsFrames
            | Answer::ReadsButNotItsFramesUnsupported => Ok(probe_of(&self.plugin)),
            Answer::Unsupported => Err(MediaError::Unsupported {
                path: path.to_owned(),
                tried: Vec::new(),
            }),
            Answer::Stopped => Err(MediaError::CodecStopped("did not answer in time".into())),
            Answer::Unavailable | Answer::UnavailableOnce => {
                Err(MediaError::CodecUnavailable("exited".into()))
            }
            Answer::Broken => Err(MediaError::Codec("broken".into())),
            Answer::NoVideoStream => Err(MediaError::NoVideoStream(path.to_owned())),
            Answer::NotFound => Err(MediaError::NotFound(path.to_owned())),
            Answer::CannotStart => unreachable!("never started"),
        }
    }

    fn decode_video(
        &mut self,
        decoder: &str,
        request: &VideoRequest<'_>,
    ) -> Result<VideoFrame, MediaError> {
        self.journal.push(format!(
            "video {}/{decoder} {:?}",
            self.plugin,
            request.sequential.map(|sequential| sequential.session)
        ));
        if self.answer == Answer::ReadsButNotItsFrames {
            return Err(MediaError::Codec("bad frame".into()));
        }
        if self.answer == Answer::ReadsButNotItsFramesUnsupported {
            return Err(MediaError::Unsupported {
                path: request.path.to_owned(),
                tried: Vec::new(),
            });
        }
        Ok(VideoFrame {
            width: 2,
            height: 1,
            pixels: Arc::new(vec![0; 8]),
        })
    }

    fn decode_audio(
        &mut self,
        decoder: &str,
        _: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<AudioBuffer, MediaError> {
        self.journal
            .push(format!("audio {}/{decoder}", self.plugin));
        let sample_rate = match self.answer {
            Answer::ReadsAudioAt(rate) => rate,
            _ => sample_rate,
        };
        Ok(AudioBuffer {
            sample_rate,
            channels,
            samples: Vec::new(),
        })
    }

    fn check_encoder(&mut self, _: Container) -> Result<(), MediaError> {
        Ok(())
    }

    fn open_encoder(&mut self, _: &EncoderSettings) -> Result<EncoderId, MediaError> {
        self.journal.push(format!("open {}", self.plugin));
        Ok(EncoderId(1))
    }

    fn encode_frame(&mut self, _: EncoderId, _: Vec<u8>) -> Result<(), MediaError> {
        Ok(())
    }

    fn finish_encoder(&mut self, _: EncoderId) -> Result<(), MediaError> {
        Ok(())
    }

    fn abort_encoder(&mut self, _: EncoderId) {}

    fn mux_audio(&mut self, _: &MuxRequest) -> Result<(), MediaError> {
        Ok(())
    }
}

/// A plugin `id` with one decoder, `dec`, reading `extensions`, whose
/// processes answer as `answer` says.
fn plugin(
    journal: &Journal,
    id: &str,
    origin: MediaOrigin,
    extensions: &[&str],
    answer: Answer,
) -> RegisteredPlugin {
    let decoder = MediaDecoderEntry {
        id: "dec".into(),
        name: "Decoder".into(),
        extensions: extensions
            .iter()
            .map(|&extension| extension.into())
            .collect(),
    };
    with_decoders(journal, id, origin, vec![decoder], answer)
}

fn with_decoders(
    journal: &Journal,
    id: &str,
    origin: MediaOrigin,
    decoders: Vec<MediaDecoderEntry>,
    answer: Answer,
) -> RegisteredPlugin {
    let started = AtomicUsize::new(0);
    let journal = journal.clone();
    let plugin = id.to_owned();
    let factory: ProviderFactory = Arc::new(move |_| {
        journal.push(format!("start {plugin}"));
        if answer == Answer::CannotStart {
            return Err(MediaError::CodecUnavailable("cannot start".into()));
        }
        let answer = match answer {
            Answer::UnavailableOnce if started.fetch_add(1, Ordering::SeqCst) > 0 => Answer::Reads,
            answer => answer,
        };
        Ok(Box::new(Fake {
            plugin: plugin.clone(),
            answer,
            journal: journal.clone(),
        }) as Box<dyn MediaProvider>)
    });
    RegisteredPlugin {
        id: id.to_owned(),
        origin,
        decoders,
        factory,
    }
}

/// `u` (user), `b` (bundled), and `p` (project), each reading `mp4`, listed
/// in the search order `p`, `b`, `u` so that only their origins put `u`
/// first.
fn three(journal: &Journal, u: Answer, b: Answer, p: Answer) -> Arc<MediaRegistry> {
    Arc::new(MediaRegistry::new(vec![
        plugin(journal, "p", MediaOrigin::Project, &["mp4"], p),
        plugin(journal, "b", MediaOrigin::Bundled, &["mp4"], b),
        plugin(journal, "u", MediaOrigin::User, &["mp4"], u),
    ]))
}

fn component(reference: &str) -> ComponentRef {
    reference.parse().unwrap()
}

/// An empty file `name` in `directory`.
fn file(directory: &tempfile::TempDir, name: &str) -> PathBuf {
    let path = directory.path().join(name);
    std::fs::write(&path, b"").unwrap();
    path
}

fn strings(entries: &[&str]) -> Vec<String> {
    entries.iter().map(|&entry| entry.to_owned()).collect()
}

#[test]
fn component_refs_read_and_write_plugin_slash_component() {
    let reference = component("ffmpeg/video");
    assert_eq!(
        reference,
        ComponentRef {
            plugin: "ffmpeg".into(),
            component: "video".into(),
        }
    );
    assert_eq!(reference.to_string(), "ffmpeg/video");
    assert_eq!(component("a/b/c").component, "b/c", "split at the first /");
    for invalid in ["", "ffmpeg", "/video", "ffmpeg/"] {
        assert!(invalid.parse::<ComponentRef>().is_err(), "{invalid:?}");
    }
}

#[test]
fn candidates_follow_extension_then_origin() {
    let journal = Journal::default();
    let registry = three(&journal, Answer::Reads, Answer::Reads, Answer::Reads);
    assert_eq!(
        registry.decoder_candidates(Path::new("/media/clip.MP4")),
        [
            (2, component("u/dec")),
            (1, component("b/dec")),
            (0, component("p/dec")),
        ]
    );
}

#[test]
fn candidates_keep_search_and_manifest_order_within_an_origin() {
    let journal = Journal::default();
    let entry = |id: &str, extension: &str| MediaDecoderEntry {
        id: id.into(),
        name: id.into(),
        extensions: vec![extension.into()],
    };
    let registry = MediaRegistry::new(vec![
        plugin(&journal, "b", MediaOrigin::Bundled, &["mov"], Answer::Reads),
        with_decoders(
            &journal,
            "u1",
            MediaOrigin::User,
            vec![entry("second", "wav"), entry("first", "mov")],
            Answer::Reads,
        ),
        plugin(&journal, "u2", MediaOrigin::User, &["mov"], Answer::Reads),
    ]);
    assert_eq!(
        registry.decoder_candidates(Path::new("/clip.mov")),
        [
            (1, component("u1/first")),
            (2, component("u2/dec")),
            (0, component("b/dec")),
        ]
    );
    assert_eq!(
        registry.decoder_candidates(Path::new("/clip")),
        [
            (1, component("u1/second")),
            (1, component("u1/first")),
            (2, component("u2/dec")),
            (0, component("b/dec")),
        ],
        "with no extension, every decoder, in the same order"
    );
}

#[test]
fn no_extension_match_tries_every_decoder() {
    let journal = Journal::default();
    let registry = Arc::new(MediaRegistry::new(vec![
        plugin(&journal, "p", MediaOrigin::Project, &["mkv"], Answer::Reads),
        plugin(&journal, "b", MediaOrigin::Bundled, &["mp4"], Answer::Reads),
        plugin(&journal, "u", MediaOrigin::User, &["wav"], Answer::Reads),
    ]));
    assert_eq!(
        registry.decoder_candidates(Path::new("/media/clip.xyz")),
        [
            (2, component("u/dec")),
            (1, component("b/dec")),
            (0, component("p/dec")),
        ]
    );
}

#[test]
fn unsupported_moves_to_the_next_decoder() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(&journal, Answer::Unsupported, Answer::Reads, Answer::Reads);
    let mut backend = MediaBackend::with_registry(registry);
    assert_eq!(backend.probe(&path).unwrap(), &probe_of("b"));
    assert_eq!(backend.decoder_of(&path), Some(&component("b/dec")));
    assert_eq!(
        journal.entries(),
        strings(&["start u", "probe u/dec", "start b", "probe b/dec"])
    );
}

#[test]
fn a_routed_path_stays_pinned() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(&journal, Answer::Unsupported, Answer::Reads, Answer::Reads);
    let mut backend =
        MediaBackend::with_registry(registry).with_sequential_video(Rational::new(30, 1));
    backend.probe(&path).unwrap();
    journal.clear();
    backend.decode_frame(&path, 0.5).unwrap();
    backend.decode_frame_for("layer", &path, 0.5).unwrap();
    backend.decode_audio(&path, 48_000, 2).unwrap();
    backend.probe(&path).unwrap();
    assert_eq!(
        backend.video_duration(&path).unwrap(),
        Some(1.5),
        "from the probe"
    );
    assert_eq!(
        journal.entries(),
        strings(&[
            "video b/dec None",
            "video b/dec Some(\"layer\")",
            "audio b/dec",
        ]),
        "only b is asked, and the probe is cached"
    );
}

#[test]
fn decode_without_probe_routes_first() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(&journal, Answer::Unsupported, Answer::Reads, Answer::Reads);
    let mut backend = MediaBackend::with_registry(registry);
    backend.decode_frame(&path, 0.0).unwrap();
    assert_eq!(
        journal.entries(),
        strings(&[
            "start u",
            "probe u/dec",
            "start b",
            "probe b/dec",
            "video b/dec None",
        ])
    );
    assert_eq!(backend.decoder_of(&path), Some(&component("b/dec")));

    let other = file(&directory, "voice.mp4");
    journal.clear();
    backend.decode_audio(&other, 48_000, 2).unwrap();
    assert_eq!(
        journal.entries(),
        strings(&["probe u/dec", "probe b/dec", "audio b/dec"]),
        "each plugin's process is started once"
    );
}

#[test]
fn missing_file_starts_no_process() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("gone.mp4");
    let journal = Journal::default();
    let registry = three(&journal, Answer::Reads, Answer::Reads, Answer::Reads);
    let mut backend = MediaBackend::with_registry(registry);
    assert!(matches!(
        backend.probe(&path),
        Err(MediaError::NotFound(missing)) if missing == path
    ));
    assert!(matches!(
        backend.decode_frame(&path, 0.0),
        Err(MediaError::NotFound(_))
    ));
    assert!(matches!(
        backend.decode_audio(&path, 48_000, 2),
        Err(MediaError::NotFound(_))
    ));
    assert_eq!(
        journal.entries(),
        Vec::<String>::new(),
        "no process started"
    );
}

#[test]
fn a_stopped_decoder_ends_routing() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(&journal, Answer::Stopped, Answer::Reads, Answer::Reads);
    let mut backend = MediaBackend::with_registry(registry);
    assert!(matches!(
        backend.probe(&path),
        Err(MediaError::CodecStopped(_))
    ));
    assert_eq!(journal.count("start u"), 1, "not sent again");
    assert_eq!(journal.count("start b"), 0, "routing ended");
    assert_eq!(backend.decoder_of(&path), None);
    // The stopped process is dropped; the next request starts another.
    assert!(backend.probe(&path).is_err());
    assert_eq!(journal.count("start u"), 2, "a new process next time");
}

#[test]
fn an_unavailable_decoder_falls_through() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(&journal, Answer::Unavailable, Answer::Reads, Answer::Reads);
    let mut backend = MediaBackend::with_registry(registry);
    assert_eq!(backend.probe(&path).unwrap(), &probe_of("b"));
    assert_eq!(
        journal.entries(),
        strings(&[
            "start u",
            "probe u/dec",
            "start u",
            "probe u/dec",
            "start b",
            "probe b/dec",
        ])
    );
}

#[test]
fn a_decoder_that_stops_answering_is_opened_again_once() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(
        &journal,
        Answer::UnavailableOnce,
        Answer::Reads,
        Answer::Reads,
    );
    let mut backend = MediaBackend::with_registry(registry);
    assert_eq!(backend.probe(&path).unwrap(), &probe_of("u"));
    assert_eq!(journal.count("start u"), 2);
    assert_eq!(journal.count("start b"), 0);
}

#[test]
fn every_decoder_failing_names_them() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(
        &journal,
        Answer::Unsupported,
        Answer::Unsupported,
        Answer::Unsupported,
    );
    let mut backend = MediaBackend::with_registry(registry);
    let Err(MediaError::Unsupported { path: named, tried }) = backend.probe(&path) else {
        panic!("not unsupported");
    };
    assert_eq!(named, path);
    assert_eq!(tried, ["u/dec", "b/dec", "p/dec"]);
    assert_eq!(backend.decoder_of(&path), None);
}

#[test]
fn a_real_error_beats_unsupported() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(
        &journal,
        Answer::Broken,
        Answer::Unsupported,
        Answer::Unsupported,
    );
    let mut backend = MediaBackend::with_registry(registry);
    assert!(matches!(
        backend.probe(&path),
        Err(MediaError::Codec(message)) if message == "broken"
    ));
    assert_eq!(journal.count("probe p/dec"), 1, "every decoder was tried");
}

#[test]
fn a_file_without_video_beats_another_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(
        &journal,
        Answer::Broken,
        Answer::NoVideoStream,
        Answer::Unsupported,
    );
    let mut backend = MediaBackend::with_registry(registry);
    assert!(matches!(
        backend.probe(&path),
        Err(MediaError::NoVideoStream(named)) if named == path
    ));
}

#[test]
fn a_missing_file_beats_every_other_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(
        &journal,
        Answer::Broken,
        Answer::NoVideoStream,
        Answer::NotFound,
    );
    let mut backend = MediaBackend::with_registry(registry);
    assert!(matches!(
        backend.probe(&path),
        Err(MediaError::NotFound(named)) if named == path
    ));
}

#[test]
fn a_pinned_decoder_s_errors_do_not_fall_back() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(
        &journal,
        Answer::Unsupported,
        Answer::ReadsButNotItsFrames,
        Answer::Reads,
    );
    let mut backend = MediaBackend::with_registry(registry);
    backend.probe(&path).unwrap();
    journal.clear();
    assert!(matches!(
        backend.decode_frame(&path, 0.0),
        Err(MediaError::Codec(message)) if message == "bad frame"
    ));
    assert_eq!(journal.entries(), strings(&["video b/dec None"]));
    assert_eq!(backend.decoder_of(&path), Some(&component("b/dec")));
}

#[test]
fn a_pinned_decoder_answering_unsupported_is_named() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(
        &journal,
        Answer::Unsupported,
        Answer::ReadsButNotItsFramesUnsupported,
        Answer::Reads,
    );
    let mut backend = MediaBackend::with_registry(registry);
    backend.probe(&path).unwrap();
    journal.clear();
    let error = backend.decode_frame(&path, 0.0).unwrap_err();
    assert_eq!(
        error.to_string(),
        format!("no decoder could read {} (tried b/dec)", path.display())
    );
    // Still pinned: the project decoder that reads it is not asked.
    assert_eq!(journal.entries(), strings(&["video b/dec None"]));
}

#[test]
fn a_decoder_whose_process_cannot_start_falls_through() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(&journal, Answer::CannotStart, Answer::Reads, Answer::Reads);
    let mut backend = MediaBackend::with_registry(registry);
    assert_eq!(backend.probe(&path).unwrap(), &probe_of("b"));
    assert_eq!(
        journal.entries(),
        strings(&["start u", "start b", "probe b/dec"])
    );
}

#[test]
fn hand_written_extensions_are_normalized() {
    let journal = Journal::default();
    let registry = MediaRegistry::new(vec![
        plugin(
            &journal,
            "upper",
            MediaOrigin::User,
            &["MP4"],
            Answer::Reads,
        ),
        plugin(
            &journal,
            "dotted",
            MediaOrigin::User,
            &[".mp4"],
            Answer::Reads,
        ),
        plugin(
            &journal,
            "other",
            MediaOrigin::User,
            &["wav"],
            Answer::Reads,
        ),
    ]);
    assert_eq!(
        registry.decoder_candidates(Path::new("/clip.mp4")),
        [(0, component("upper/dec")), (1, component("dotted/dec"))]
    );
    assert_eq!(registry.plugin(1).decoders[0].extensions, ["mp4"]);
}

#[test]
fn no_decoders_is_no_decoder() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let empty = MediaRegistry::new(Vec::new());
    assert!(!empty.has_decoders());
    let mut backend = MediaBackend::with_registry(Arc::new(empty));
    assert!(matches!(backend.probe(&path), Err(MediaError::NoDecoder)));

    // A plugin without decoders, there to encode, decodes nothing.
    let journal = Journal::default();
    let encoder_only = MediaRegistry::new(vec![with_decoders(
        &journal,
        "e",
        MediaOrigin::Bundled,
        Vec::new(),
        Answer::Reads,
    )]);
    assert!(!encoder_only.has_decoders());
    let mut backend = MediaBackend::with_registry(Arc::new(encoder_only));
    assert!(matches!(
        backend.decode_frame(&path, 0.0),
        Err(MediaError::NoDecoder)
    ));
    assert_eq!(journal.entries(), Vec::<String>::new());
}

#[test]
fn project_uses_are_recorded() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let registry = three(
        &journal,
        Answer::Unsupported,
        Answer::Unsupported,
        Answer::Reads,
    );
    assert_eq!(registry.project_decoder_uses(), (0, Vec::new()));
    MediaBackend::with_registry(registry.clone())
        .probe(&path)
        .unwrap();
    let expected = vec![DecoderUse {
        decoder: component("p/dec"),
        origin: MediaOrigin::Project,
        path: path.clone(),
    }];
    assert_eq!(registry.project_decoder_uses(), (1, expected.clone()));
    // Another backend routing the same path to the same decoder adds none.
    MediaBackend::with_registry(registry.clone())
        .probe(&path)
        .unwrap();
    assert_eq!(registry.project_decoder_uses(), (1, expected));

    for answers in [
        (Answer::Reads, Answer::Reads),
        (Answer::Unsupported, Answer::Reads),
    ] {
        let registry = three(&journal, answers.0, answers.1, Answer::Reads);
        MediaBackend::with_registry(registry.clone())
            .probe(&path)
            .unwrap();
        assert_eq!(
            registry.project_decoder_uses(),
            (0, Vec::new()),
            "{answers:?}"
        );
    }
}

fn settings() -> EncoderSettings {
    EncoderSettings {
        output: "/out/video.mp4".into(),
        width: 2,
        height: 1,
        frame_rate: Rational::new(30, 1),
        input: PixelFormat::Yuv420p,
        container: Container::Mp4,
        preset: "medium".into(),
        crf: 18,
        bit_depth: 8,
    }
}

#[test]
fn encoding_without_a_bundled_plugin_uses_the_first_that_is_not_a_projects() {
    let journal = Journal::default();
    let registry = Arc::new(MediaRegistry::new(vec![
        plugin(&journal, "p", MediaOrigin::Project, &["mp4"], Answer::Reads),
        with_decoders(&journal, "u", MediaOrigin::User, Vec::new(), Answer::Reads),
        plugin(&journal, "u2", MediaOrigin::User, &["mp4"], Answer::Reads),
    ]));
    assert_eq!(registry.legacy_encoder(), Some(1));
    let mut backend = MediaBackend::with_registry(registry);
    backend.check_encoder(Container::Mp4).unwrap();
    let encoder = backend.open_encoder(&settings()).unwrap();
    backend.encode_frame(encoder, vec![0; 3]).unwrap();
    backend.finish_encoder(encoder).unwrap();
    assert_eq!(journal.entries(), strings(&["start u", "open u"]));

    // A project's plugin never encodes, even when it is the only one.
    let project_only = Arc::new(MediaRegistry::new(vec![
        plugin(&journal, "p", MediaOrigin::Project, &["mp4"], Answer::Reads),
        plugin(
            &journal,
            "p2",
            MediaOrigin::Project,
            &["mp4"],
            Answer::Reads,
        ),
    ]));
    assert_eq!(project_only.legacy_encoder(), None);
    let mut backend = MediaBackend::with_registry(project_only);
    assert!(matches!(
        backend.check_encoder(Container::Mp4),
        Err(MediaError::NoCodec)
    ));
    assert!(matches!(
        backend.open_encoder(&settings()),
        Err(MediaError::NoCodec)
    ));
    assert_eq!(journal.entries(), strings(&["start u", "open u"]));

    let empty = MediaRegistry::new(Vec::new());
    assert_eq!(empty.legacy_encoder(), None);
    let mut backend = MediaBackend::with_registry(Arc::new(empty));
    assert!(matches!(
        backend.open_encoder(&settings()),
        Err(MediaError::NoCodec)
    ));
}

#[test]
fn encoding_prefers_the_first_bundled_plugin() {
    let journal = Journal::default();
    let registry = Arc::new(MediaRegistry::new(vec![
        plugin(&journal, "u", MediaOrigin::User, &["mp4"], Answer::Reads),
        plugin(&journal, "p", MediaOrigin::Project, &["mp4"], Answer::Reads),
        plugin(&journal, "b", MediaOrigin::Bundled, &["mp4"], Answer::Reads),
        plugin(
            &journal,
            "b2",
            MediaOrigin::Bundled,
            &["mp4"],
            Answer::Reads,
        ),
    ]));
    assert_eq!(registry.legacy_encoder(), Some(2));
    let mut backend = MediaBackend::with_registry(registry);
    backend.open_encoder(&settings()).unwrap();
    assert_eq!(journal.entries(), strings(&["start b", "open b"]));

    // Without a bundled plugin, the first in search order encodes, a
    // project's aside.
    let user_only = MediaRegistry::new(vec![plugin(
        &journal,
        "u",
        MediaOrigin::User,
        &["mp4"],
        Answer::Reads,
    )]);
    assert_eq!(user_only.legacy_encoder(), Some(0));
}

#[test]
fn a_backend_made_before_set_registry_uses_it() {
    // The only test that sets the process's registry: every other one gives
    // its backend a registry of its own.
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let mut backend = MediaBackend::new();
    let journal = Journal::default();
    let chosen = three(&journal, Answer::Reads, Answer::Reads, Answer::Reads);
    set_registry(Some(chosen.clone()));
    assert!(Arc::ptr_eq(&registry().unwrap(), &chosen));
    let probed = backend.probe(&path).cloned();
    set_registry(None);
    assert_eq!(probed.unwrap(), probe_of("u"));
    assert!(registry().is_none());
    assert!(matches!(
        MediaBackend::new().probe(&path),
        Err(MediaError::NoDecoder)
    ));
    // The backend keeps the registry it first used.
    backend.decode_frame(&path, 0.0).unwrap();
}

/// A registry of one plugin, `fake`, reading `mp4` with `factory`.
fn single(factory: ProviderFactory) -> Arc<MediaRegistry> {
    Arc::new(MediaRegistry::new(vec![RegisteredPlugin {
        id: "fake".into(),
        origin: MediaOrigin::Bundled,
        decoders: vec![MediaDecoderEntry {
            id: "dec".into(),
            name: "Decoder".into(),
            extensions: vec!["mp4".into()],
        }],
        factory,
    }]))
}

fn reads(journal: &Journal) -> ProviderFactory {
    plugin(
        journal,
        "fake",
        MediaOrigin::Bundled,
        &["mp4"],
        Answer::Reads,
    )
    .factory
}

#[test]
fn a_decoder_in_this_process_opens_after_its_switch_is_killed() {
    // It has no process to stop, so it answers as before: an export's
    // Cancel stops it between frames, as it always did.
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let kill = PluginKillSwitch::new();
    let mut backend = MediaBackend::with_registry(single(reads(&Journal::default())))
        .with_kill_switch(kill.clone());
    kill.kill();
    assert!(backend.decode_audio(&path, 48_000, 1).is_ok());
}

#[test]
fn a_backend_gives_its_plugins_its_kill_switch() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let kill = PluginKillSwitch::new();
    let given = Arc::new(Mutex::new(None));
    let seen = given.clone();
    let inner = reads(&Journal::default());
    let factory: ProviderFactory = Arc::new(move |kill| {
        *seen.lock().unwrap() = Some(kill.clone());
        inner(kill)
    });
    let mut backend = MediaBackend::with_registry(single(factory)).with_kill_switch(kill.clone());
    let _ = backend.decode_audio(&path, 48_000, 1);
    let given = given
        .lock()
        .unwrap()
        .take()
        .expect("the plugin was started");
    assert!(!given.is_killed());
    kill.kill();
    assert!(given.is_killed(), "the plugin got the backend's switch");
}

#[test]
fn a_backend_asks_only_for_mono_or_stereo() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let mut backend = MediaBackend::with_registry(single(reads(&journal)));
    assert!(matches!(
        backend.decode_audio(&path, 48_000, 3),
        Err(MediaError::InvalidAudioFormat { channels: 3, .. })
    ));
    assert_eq!(journal.entries(), Vec::<String>::new(), "asked nothing");
    assert_eq!(backend.decode_audio(&path, 48_000, 1).unwrap().channels, 1);
}

#[test]
fn audio_in_another_format_than_asked_for_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let path = file(&directory, "clip.mp4");
    let journal = Journal::default();
    let factory = plugin(
        &journal,
        "fake",
        MediaOrigin::Bundled,
        &["mp4"],
        Answer::ReadsAudioAt(44_100),
    )
    .factory;
    let mut backend = MediaBackend::with_registry(single(factory));
    assert!(matches!(
        backend.decode_audio(&path, 48_000, 2),
        Err(MediaError::Codec(_))
    ));
}
