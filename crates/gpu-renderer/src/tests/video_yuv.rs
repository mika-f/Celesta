use super::*;
use crate::texture::{LayerTexture, VideoUploads};
use celesta_composition::LayerFilter;
use celesta_media::{
    DecodedColor, DecodedMatrix, DecodedPixelFormat as F, DecodedRange, Decoder, FrameTransport,
    MediaBackend, MediaProcess, MediaRegistry, ProviderUse, RegisteredPlugin, VideoRequest,
};
use celesta_plugin_host::{MediaManifest, MediaOrigin};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const YUV: &[F] = &[F::Nv12, F::Yuv420p, F::Rgba8];

fn read_texture(renderer: &GpuRenderer, texture: &LayerTexture) -> Vec<u8> {
    let (width, height) = (texture.width, texture.height);
    let row = wgpu::util::align_to(width * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("YUV gate readback"),
        size: u64::from(row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture._texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue.submit([encoder.finish()]);
    let (tx, rx) = mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for data in mapped.chunks_exact(row as usize) {
        pixels.extend_from_slice(&data[..(width * 4) as usize]);
    }
    drop(mapped);
    buffer.unmap();
    pixels
}

fn gate(label: &str, actual: &[u8], reference: &[u8], limit: u8) {
    assert_eq!(actual.len(), reference.len());
    let (mut max, mut sum, mut differing) = (0, 0u64, 0);
    for (a, b) in actual
        .as_chunks::<4>()
        .0
        .iter()
        .zip(reference.as_chunks::<4>().0)
    {
        assert_eq!(a[3], b[3], "{label}: alpha");
        differing += usize::from(a != b);
        for c in 0..3 {
            let d = a[c].abs_diff(b[c]);
            max = max.max(d);
            sum += u64::from(d);
        }
    }
    println!(
        "GATE {label} max={max} mean={:.6} differing={differing}",
        sum as f64 / (actual.len() / 4 * 3) as f64
    );
    assert!(max <= limit, "{label}: {max} > {limit}");
}

fn pattern(
    format: F,
    width: u32,
    height: u32,
    matrix: DecodedMatrix,
    range: DecodedRange,
) -> VideoFrame {
    let len = format.frame_bytes(width, height).unwrap();
    VideoFrame::new(
        width,
        height,
        format,
        Some(DecodedColor {
            matrix,
            range,
            chroma_location: Default::default(),
        }),
        (0..len)
            .map(|i| ((i * 73 + 19) % 256) as u8)
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

fn upload(renderer: &mut GpuRenderer, frame: &VideoFrame) -> LayerTexture {
    if frame.pixel_format == F::Rgba8 {
        renderer.video_uploads.upload(
            &renderer.device,
            &renderer.queue,
            &renderer.texture_bind_group_layout,
            &DecodedImage::shared(frame.width, frame.height, Arc::clone(&frame.pixels)).unwrap(),
            false,
        )
    } else {
        renderer
            .video_uploads
            .upload_yuv(
                &renderer.device,
                &renderer.queue,
                &renderer.texture_bind_group_layout,
                frame,
            )
            .unwrap()
    }
}

#[test]
fn planes_match_cpu_conversion_and_reuse_staging() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    for format in [F::Nv12, F::Yuv420p] {
        for (width, height) in [(2, 2), (8, 10), (18, 14), (3, 3)] {
            for matrix in [DecodedMatrix::Bt601, DecodedMatrix::Bt709] {
                for range in [DecodedRange::Limited, DecodedRange::Full] {
                    renderer.video_uploads = VideoUploads::default();
                    let frame = pattern(format, width, height, matrix, range);
                    let reference = frame.to_rgba().unwrap();
                    let mut allocations = 0;
                    for repeat in 0..3 {
                        renderer.video_uploads.begin_render();
                        let texture = upload(&mut renderer, &frame);
                        gate(
                            "synthetic",
                            &read_texture(&renderer, &texture),
                            &reference.pixels,
                            1,
                        );
                        if repeat == 0 {
                            allocations = renderer.video_uploads.allocations;
                        } else {
                            assert_eq!(renderer.video_uploads.allocations, allocations);
                            assert_eq!(renderer.video_uploads.profile.groups, 0);
                        }
                        assert_eq!(renderer.video_uploads.profile.submits, 1);
                    }
                    let mut malformed = frame.clone();
                    malformed.pixels = vec![0; 1].into();
                    let renderer = &mut renderer.renderer;
                    assert!(
                        renderer
                            .video_uploads
                            .upload_yuv(
                                &renderer.device,
                                &renderer.queue,
                                &renderer.texture_bind_group_layout,
                                &malformed
                            )
                            .is_err()
                    );
                }
            }
        }
    }
}

#[test]
fn queued_conversions_keep_distinct_layer_pixels() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut frame = pattern(F::Nv12, 18, 14, DecodedMatrix::Bt709, DecodedRange::Full);
    frame.pixels = vec![128; frame.pixels.len()].into();
    let first = upload(&mut renderer, &frame);
    let mut bytes = frame.pixels.as_ref().clone();
    bytes[..18 * 14].fill(220);
    frame.pixels = bytes.into();
    frame.color.as_mut().unwrap().range = DecodedRange::Limited;
    frame.color.as_mut().unwrap().matrix = DecodedMatrix::Bt601;
    let second = upload(&mut renderer, &frame);
    gate(
        "queued-first",
        &read_texture(&renderer, &first),
        &[128, 128, 128, 255].repeat(18 * 14),
        1,
    );
    gate(
        "queued-second",
        &read_texture(&renderer, &second),
        &frame.to_rgba().unwrap().pixels,
        1,
    );
}

#[derive(Clone)]
struct FrameDecoder(VideoFrame);

#[test]
fn unavailable_staging_mapping_still_uploads_and_converts() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    for format in [F::Rgba8, F::Nv12, F::Yuv420p] {
        renderer.video_uploads = VideoUploads::default();
        let frame = pattern(
            if format == F::Rgba8 { F::Nv12 } else { format },
            18,
            14,
            DecodedMatrix::Bt709,
            DecodedRange::Limited,
        );
        let frame = if format == F::Rgba8 {
            frame.to_rgba().unwrap()
        } else {
            frame
        };
        let texture = upload(&mut renderer, &frame);
        read_texture(&renderer, &texture);
        drop(texture);
        renderer.video_uploads.invalidate_staging_mapping();
        let texture = upload(&mut renderer, &frame);
        gate(
            "unmapped staging fallback",
            &read_texture(&renderer, &texture),
            &frame.to_rgba().unwrap().pixels,
            1,
        );
        assert_eq!(renderer.video_uploads.profile.submits, 1);
    }
}

impl VideoFrameDecoder for FrameDecoder {
    fn decode_frame(&mut self, _: &Path, _: f64) -> Result<VideoFrame, MediaError> {
        Ok(self.0.clone())
    }
}

#[test]
fn mipmaps_and_oversized_sources_keep_the_cpu_path() {
    let frame = pattern(F::Nv12, 18, 14, DecodedMatrix::Bt709, DecodedRange::Limited);
    for oversized in [false, true] {
        let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
            return;
        };
        renderer.render_quality = RenderQuality::Final;
        if oversized {
            renderer.max_texture_dimension = 8;
        }
        let mut scene = empty_scene(18, 14);
        scene
            .layers
            .push(video_at("clip", 0., 0., if oversized { 1. } else { 0.25 }));
        renderer.video_decoder = Some(Box::new(FrameDecoder(frame.to_rgba().unwrap())));
        let reference = renderer.render(&scene).unwrap();
        renderer.video_decoder = Some(Box::new(FrameDecoder(frame.clone())));
        let actual = renderer.render(&scene).unwrap();
        gate("CPU fallback", actual.pixels(), reference.pixels(), 0);
        assert_eq!(renderer.video_uploads.yuv_allocations(), 0);
    }
}

