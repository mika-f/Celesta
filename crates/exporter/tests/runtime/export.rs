//! React entries exported to video and subtitles.

use std::path::{Path, PathBuf};

use celesta_codec_test::{write_audio, write_rgba_video};
use celesta_composition::{Rational, Time};
use celesta_exporter::{
    ExportCancellation, ExportError, ExportJob, ExportOptions, ExportProgress, ExportRange,
    ExportedFile, LoudnessTarget, ReactRuntimeOptions,
};
use celesta_gpu_renderer::GpuRenderError;

use crate::common::{read, root, sine, skip_without_gpu, stored_video};

#[test]
fn exports_a_silent_react_entry_to_webm() {
    let runtime_path = crate::runtime();
    let directory = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = directory.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect } from '@celesta.studio/react';
export default function Root() {
  return <Composition width={64} height={64} fps={2} durationInFrames={2}>
    <Rect width={32} height={32} fill="#ff0000" />
  </Composition>;
}
"##,
    )
    .unwrap();
    let output = directory.path().join("film.webm");
    let (mut written, mut warnings) = (Vec::new(), Vec::new());
    // Without sound, a loudness target has nothing to adjust.
    match crate::exporter(ExportOptions {
        loudness: Some(LoudnessTarget::STREAMING),
        ..ExportOptions::default()
    })
    .export_react_entry_with_progress(
        &entry,
        &ReactRuntimeOptions::new("node", runtime_path),
        &output,
        |event| match event {
            ExportProgress::Wrote(file) => written.push(file),
            ExportProgress::Warning(warning) => warnings.push(warning),
            _ => {}
        },
    ) {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            return;
        }
        Err(error) => panic!("React WebM export failed: {error}"),
    }

    let media = read(&output);
    assert_eq!(media.extension.as_deref(), Some("webm"));
    assert!(media.audio.is_none());
    assert!(
        matches!(
            written[..],
            [ExportedFile::Video {
                audio: None,
                loudness: None,
                ..
            }]
        ),
        "{written:?}"
    );
    assert!(
        warnings.len() == 1 && warnings[0].contains("no sound"),
        "{warnings:?}"
    );
}

/// A React entry of two 64x64 frames at 2 fps with no sound, written to
/// `film.tsx` in a new directory under `packages/cli`.
fn silent_react_entry() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let runtime_path = crate::runtime();
    let directory = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = directory.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect } from '@celesta.studio/react';
export default function Root() {
  return <Composition width={64} height={64} fps={2} durationInFrames={2}>
    <Rect width={32} height={32} fill="#ff0000" />
  </Composition>;
}
"##,
    )
    .unwrap();
    (directory, entry, runtime_path)
}

/// Without a mix there is no pre-roll to send, so a looping React entry
/// with no sound gets a video-only file.
#[test]
fn a_silent_react_entry_with_loop_has_no_audio_track() {
    let (directory, entry, runtime_path) = silent_react_entry();
    let output = directory.path().join("film.mp4");
    let mut written = Vec::new();
    match crate::exporter(ExportOptions {
        looping: true,
        ..ExportOptions::default()
    })
    .export_react_entry_with_progress(
        &entry,
        &ReactRuntimeOptions::new("node", runtime_path),
        &output,
        |event| {
            if let ExportProgress::Wrote(file) = event {
                written.push(file);
            }
        },
    ) {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            return;
        }
        Err(error) => panic!("React export failed: {error}"),
    }
    assert!(
        matches!(written[..], [ExportedFile::Video { audio: None, .. }]),
        "{written:?}"
    );
    assert!(read(&output).audio.is_none());
}

