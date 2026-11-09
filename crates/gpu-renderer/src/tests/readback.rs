//! Reading frames back: a composited frame is quantized or converted
//! straight from the root canvas it finishes on, and must read back as the
//! same pixels drawn without compositing.

use super::*;

/// Columns of `colors`, two pixels wide each, on a 16x4 scene; with
/// `screen`, each column screens onto the black background, which leaves
/// its color as it is but composites the frame through the root canvas.
fn columns(colors: &[&str], screen: bool) -> Scene {
    let mut scene = empty_scene(16, 4);
    for (index, color) in colors.iter().enumerate() {
        let mut layer = solid_rect(
            &format!("column-{index}"),
            index as f64 * 2.0 + 1.0,
            2.0,
            4.0,
            color,
        );
        if screen {
            layer.blend_mode = BlendMode::Screen;
        }
        scene.layers.push(layer);
    }
    scene
}

#[test]
fn reads_composited_frames_back_like_directly_drawn_ones() {
    let Some(mut renderer) = renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let first = ["#ff0000", "#33aa66", "#0000ff", "#ffffff", "#808080"];
    let second = ["#123456", "#fedcba", "#00ff00", "#000000", "#c0ffee"];

    let plain = renderer.render(&columns(&first, false)).unwrap();
    let screened = renderer.render(&columns(&first, true)).unwrap();
    assert_eq!(screened.pixels(), plain.pixels(), "render");

    for format in [
        ReadbackFormat::Rgba8,
        ReadbackFormat::Rgba16,
        ReadbackFormat::Yuv420p,
        ReadbackFormat::Yuv420p10,
        ReadbackFormat::Nv12,
        ReadbackFormat::P010,
    ] {
        if format.is_yuv() && !renderer.supports_yuv420p_readback() {
            eprintln!("skipping {format:?}: the GPU cannot convert to yuv420p");
            continue;
        }
        renderer.set_readback_format(format).unwrap();
        // In flight together, so each composited frame must have been read
        // from the root canvas before the next one drew onto it.
        let mut frames = Vec::new();
        for scene in [
            columns(&first, false),
            columns(&first, true),
            columns(&second, true),
            columns(&second, false),
        ] {
            frames.extend(renderer.submit(&scene).unwrap());
        }
        frames.extend(renderer.drain().unwrap());
        assert_eq!(frames.len(), 4);
        assert!(frames.iter().all(|frame| frame.format() == format));
        assert_eq!(frames[1].pixels(), frames[0].pixels(), "{format:?} first");
        assert_eq!(frames[2].pixels(), frames[3].pixels(), "{format:?} second");
        assert_ne!(frames[0].pixels(), frames[3].pixels(), "{format:?}");
    }
}

const REFERENCE_COLORS: [&str; 8] = [
    "#ff0000", "#00ff00", "#0000ff", "#ffffff", "#000000", "#808080", "#102030", "#c07090",
];

/// Reads `scene` back in `format`, or `None` when the GPU cannot convert.
fn read_back(
    renderer: &mut TestRenderer,
    format: ReadbackFormat,
    scene: &Scene,
) -> Option<Vec<u8>> {
    if !renderer.supports_yuv420p_readback() {
        eprintln!("skipping {format:?}: the GPU cannot convert to yuv420p");
        return None;
    }
    renderer.set_readback_format(format).unwrap();
    let mut frames = renderer
        .submit(scene)
        .unwrap()
        .into_iter()
        .collect::<Vec<_>>();
    frames.extend(renderer.drain().unwrap());
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].format(), format);
    Some(frames[0].pixels().to_vec())
}

fn black_renderer() -> Option<TestRenderer> {
    renderer(GpuRenderOptions {
        background: Color::rgba(0, 0, 0, 255),
        ..GpuRenderOptions::default()
    })
}

/// Y, U, and V as the BT.709 limited-range formula gives them, unrounded,
/// from `rgba` (`width`x`height`); chroma is the mean of each 2x2 block.
/// `bits == 10` scales the 8-bit values by four.
fn reference_yuv(
    rgba: &[u8],
    width: usize,
    height: usize,
    bits: u32,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let scale = if bits == 10 { 4.0 } else { 1.0 };
    let rgb = |x: usize, y: usize| {
        let at = (y * width + x) * 4;
        [0, 1, 2].map(|c| f64::from(rgba[at + c]) / 255.0)
    };
    let mut luma = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let [r, g, b] = rgb(x, y);
            let value = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            luma.push((16.0 + 219.0 * value) * scale);
        }
    }
    let (mut u, mut v) = (Vec::new(), Vec::new());
    for by in 0..height / 2 {
        for bx in 0..width / 2 {
            let mut mean = [0.0; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let pixel = rgb(bx * 2 + dx, by * 2 + dy);
                for c in 0..3 {
                    mean[c] += pixel[c] / 4.0;
                }
            }
            let [r, g, b] = mean;
            let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            u.push((128.0 + 224.0 * (b - y) / 1.8556) * scale);
            v.push((128.0 + 224.0 * (r - y) / 1.5748) * scale);
        }
    }
    (luma, u, v)
}