fn program() -> Option<PathBuf> {
    std::env::var_os("CELESTA_TEST_FFMPEG").map(PathBuf::from)
}

fn backend(program: &Path, formats: &[F]) -> MediaBackend {
    let output = std::process::Command::new(program)
        .arg("--manifest")
        .output()
        .unwrap();
    assert!(output.status.success());
    let manifest: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let (media, errors) = MediaManifest::from_json(manifest["media"].clone());
    assert!(errors.is_empty());
    let executable = program.to_owned();
    let plugin = RegisteredPlugin {
        id: manifest["id"].as_str().unwrap().into(),
        name: "GPU decode gate".into(),
        version: manifest["version"].as_str().unwrap().into(),
        origin: MediaOrigin::Bundled,
        directory: program.parent().unwrap().into(),
        executable: Some(program.into()),
        media,
        factory: Arc::new(move |kill, purpose| {
            let mut process = MediaProcess::start_with(
                &executable,
                executable.parent().unwrap(),
                kill,
                FrameTransport::SharedMemory,
                purpose,
            )?;
            if purpose == ProviderUse::Decode {
                assert!(process.shares_frames());
            }
            process.set_decode_prefetch(true);
            Ok(Box::new(process))
        }),
    };
    MediaBackend::with_registry(Arc::new(MediaRegistry::new(vec![plugin])))
        .with_sequential_video(Rational::new(30, 1))
        .with_accepted_pixel_formats(formats)
}