#[test]
fn writes_a_react_entrys_subtitles_shifted_to_the_exported_range() {
    let runtime_path = crate::runtime();
    let directory = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = directory.path().join("film.tsx");
    std::fs::write(
        directory.path().join("captions.srt"),
        "\u{feff}1\r\n00:00:00,000 --> 00:00:01,000\r\nBefore\r\n\r\n\
         2\r\n00:00:01,000 --> 00:00:02,000\r\nAcross <i>start</i>\r\n\r\n\
         3\r\n00:00:02,000 --> 00:00:02,500\r\nInside\r\n\r\n\
         4\r\n00:00:02,500 --> 00:00:04,000\r\nAcross end\r\n",
    )
    .unwrap();
    std::fs::write(
        &entry,
        r##"
import { Composition, Rect, Sequence, Subtitles, parseSubtitles } from '@celesta.studio/react';
import captions from './captions.srt';
const cues = parseSubtitles(captions, { fps: 10 });
export default function Root() {
  return <Composition width={64} height={64} fps={10} durationInFrames={40}>
    <Rect width={32} height={32} fill="#ff0000" />
    <Sequence from={5}><Subtitles cues={cues} /></Sequence>
  </Composition>;
}
"##,
    )
    .unwrap();
    let output = directory.path().join("film.mp4");
    let subtitles = directory.path().join("film.vtt");
    let mut written = Vec::new();
    match crate::exporter(ExportOptions {
        range: Some(ExportRange::new(Time::new(2, 1), Time::new(7, 2))),
        subtitles: vec![subtitles.clone()],
        ..ExportOptions::default()
    })
    .export_react_entry_with_progress(
        &entry,
        &ReactRuntimeOptions::new("node", runtime_path),
        &output,
        |event| {
            if let ExportProgress::Wrote(file) = event {
                written.push(file);
            }
        },
    ) {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            skip_without_gpu(&error);
            return;
        }
        Err(error) => panic!("React subtitle export failed: {error}"),
    }

    // The sequence shifts the cues by 0.5 s; the range starts the file at
    // 2 s and ends it at 3.5 s, so "Before" is never shown and the cues on
    // either edge are cut.
    assert_eq!(
        std::fs::read_to_string(&subtitles).unwrap(),
        "WEBVTT\n\n\
         00:00:00.000 --> 00:00:00.500\nAcross start\n\n\
         00:00:00.500 --> 00:00:01.000\nInside\n\n\
         00:00:01.000 --> 00:00:01.500\nAcross end\n"
    );
    assert!(
        matches!(
            &written[..],
            [
                ExportedFile::Video { frames: 15, .. },
                ExportedFile::Subtitles { cues: 3, .. }
            ]
        ),
        "{written:?}"
    );
}

#[test]
fn writes_only_a_react_entrys_subtitles_per_language_and_format() {
    let runtime_path = crate::runtime();
    let directory = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let entry = directory.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Sequence, Subtitles } from '@celesta.studio/react';
const ja = [{ from: 0, durationInFrames: 10, text: 'こんにちは' }, { from: 20, durationInFrames: 10, text: '世界' }];
const en = [{ from: 0, durationInFrames: 10, text: 'Hello' }, { from: 20, durationInFrames: 10, text: 'World' }];
export default function Root() {
  return <Composition width={64} height={64} fps={10} durationInFrames={30} lang="ja">
    <Subtitles cues={ja} />
    <Sequence from={5}><Subtitles lang="en" cues={en} /></Sequence>
  </Composition>;
}
"##,
    )
    .unwrap();
    let per_language =
        |extension: &str| directory.path().join(format!("film.{{lang}}.{extension}"));
    let mut written = Vec::new();
    let mut rendered = Vec::new();
    let job = ExportJob {
        source: entry.clone(),
        react: true,
        composition_id: None,
        companion: None,
        props_file: None,
        props: None,
        output: None,
        stills: None,
        options: ExportOptions {
            subtitles: vec![per_language("srt"), per_language("vtt")],
            ..ExportOptions::default()
        },
    };
    let runtime = ReactRuntimeOptions::new("node", &runtime_path);
    crate::exporter(job.options.clone())
        .export_react_subtitles(
            &entry,
            &runtime,
            None,
            &ExportCancellation::default(),
            |event| match event {
                ExportProgress::Wrote(file) => written.push(file),
                ExportProgress::Rendering { frame, total } => rendered.push((frame, total)),
                _ => {}
            },
        )
        .unwrap();

    // No video, one file per language and format; the English cues are
    // shifted by their sequence and cut to the composition.
    assert!(!directory.path().join("film.mp4").exists());
    assert_eq!(
        std::fs::read_to_string(directory.path().join("film.ja.srt")).unwrap(),
        "1\n00:00:00,000 --> 00:00:01,000\nこんにちは\n\n\
         2\n00:00:02,000 --> 00:00:03,000\n世界\n"
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("film.en.vtt")).unwrap(),
        "WEBVTT\n\n\
         00:00:00.500 --> 00:00:01.500\nHello\n\n\
         00:00:02.500 --> 00:00:03.000\nWorld\n"
    );
    let mut files: Vec<_> = written
        .iter()
        .map(|file| match file {
            ExportedFile::Subtitles {
                path, cues, lang, ..
            } => (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                *cues,
                lang.clone(),
            ),
            other => panic!("only subtitles should be written, not {other:?}"),
        })
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            ("film.en.srt".into(), 2, Some("en".into())),
            ("film.en.vtt".into(), 2, Some("en".into())),
            ("film.ja.srt".into(), 2, Some("ja".into())),
            ("film.ja.vtt".into(), 2, Some("ja".into())),
        ]
    );
    assert_eq!(rendered.last(), Some(&(30, 30)));

    // A path without {lang} cannot take both languages, and writes nothing.
    let mixed = directory.path().join("mixed.srt");
    let error = crate::exporter(ExportOptions {
        subtitles: vec![directory.path().join("other.{lang}.srt"), mixed.clone()],
        ..ExportOptions::default()
    })
    .export_react_subtitles(
        &entry,
        &runtime,
        None,
        &ExportCancellation::default(),
        |_| {},
    )
    .unwrap_err();
    assert!(
        matches!(&error, ExportError::MixedSubtitleLanguages { languages, .. } if languages == &["en", "ja"]),
        "{error}"
    );
    assert!(!mixed.exists());
    assert!(!directory.path().join("other.ja.srt").exists());

    // The job runs the same export when it has no output.
    std::fs::remove_file(directory.path().join("film.ja.srt")).unwrap();
    let mut job = job;
    job.options.subtitles = vec![per_language("srt")];
    job.options.overwrite = true;
    job.run(&ExportCancellation::default(), &mut |_| {})
        .unwrap();
    assert!(directory.path().join("film.ja.srt").exists());
}