#[test]
fn nv12_interleaves_the_yuv420p_planes() {
    let Some(mut renderer) = black_renderer() else {
        return;
    };
    let scene = columns(&REFERENCE_COLORS, false);
    let Some(planar) = read_back(&mut renderer, ReadbackFormat::Yuv420p, &scene) else {
        return;
    };
    let nv12 = read_back(&mut renderer, ReadbackFormat::Nv12, &scene).unwrap();
    assert_eq!(nv12.len(), 16 * 4 * 3 / 2);
    assert_eq!(nv12[..64], planar[..64]);
    let pairs = nv12[64..]
        .chunks(2)
        .map(|pair| (pair[0], pair[1]))
        .collect::<Vec<_>>();
    // The 8x2 U plane, then the 8x2 V plane.
    let expected = planar[64..80]
        .iter()
        .zip(&planar[80..])
        .map(|(&u, &v)| (u, v))
        .collect::<Vec<_>>();
    assert_eq!(pairs, expected);
}

/// The samples of a `P010` or `Yuv420p10` frame, with the UV plane of a
/// semi-planar one de-interleaved into U then V.
fn planar_samples(bytes: &[u8], semi_planar: bool) -> Vec<u16> {
    let samples = samples16(bytes);
    if !semi_planar {
        return samples;
    }
    let (luma, chroma) = samples.split_at(64);
    let u = chroma.iter().step_by(2);
    let v = chroma.iter().skip(1).step_by(2);
    luma.iter().chain(u).chain(v).copied().collect()
}

#[test]
fn p010_is_yuv420p10_in_the_high_bits() {
    let Some(mut renderer) = black_renderer() else {
        return;
    };
    let scene = columns(&REFERENCE_COLORS, false);
    let Some(planar) = read_back(&mut renderer, ReadbackFormat::Yuv420p10, &scene) else {
        return;
    };
    let p010 = read_back(&mut renderer, ReadbackFormat::P010, &scene).unwrap();
    assert_eq!(p010.len(), 16 * 4 * 3);
    let raw = samples16(&p010);
    assert!(raw.iter().all(|sample| sample & 0x3f == 0));
    let planar = samples16(&planar);
    let interleaved = planar_samples(&p010, true);
    assert_eq!(interleaved.len(), planar.len());
    for (index, (high, low)) in interleaved.iter().zip(&planar).enumerate() {
        assert_eq!(high >> 6, *low, "sample {index}");
    }
}

#[test]
fn nv12_and_p010_match_a_cpu_reference() {
    let Some(mut renderer) = black_renderer() else {
        return;
    };
    let scene = columns(&REFERENCE_COLORS, false);
    let rgba = renderer.render(&scene).unwrap().pixels().to_vec();
    for (format, bits) in [(ReadbackFormat::Nv12, 8), (ReadbackFormat::P010, 10)] {
        let Some(bytes) = read_back(&mut renderer, format, &scene) else {
            return;
        };
        let actual = if bits == 10 {
            planar_samples(&bytes, true)
                .into_iter()
                .map(|sample| f64::from(sample >> 6))
                .collect::<Vec<_>>()
        } else {
            let (luma, chroma) = bytes.split_at(64);
            let u = chroma.iter().step_by(2);
            let v = chroma.iter().skip(1).step_by(2);
            luma.iter()
                .chain(u)
                .chain(v)
                .map(|&s| f64::from(s))
                .collect()
        };
        let (y, u, v) = reference_yuv(&rgba, 16, 4, bits);
        let expected = y.into_iter().chain(u).chain(v).collect::<Vec<_>>();
        assert_eq!(actual.len(), expected.len());
        for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
            let rounded = expected.round();
            assert!(
                (actual - rounded).abs() <= 1.0,
                "{format:?} sample {index}: {actual} against {expected}"
            );
            // The 8-bit dither keeps a code within an eighth of it; the 10-bit
            // one has no such margin, only the dither's own (under 1/128).
            let margin = if bits == 10 { 1.0 / 128.0 } else { 0.125 };
            if (expected - rounded).abs() <= margin {
                assert_eq!(*actual, rounded, "{format:?} sample {index}: {expected}");
            }
        }
    }
}

#[test]
fn nv12_and_p010_need_even_dimensions() {
    let Some(mut renderer) = black_renderer() else {
        return;
    };
    if !renderer.supports_yuv420p_readback() {
        eprintln!("skipping: the GPU cannot convert to yuv420p");
        return;
    }
    for format in [ReadbackFormat::Nv12, ReadbackFormat::P010] {
        renderer.set_readback_format(format).unwrap();
        assert!(matches!(
            renderer.submit(&empty_scene(6, 3)),
            Err(GpuRenderError::OddYuv420pSize {
                width: 6,
                height: 3
            })
        ));
    }
}