#[test]
fn ffmpeg_fixtures_meet_texture_and_scene_gates() {
    let Some(program) = program() else {
        eprintln!("set CELESTA_TEST_FFMPEG for the real-plugin pixel gate");
        return;
    };
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        panic!("the pixel gate requires a GPU");
    };
    for name in [
        "yuv-709-limited.mp4",
        "yuv-untagged.mp4",
        "yuv-709-full.mp4",
        "yuv-odd-chroma.mp4",
        "yuv-native-nv12.nut",
        "yuv-sequence.mp4",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugins/ffmpeg/tests/fixtures")
            .join(name)
            .canonicalize()
            .unwrap();
        let mut process =
            MediaProcess::start(&program, program.parent().unwrap(), &Default::default()).unwrap();
        let frame = process
            .decode_video(
                "ffmpeg",
                &VideoRequest {
                    path: &path,
                    time: 0.,
                    sequential: None,
                    accepted_pixel_formats: YUV,
                },
            )
            .unwrap();
        assert_ne!(frame.pixel_format, F::Rgba8);
        renderer.video_uploads.begin_render();
        let texture = upload(&mut renderer, &frame);
        gate(
            &format!("{name} texture"),
            &read_texture(&renderer, &texture),
            &frame.to_rgba().unwrap().pixels,
            1,
        );
        drop(texture);
        for effects in [false, true] {
            let mut scene = empty_scene(frame.width, frame.height);
            let mut layer = video_at("clip", 0., 0., 1.);
            if let LayerContent::Video { asset, .. } = &mut layer.content {
                asset.location = AssetLocation::File {
                    path: path.to_string_lossy().into_owned(),
                };
            }
            if effects {
                layer
                    .effects
                    .filters
                    .push(LayerFilter::Brightness { amount: 0.8 });
                layer.blend_mode = BlendMode::Screen;
                layer.opacity = 0.75;
            }
            scene.layers.push(layer);
            renderer.video_decoder = Some(Box::new(backend(&program, &[F::Rgba8])));
            let reference = renderer.render(&scene).unwrap();
            renderer.video_decoder = Some(Box::new(backend(&program, YUV)));
            let actual = renderer.render(&scene).unwrap();
            gate(
                &format!("{name} scene effects={effects}"),
                actual.pixels(),
                reference.pixels(),
                2,
            );
            assert!(renderer.video_uploads.yuv_allocations() > 0);
            if name == "yuv-709-limited.mp4"
                && let Some(directory) = std::env::var_os("CELESTA_YUV_SHOTS")
            {
                let directory = PathBuf::from(directory);
                for (policy, pixels) in [("rgba", reference.pixels()), ("yuv", actual.pixels())] {
                    image::save_buffer(
                        directory.join(format!("{policy}-effects-{effects}.png")),
                        pixels,
                        frame.width,
                        frame.height,
                        image::ColorType::Rgba8,
                    )
                    .unwrap();
                }
            }
        }
    }
}

