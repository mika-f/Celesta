//! Decode the real export with libvpx, which preserves the alpha plane.
use celesta_composition::{Rational, Time, TimeRange};
use celesta_exporter::{ExportError, ExportOptions, Exporter};
use celesta_gpu_renderer::GpuRenderError;
use celesta_media::{MediaProcess, MediaRegistry};
use celesta_project::{
    Asset, AssetSource, Project, TimelineContent, TimelineItem, Track, TrackKind,
};
use ez_ffmpeg::{FfmpegContext, Input, Output};
use std::path::Path;
use std::sync::Arc;

fn transparent_project(dir: &Path) -> Project {
    let mut project = Project::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/minimal.celesta.json"),
    )
    .unwrap();
    project.settings.width = 64;
    project.settings.height = 64;
    project.settings.frame_rate = Rational::new(2, 1);
    project.settings.sample_rate = 8_000;
    project.settings.duration = Some(Time::new(1, 1));
    let file = std::fs::File::create(dir.join("red.png")).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), 64, 64);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[255, 0, 0, 128].repeat(64 * 64))
        .unwrap();
    writer.finish().unwrap();
    project.assets.insert(
        "red".to_owned(),
        Asset::Image {
            name: None,
            source: AssetSource::File {
                path: "red.png".to_owned(),
            },
        },
    );
    project.tracks.push(Track {
        id: "images".to_owned(),
        name: "Images".to_owned(),
        kind: TrackKind::Video,
        enabled: None,
        locked: None,
        muted: None,
        solo: None,
        items: vec![TimelineItem {
            id: "red".to_owned(),
            name: None,
            range: TimeRange {
                start: Time::ZERO,
                duration: Time::new(1, 1),
            },
            content: TimelineContent::Image {
                asset: "red".to_owned(),
                width: None,
                height: None,
                fit: None,
            },
            enabled: None,
            transform: None,
            opacity: None,
            blend_mode: None,
            effects: None,
        }],
    });
    project
}

#[test]
fn a_rendered_vp9_webm_preserves_the_transparent_quadrant() {
    let directory = tempfile::tempdir().unwrap();
    let project = transparent_project(directory.path());
    let path = directory.path().join("rendered.webm");
    let mut plugin = celesta_codec_ffmpeg::registered();
    let executable = std::path::PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg"));
    let program_directory = executable.parent().unwrap().to_path_buf();
    plugin.factory = MediaProcess::factory(executable, program_directory);
    celesta_media::set_registry(Some(Arc::new(MediaRegistry::new(vec![plugin]))));
    match Exporter::new(ExportOptions::default()).export_project(&project, directory.path(), &path)
    {
        Ok(()) => {}
        Err(ExportError::Render(GpuRenderError::RequestAdapter(error))) => {
            assert!(
                std::env::var("CELESTA_REQUIRE_GPU").as_deref() != Ok("1"),
                "GPU required: {error}"
            );
            eprintln!("skipping live export test: no GPU adapter is available: {error}");
            return;
        }
        Err(error) => panic!("export failed: {error}"),
    }
    let raw = directory.path().join("frame.rgba64");
    FfmpegContext::builder()
        .input(Input::from(path.to_string_lossy().into_owned()).set_video_codec("libvpx-vp9"))
        .output(
            Output::from(raw.to_string_lossy().into_owned())
                .set_format("rawvideo")
                .set_pix_fmt("rgba64le")
                .set_max_video_frames(1),
        )
        .build()
        .unwrap()
        .start()
        .unwrap()
        .wait()
        .unwrap();
    let bytes = std::fs::read(raw).unwrap();
    assert_eq!(bytes.len(), 64 * 64 * 8);
    let pixel = |x: usize, y: usize| -> [u16; 4] {
        let offset = (y * 64 + x) * 8;
        std::array::from_fn(|channel| {
            u16::from_le_bytes([bytes[offset + 2 * channel], bytes[offset + 2 * channel + 1]])
        })
    };
    let inside = pixel(8, 8);
    let outside = pixel(48, 48);
    assert!(inside[3].abs_diff(128 * 257) <= 2 * 257, "{inside:?}");
    assert!(
        inside[0].abs_diff(65535) <= 8 * 257 && inside[1] <= 8 * 257,
        "{inside:?}"
    );
    assert_eq!(outside[3], 0, "{outside:?}");
}
