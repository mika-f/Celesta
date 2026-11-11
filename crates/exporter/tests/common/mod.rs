//! Helpers both test targets use: `integration`, which needs nothing but
//! the Rust build, and `runtime`, which needs the built React runtime.

use std::path::{Path, PathBuf};

use celesta_codec_test::{StoredVideo, TestMedia};
use celesta_project::Project;

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn pixels(path: &Path) -> Vec<u8> {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
        .read_info()
        .unwrap();
    assert_eq!((reader.info().width, reader.info().height), (3, 3));
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut pixels).unwrap();
    pixels
}

/// A 3x3, 4 fps, one-second (four-frame) project written into `dir`.
pub fn tiny_project(dir: &Path) -> PathBuf {
    let mut project = Project::load(root().join("examples/minimal.celesta.json")).unwrap();
    project.settings.width = 3;
    project.settings.height = 3;
    project.settings.duration = Some(celesta_composition::Time::new(1, 1));
    project.settings.frame_rate = celesta_composition::Rational::new(4, 1);
    let source = dir.join("project.celesta.json");
    std::fs::write(&source, serde_json::to_vec(&project).unwrap()).unwrap();
    source
}

pub fn decode(path: &Path) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
        .read_info()
        .unwrap();
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    let (width, height) = (reader.info().width, reader.info().height);
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut pixels).unwrap();
    (width, height, pixels)
}

pub fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The names in `dir` that start with `.celesta-`: staged files and folders.
pub fn staging_in(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".celesta-"))
        .collect()
}

/// Reports a test skipped for want of a GPU adapter, or fails it when
/// `CELESTA_REQUIRE_GPU=1` says the machine has one.
pub fn skip_without_gpu(error: &impl std::fmt::Display) {
    assert!(
        std::env::var("CELESTA_REQUIRE_GPU").as_deref() != Ok("1"),
        "GPU tests require an adapter: {error}"
    );
    eprintln!("skipping live export test: no GPU adapter is available: {error}");
}

/// What the test codec stored in `path`: the frames its encoder was given
/// and the audio muxed in, as they were.
pub fn read(path: &Path) -> TestMedia {
    celesta_codec_test::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

pub fn stored_video(media: &TestMedia) -> &StoredVideo {
    media.video.as_ref().expect("the export has video")
}

/// `seconds` of mono samples at `sample_rate`, each `signal` of its time.
pub fn generate(sample_rate: u32, seconds: f64, signal: impl Fn(f64) -> f64) -> Vec<f32> {
    let rate = f64::from(sample_rate);
    let frames = (seconds * rate).round() as usize;
    (0..frames)
        .map(|frame| signal(frame as f64 / rate) as f32)
        .collect()
}

/// A sine of `frequency` at amplitude 0.125, as FFmpeg's `sine` source
/// makes it.
pub fn sine(frequency: f64, sample_rate: u32, seconds: f64) -> Vec<f32> {
    generate(sample_rate, seconds, |t| {
        0.125 * (std::f64::consts::TAU * frequency * t).sin()
    })
}
pub fn json(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap()
}