#[test]
#[ignore = "requires CELESTA_TEST_FFMPEG and CELESTA_YUV_BENCH_INPUT"]
fn benchmark_completed_video_texture() {
    let program = program().expect("CELESTA_TEST_FFMPEG");
    let input = PathBuf::from(
        std::env::var_os("CELESTA_YUV_BENCH_INPUT").expect("CELESTA_YUV_BENCH_INPUT"),
    );
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        panic!("benchmark requires a GPU");
    };
    println!("BENCH adapter={:?}", renderer.adapter_info());
    for repeat in 0..3 {
        for work_ms in [0, 15] {
            for formats in [&[F::Rgba8][..], YUV] {
                let mut backend = backend(&program, formats);
                let rate = backend
                    .probe(&input)
                    .unwrap()
                    .video
                    .as_ref()
                    .unwrap()
                    .frame_rate
                    .unwrap();
                let fps = rate.numerator as f64 / rate.denominator as f64;
                renderer.video_uploads = VideoUploads::default();
                renderer.video_uploads.skip_conversion =
                    std::env::var_os("CELESTA_YUV_BENCH_NO_DISPATCH").is_some();
                let mut bytes = 0;
                let mut totals = [Duration::ZERO; 7];
                let mut counts = [0u64; 4];
                let mut start = Instant::now();
                for index in 0..124 {
                    if index == 4 {
                        start = Instant::now();
                    }
                    let frame_start = Instant::now();
                    let frame = backend
                        .decode_frame_for("clip", &input, index as f64 / fps)
                        .unwrap();
                    let decoded = frame_start.elapsed();
                    assert_eq!(
                        frame.pixel_format,
                        if formats.len() == 1 {
                            F::Rgba8
                        } else {
                            F::Yuv420p
                        }
                    );
                    bytes = frame.pixels.len();
                    renderer.video_uploads.begin_render();
                    let allocations = renderer.video_uploads.allocations;
                    let before_upload = Instant::now();
                    let texture = upload(&mut renderer, &frame);
                    let uploaded = before_upload.elapsed();
                    let before_wait = Instant::now();
                    renderer
                        .device
                        .poll(wgpu::PollType::wait_indefinitely())
                        .unwrap();
                    let waited = before_wait.elapsed();
                    std::hint::black_box(&texture);
                    drop(texture);
                    let work = Instant::now();
                    while work.elapsed() < Duration::from_millis(work_ms) {
                        std::hint::spin_loop();
                    }
                    let worked = work.elapsed();
                    if index >= 4 {
                        let profile = &renderer.video_uploads.profile;
                        let values = [
                            decoded,
                            profile.stage,
                            profile.convert,
                            uploaded - profile.stage - profile.convert,
                            waited,
                            frame_start.elapsed() - decoded - uploaded - waited - worked,
                            worked,
                        ];
                        for (total, value) in totals.iter_mut().zip(values) {
                            *total += value;
                        }
                        for (total, value) in counts.iter_mut().zip([
                            renderer.video_uploads.allocations - allocations,
                            profile.pipelines,
                            profile.groups,
                            profile.submits,
                        ]) {
                            *total += value;
                        }
                    }
                }
                println!(
                    "BENCH repeat={repeat} work_ms={work_ms} formats={formats:?} bytes={bytes} ms/frame={:.3}",
                    start.elapsed().as_secs_f64() * 1000. / 120.
                );
                println!(
                    "BREAKDOWN repeat={repeat} work_ms={work_ms} formats={formats:?} decode={:.6} stage={:.6} convert_submit={:.6} upload_other={:.6} wait={:.6} other={:.6} work={:.6} misses={} pipelines={} groups={} submits={}",
                    totals[0].as_secs_f64() * 1000. / 120.,
                    totals[1].as_secs_f64() * 1000. / 120.,
                    totals[2].as_secs_f64() * 1000. / 120.,
                    totals[3].as_secs_f64() * 1000. / 120.,
                    totals[4].as_secs_f64() * 1000. / 120.,
                    totals[5].as_secs_f64() * 1000. / 120.,
                    totals[6].as_secs_f64() * 1000. / 120.,
                    counts[0],
                    counts[1],
                    counts[2],
                    counts[3]
                );
            }
        }
    }
}