/// Writes plugin `hang` into `directory`: a component, `<Hang>`, whose layer
/// is the plugin's content, and a program that answers `hello` and `open`,
/// then never answers `frame`. The program writes its process id to `pid`
/// once it has answered.
#[cfg(unix)]
fn write_hanging_plugin(directory: &Path) {
    use celesta_plugin::protocol::{Message, encode_opened, write_message};
    use celesta_plugin::{MeshModel, ModelId, Opened};
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(directory.join("bin")).unwrap();
    std::fs::write(
        directory.join(celesta_plugin_host::MANIFEST),
        serde_json::json!({
            "id": "hang",
            "version": "1.0.0",
            "protocol": 1,
            "celesta": concat!("=", env!("CARGO_PKG_VERSION")),
            "module": "index.ts",
            "executable": { celesta_plugin_host::platform().0: "bin/p" },
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        directory.join("index.ts"),
        r#"
import * as React from 'react';
import { registerHostElement } from '@celesta.studio/react/internal';

const ELEMENT = 'celesta-plugin-hang';
export function Hang(): React.ReactElement {
  return React.createElement(ELEMENT, {});
}
registerHostElement(ELEMENT, {
  content: () => ({ type: 'plugin', plugin: 'hang', assets: {}, time: 0 }),
});
"#,
    )
    .unwrap();
    let mut answers = Vec::new();
    let hello = serde_json::json!({
        "type": "hello", "protocol": 1, "plugin": "hang",
        "version": "1.0.0", "roles": ["content"],
    });
    write_message(&mut answers, &Message::new(hello)).unwrap();
    let opened = Opened {
        model: ModelId(1),
        content: MeshModel {
            canvas: [2.0, 2.0],
            textures: Vec::new(),
            meshes: Vec::new(),
        }
        .into(),
        notices: Vec::new(),
    };
    write_message(&mut answers, &encode_opened(&opened)).unwrap();
    std::fs::write(directory.join("answers"), answers).unwrap();
    let program = directory.join("bin/p");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\ncat answers\necho $$ > '{}'\nexec sleep 60\n",
            directory.join("pid").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Cancel kills the plugins an export runs: a frame waiting on a plugin that
/// never answers returns at once, long before the frame's deadline, and the
/// export ends as cancelled with the plugin's process gone.
#[cfg(unix)]
#[test]
fn cancel_ends_a_frame_waiting_on_a_plugin_that_hangs() {
    use std::time::{Duration, Instant};

    let runtime_path = crate::runtime();
    let directory = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let plugin = directory.path().join("plugins/hang");
    write_hanging_plugin(&plugin);
    let (plugins, errors) = celesta_plugin_host::discover(&[directory.path().join("plugins")]);
    assert!(errors.is_empty(), "{errors:?}");
    let entry = directory.path().join("film.tsx");
    std::fs::write(
        &entry,
        r#"
import { Composition } from '@celesta.studio/react';
import { Hang } from '@celesta-plugin/hang';
export default function Root() {
  return <Composition width={64} height={64} fps={2} durationInFrames={2}>
    <Hang />
  </Composition>;
}
"#,
    )
    .unwrap();

    let cancellation = ExportCancellation::default();
    let pid = plugin.join("pid");
    let canceller = {
        let cancellation = cancellation.clone();
        let pid = pid.clone();
        std::thread::spawn(move || {
            // Once the plugin answered `open`, the frame waits on it.
            let deadline = Instant::now() + Duration::from_secs(60);
            while !pid.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            std::thread::sleep(Duration::from_millis(300));
            cancellation.cancel();
        })
    };
    let output = directory.path().join("film.mp4");
    let begun = Instant::now();
    let result = crate::exporter(ExportOptions {
        plugins: Some(celesta_plugin_host::InstalledPlugins {
            plugins,
            ..Default::default()
        }),
        ..ExportOptions::default()
    })
    .export_react_entry_cancellable(
        &entry,
        &ReactRuntimeOptions::new("node", runtime_path),
        &output,
        &cancellation,
        |_| {},
    );
    if let Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) = &result {
        skip_without_gpu(error);
        cancellation.cancel();
        canceller.join().unwrap();
        return;
    }
    canceller.join().unwrap();
    assert!(matches!(result, Err(ExportError::Cancelled)), "{result:?}");
    assert!(
        begun.elapsed() < Duration::from_secs(25),
        "the frame returned before its 30 s deadline"
    );
    assert!(!output.exists());
    let pid = std::fs::read_to_string(&pid).unwrap();
    let running = std::process::Command::new("kill")
        .args(["-0", pid.trim()])
        .status()
        .unwrap()
        .success();
    assert!(!running, "the plugin's process was stopped");
}

/// A React entry playing a `<Video>` and an `<Audio>` exported by the
/// `celesta-exporter` program with the test codec's program as its media
/// plugin writes the same file whether its frames go through the program's
/// ring of shared slots or through the pipe, and the timing line says which
/// they went through.
#[test]
fn an_export_through_the_programs_ring_is_byte_identical_to_the_pipe() {
    use celesta_codec_test::{PLUGIN_PATH_ENV, TestCodecPlugin};
    use celesta_media::SHARED_MEMORY_VARIABLE;
    use std::process::Command;

    crate::runtime();
    let directory = tempfile::tempdir_in(root().join("packages/cli")).unwrap();
    let colors: [[u8; 4]; 4] = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
    ];
    let frames: Vec<Vec<u8>> = colors.iter().map(|color| color.repeat(64 * 64)).collect();
    write_rgba_video(
        &directory.path().join("clip.ctc"),
        64,
        64,
        Rational::new(2, 1),
        &frames,
    )
    .unwrap();
    write_audio(
        &directory.path().join("tone.ctc"),
        48_000,
        1,
        &sine(440.0, 48_000, 2.0),
    )
    .unwrap();
    let entry = directory.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Video, Audio } from '@celesta.studio/react';
export default function Root() {
  return <Composition width={64} height={64} fps={2} durationInFrames={4}>
    <Video src="./clip.ctc" width={64} height={64} />
    <Audio src="./tone.ctc" />
  </Composition>;
}
"##,
    )
    .unwrap();

    let plugin = TestCodecPlugin::new();
    let export = |shared: &str| {
        let output = directory.path().join(format!("film-{shared}.mp4"));
        let result = Command::new(env!("CARGO_BIN_EXE_celesta-exporter"))
            .env_remove(PLUGIN_PATH_ENV)
            .env(SHARED_MEMORY_VARIABLE, shared)
            .env("CELESTA_EXPORT_TIMING", "1")
            .arg("--plugin-dir")
            .arg(plugin.folder())
            .arg("--react")
            .arg(&entry)
            .arg(&output)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
        (result.status.success(), stderr, output)
    };
    let (ok, stderr, through_ring) = export("1");
    if !ok && stderr.contains("adapter") {
        skip_without_gpu(&stderr);
        return;
    }
    assert!(ok, "through the ring: {stderr}");
    if cfg!(any(unix, windows)) {
        assert!(stderr.contains("; frames ring 3 deep"), "{stderr}");
    }
    let (ok, stderr, through_pipe) = export("0");
    assert!(ok, "through the pipe: {stderr}");
    assert!(stderr.contains("; frames pipe 3 deep"), "{stderr}");

    let media = read(&through_ring);
    assert_eq!(stored_video(&media).frames.len(), 4);
    assert!(media.audio.is_some(), "the export has the tone");
    assert!(
        std::fs::read(&through_ring).unwrap() == std::fs::read(&through_pipe).unwrap(),
        "the files differ"
    );
}
