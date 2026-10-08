use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use celesta_composition::{
    Animatable, BlendMode, EvaluatedTransform, Layer, LayerContent, Rational, TextStyle, Time,
};
use celesta_react_bridge::{
    ComponentPropertyField, ComponentResolutionRequest, ProjectFrame, PropertyInputs, ReactBridge,
    ReactBridgeError, react_audio_clips,
};

fn no_tracks() -> BTreeMap<String, Vec<Layer>> {
    BTreeMap::new()
}

/// Depth-first iteration over a layer tree — `<Sequence>` wraps its children
/// in a group, so content of interest can be nested.
fn all_layers(layers: &[Layer]) -> Vec<&Layer> {
    let mut all = Vec::new();
    for layer in layers {
        if let LayerContent::Group {
            layers: children, ..
        } = &layer.content
        {
            all.extend(all_layers(children));
        }
        all.push(layer);
    }
    all
}

#[test]
fn evaluates_the_example_composition_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/title.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let metadata = bridge.metadata().clone();
    assert_eq!((metadata.width, metadata.height), (1920, 1080));
    assert_eq!(metadata.frame_rate, Rational::new(30, 1));
    assert_eq!(metadata.duration_in_frames, 150);

    let scene = bridge.scene_at(Time::new(0, 30)).unwrap();
    assert_eq!((scene.width, scene.height), (1920, 1080));
    assert_eq!(scene.layers.len(), 1);

    // A second request on the same live process exercises the persistent
    // pipe rather than spawning a new Node process per frame.
    let scene_at_frame_fifteen = bridge.scene_at(Time::new(15, 30)).unwrap();
    assert_eq!(scene_at_frame_fifteen.time, Time::new(15, 30));
}

#[test]
fn render_text_metrics_match_loaded_fonts_and_component_preview_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };
    let entry = package_root.join("examples/with-text-metrics.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    let mut measurer = celesta_renderer::TextRasterizer::new();

    for frame in [0, 9, 99, 0] {
        let time = Time::new(frame, 30);
        let scene = bridge.scene_at(time).unwrap();
        measurer
            .load_fonts(&scene.fonts, entry.parent().unwrap())
            .unwrap();
        let layers = all_layers(&scene.layers);
        let backgrounds: Vec<_> = layers
            .iter()
            .filter(|layer| layer.id == "pill-background")
            .collect();
        let labels: Vec<_> = layers
            .iter()
            .filter(|layer| layer.id == "pill-label")
            .collect();
        assert_eq!(backgrounds.len(), 4);
        for (background, label) in backgrounds.iter().zip(&labels) {
            let LayerContent::Text { text, style, .. } = &label.content else {
                panic!("expected a text label");
            };
            assert_eq!(style.lang.as_deref(), Some("ja-JP"));
            let metrics = measurer.measure(text, style, None);
            let LayerContent::Rect {
                width,
                height,
                corner_radius,
                ..
            } = &background.content
            else {
                panic!("expected a fitted rectangle");
            };
            assert_eq!(*width, metrics.width + 48.0);
            assert_eq!(*height, metrics.height + 24.0);
            assert_eq!(*corner_radius, height / 2.0);
            assert_eq!(label.transform.position.y, metrics.ascent + 12.0);
        }

        let heading = &scene.layers[0];
        let LayerContent::Group { layers: runs, .. } = &heading.content else {
            panic!("expected a heading group");
        };
        let mut advance = 0.0;
        for run in runs {
            assert_eq!(run.transform.position.x, advance);
            let LayerContent::Text {
                text,
                style,
                baseline_anchor,
                ..
            } = &run.content
            else {
                panic!("expected a heading run");
            };
            assert!(*baseline_anchor);
            advance += measurer.measure(text, style, None).width;
        }

        let row = &scene.layers[2];
        let LayerContent::Group { layers: chips, .. } = &row.content else {
            panic!("expected a chip row");
        };
        let widths: Vec<_> = backgrounds[1..]
            .iter()
            .map(|layer| {
                let LayerContent::Rect { width, .. } = layer.content else {
                    unreachable!()
                };
                width
            })
            .collect();
        assert_eq!(
            row.transform.position.x,
            (1600.0 - widths.iter().sum::<f64>() - 32.0) / 2.0
        );
        assert_eq!(chips[1].transform.position.x, widths[0] + 16.0);
        assert_eq!(chips[2].transform.position.x, widths[0] + widths[1] + 32.0);

        let props = BTreeMap::new();
        let preview = bridge
            .resolve_components(
                &[ComponentResolutionRequest {
                    component: "MeasuredPill",
                    props: &props,
                }],
                time,
            )
            .unwrap();
        let preview_layers = all_layers(preview[0].as_deref().unwrap());
        assert_eq!(
            preview_layers
                .iter()
                .find(|layer| layer.id == "pill-background")
                .unwrap()
                .content,
            backgrounds[0].content
        );
        assert_eq!(
            preview_layers
                .iter()
                .find(|layer| layer.id == "pill-label")
                .unwrap()
                .content,
            labels[0].content
        );

        let LayerContent::Group {
            layers: export_pill,
            ..
        } = &scene.layers[1].content
        else {
            panic!("expected a pill group");
        };
        let mut export_scene = scene.clone();
        export_scene.layers = export_pill.clone();
        let mut preview_scene = scene.clone();
        preview_scene.layers = preview[0].clone().unwrap();
        let mut renderer = celesta_renderer::CpuRenderer::new(Default::default())
            .with_asset_root(entry.parent().unwrap());
        assert_eq!(
            renderer.render(&export_scene).unwrap(),
            renderer.render(&preview_scene).unwrap()
        );
    }
}

#[test]
fn text_boxes_fit_their_captions_with_the_renderer_shaping_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };
    let entry = package_root.join("examples/with-text-box.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    let mut measurer = celesta_renderer::TextRasterizer::new();
    let time = Time::new(0, 30);
    let scene = bridge.scene_at(time).unwrap();
    measurer
        .load_fonts(&scene.fonts, entry.parent().unwrap())
        .unwrap();

    let boxes: Vec<_> = all_layers(&scene.layers)
        .into_iter()
        .filter(|layer| layer.id == "caption-text")
        .collect();
    assert_eq!(boxes.len(), 5);
    // The caption box: 1024×136, two lines, line height 1.3 × the size.
    let fits = |measurer: &mut celesta_renderer::TextRasterizer,
                text: &str,
                style: &TextStyle,
                size: f64| {
        let style = TextStyle {
            font_size: Some(size),
            line_height: Some(size * 1.3),
            ..style.clone()
        };
        let metrics = measurer.measure(text, &style, Some(1024.0));
        metrics.width <= 1024.001 && metrics.height <= 136.001 && metrics.lines <= 2
    };
    let mut sizes = Vec::new();
    for (index, text_box) in boxes.iter().enumerate() {
        let LayerContent::Group { layers, clip, .. } = &text_box.content else {
            panic!("expected a text box group");
        };
        let LayerContent::Text {
            text,
            style,
            max_width,
            baseline_anchor,
        } = &layers[0].content
        else {
            panic!("expected the fitted text");
        };
        assert_eq!(*max_width, Some(1024.0));
        assert!(*baseline_anchor);
        let size = style.font_size.unwrap();
        assert_eq!(style.line_height, Some(size * 1.3));
        // The drawn style is the measured one, so what fits is what draws.
        let metrics = measurer.measure(text, style, *max_width);
        let fitted = fits(&mut measurer, text, style, size);
        if index < 4 {
            assert!(fitted, "caption {index} fits at {size}px: {metrics:?}");
            assert!(clip.is_none());
            assert!(
                size == 64.0 || !fits(&mut measurer, text, style, size + 1.0),
                "caption {index} could be larger than {size}px"
            );
            assert_eq!(
                layers[0].transform.position.y,
                (136.0 - metrics.height) / 2.0 + metrics.ascent
            );
        } else {
            assert!(!fitted);
            assert_eq!(size, 28.0);
            assert!(clip.is_some());
        }
        sizes.push(size);
    }
    assert_eq!(sizes[0], 64.0);
    assert!(sizes[2] < 64.0 && sizes[2] >= 28.0, "{sizes:?}");

    let mut props = BTreeMap::new();
    let LayerContent::Group { layers, .. } = &boxes[2].content else {
        unreachable!()
    };
    let LayerContent::Text { text, .. } = &layers[0].content else {
        unreachable!()
    };
    props.insert("text".to_owned(), serde_json::json!(text));
    let preview = bridge
        .resolve_components(
            &[ComponentResolutionRequest {
                component: "Caption",
                props: &props,
            }],
            time,
        )
        .unwrap();
    let preview_box = all_layers(preview[0].as_deref().unwrap())
        .into_iter()
        .find(|layer| layer.id == "caption-text")
        .unwrap()
        .clone();
    // Child ids differ between the two paths; what draws does not.
    let LayerContent::Group {
        layers: preview_layers,
        clip: preview_clip,
        ..
    } = &preview_box.content
    else {
        panic!("expected a text box group");
    };
    assert!(preview_clip.is_none());
    assert_eq!(preview_layers[0].content, layers[0].content);
    assert_eq!(preview_layers[0].transform, layers[0].transform);
}

#[test]
fn prepare_can_preload_media_metadata_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-media-info.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    assert!(bridge.metadata().duration_in_frames > 30);

    let scene = bridge.scene_at(Time::ZERO).unwrap();
    assert!(scene.layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text.starts_with("voice: ") && text.ends_with(" Hz")
    )));
}

#[test]
fn debug_guides_only_render_during_component_preview_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-debug.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    assert!(bridge.scene_at(Time::new(5, 30)).unwrap().layers.is_empty());

    let props = BTreeMap::new();
    let resolved = bridge
        .resolve_components(
            &[ComponentResolutionRequest {
                component: "DebugCard",
                props: &props,
            }],
            Time::new(5, 30),
        )
        .unwrap();
    let layers = all_layers(resolved[0].as_deref().unwrap());
    assert!(layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "frame 5 · 640×360"
    )));
    assert!(layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "card"
    )));
}

#[test]
fn evaluates_a_psd_character_with_one_selected_mouth_layer_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-psd-character.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    let scene = bridge.scene_at(Time::ZERO).unwrap();
    let LayerContent::Psd {
        asset,
        visible_layers,
        enabled_layers,
        disabled_layers,
    } = &scene.layers[0].content
    else {
        panic!("expected a PSD layer");
    };
    assert_eq!(asset.id, "./teto.psd");
    assert!(visible_layers.is_empty());
    assert_eq!(enabled_layers, &["本体/顔パーツ/口/あいうえお/あ"]);
    assert_eq!(disabled_layers.len(), 5);
}

#[test]
fn evaluates_a_react_dialogue_as_a_character_and_subtitle_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-dialogue.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    let evaluation = bridge.evaluate_at(Time::ZERO, None).unwrap();
    let layers = all_layers(&evaluation.scene.layers);

    assert!(layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Image { asset, .. } if asset.id == "./character.png"
    )));
    assert!(layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, max_width, .. }
            if text == "React から Dialogue を表示できます。" && *max_width == Some(1120.0)
    )));
    assert_eq!(evaluation.audio.len(), 1);
    assert_eq!(evaluation.audio[0].src, "./voice.wav");
}

#[test]
fn resolves_a_psd_portrait_preset_into_visible_layers_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-psd-preset.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    let scene = bridge.scene_at(Time::ZERO).unwrap();
    let LayerContent::Psd {
        visible_layers,
        enabled_layers,
        disabled_layers,
        ..
    } = &scene.layers[0].content
    else {
        panic!("expected a PSD layer");
    };
    // The `layers` prop was a raw PSDTool "all layer" state string; render.ts
    // parsed it into the visible-layer list.
    assert!(visible_layers.contains(&"body/base".to_owned()));
    assert!(visible_layers.contains(&"face/eyes/open".to_owned()));
    assert_eq!(enabled_layers, &["face/mouth/a"]);
    assert_eq!(disabled_layers.len(), 5);
}

#[test]
fn generates_a_lip_sync_mouth_track_from_a_wav_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-lip-sync.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let mouth_at = |bridge: &mut ReactBridge, frame: i64| -> Vec<String> {
        let scene = bridge.scene_at(Time::new(frame, 30)).unwrap();
        let LayerContent::Psd { enabled_layers, .. } = &scene.layers[0].content else {
            panic!("expected a PSD layer");
        };
        enabled_layers.clone()
    };

    // 001.wav opens with a beat of silence, so the first frame is closed; a
    // frame partway through the voiced span opens to a vowel from あいうえお.
    assert_eq!(
        mouth_at(&mut bridge, 0),
        vec!["face/mouth/closed".to_owned()]
    );
    let voiced = mouth_at(&mut bridge, 15);
    assert_eq!(voiced.len(), 1);
    assert!(
        ["a", "i", "u", "e", "o"]
            .iter()
            .any(|shape| voiced[0] == format!("face/mouth/{shape}")),
        "expected a vowel mouth at frame 15, got {voiced:?}"
    );
}

#[test]
fn computes_video_timing_from_the_composition_clock_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-video.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    // <Video src="./clip.mp4" startFrom={1} playbackRate={2} /> has no
    // timeline-item range the way a project clip does, so it plays synced
    // to the composition's own clock from frame 0: at frame 15/30 (0.5s in),
    // sourceTimeSeconds should be startFrom + 0.5 * playbackRate = 2.0.
    let scene = bridge.scene_at(Time::new(15, 30)).unwrap();
    assert_eq!(scene.layers.len(), 1);
    let LayerContent::Video { asset, timing } = &scene.layers[0].content else {
        panic!("expected a video layer");
    };
    assert_eq!(asset.id, "./clip.mp4");
    assert_eq!(timing.playback_rate, 2.0);
    assert_eq!(timing.source_time_seconds, 2.0);
}

#[test]
fn evaluates_a_rect_with_fill_stroke_and_corner_radius_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-rect.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let scene = bridge.scene_at(Time::new(0, 30)).unwrap();
    assert_eq!(scene.layers.len(), 1);
    let LayerContent::Rect {
        width,
        height,
        fill,
        stroke,
        corner_radius,
    } = &scene.layers[0].content
    else {
        panic!("expected a rect layer");
    };
    assert_eq!(*width, 200.0);
    assert_eq!(*height, 100.0);
    assert_eq!(
        fill,
        &Some(celesta_composition::Paint::Solid {
            color: "#3366CC".to_owned()
        })
    );
    let stroke = stroke.as_ref().expect("expected a stroke");
    assert_eq!(
        stroke.paint,
        celesta_composition::Paint::Solid {
            color: "#FFFFFF".to_owned()
        }
    );
    assert_eq!(stroke.width, 4.0);
    assert_eq!(*corner_radius, 16.0);
}

#[test]
fn interpolates_rect_text_and_gradient_stop_colors_when_node_is_available() {
    use celesta_composition::{GradientStop, Paint};

    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-color.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    let solid = |color: &str| Paint::Solid {
        color: color.to_owned(),
    };
    // [sky rect fill, gradient stops, text fill] at a frame.
    let mut colors_at = |frame: i64| {
        let scene = bridge.scene_at(Time::new(frame, 30)).unwrap();
        let LayerContent::Rect { fill: sky, .. } = &scene.layers[0].content else {
            panic!("expected the sky rect");
        };
        let LayerContent::Rect {
            fill: Some(Paint::Linear { stops, .. }),
            ..
        } = &scene.layers[1].content
        else {
            panic!("expected a rect with a linear gradient");
        };
        let LayerContent::Text { style, .. } = &scene.layers[2].content else {
            panic!("expected the title text");
        };
        (
            sky.clone().unwrap(),
            stops.clone(),
            style.fill.clone().unwrap(),
        )
    };
    let stops = |top: &str, bottom: &str| {
        vec![
            GradientStop {
                offset: 0.0,
                color: top.to_owned(),
            },
            GradientStop {
                offset: 1.0,
                color: bottom.to_owned(),
            },
        ]
    };

    // Frame 0: each range's first color; the bottom stop's range starts at
    // 30, so it holds its first color.
    let (sky, gradient, title) = colors_at(0);
    assert_eq!(sky, solid("#101820FF"));
    assert_eq!(gradient, stops("#FFD84D00", "#3366CCFF"));
    assert_eq!(title, solid("#FFFFFF00"));

    // Frame 45: the sky's middle color exactly; the bottom stop a quarter
    // through 30..90; the text 25/70 of the way from white to #FFD84D.
    let (sky, gradient, title) = colors_at(45);
    assert_eq!(sky, solid("#EF7B45FF"));
    assert_eq!(gradient[1].color, "#625DA4FF");
    assert_eq!(title, solid("#FFF1BFFF"));
}

#[test]
fn draws_circles_as_rects_and_ellipses_and_arrows_as_paths_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-shapes.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    // At frame 0 nothing is rotated, which the CPU renderer needs, and the
    // growing circle has no radius yet, so it draws nothing.
    let scene = bridge.scene_at(Time::new(0, 30)).unwrap();
    let layers = all_layers(&scene.layers);
    let paths = layers
        .iter()
        .filter(|layer| matches!(layer.content, LayerContent::Path { .. }))
        .count();
    // 5 ellipses, 12 + 6 arrows; the rest are rects and a group.
    assert_eq!(paths, 23);
    let circles = layers
        .iter()
        .filter(|layer| {
            matches!(layer.content, LayerContent::Rect { width, corner_radius, .. }
                if corner_radius * 2.0 == width)
        })
        .count();
    assert_eq!(circles, 4);

    let frame = celesta_renderer::CpuRenderer::default()
        .render(&scene)
        .unwrap();
    let pixel = |x: u32, y: u32| {
        let offset = ((y * frame.width() + x) * 4) as usize;
        <[u8; 3]>::try_from(&frame.pixels()[offset..offset + 3]).unwrap()
    };
    let background = [0x10, 0x18, 0x20];
    // A filled circle, and a stroked one whose stroke stays inside its box.
    assert_eq!(pixel(140, 120), [0x33, 0x66, 0xCC]);
    assert_eq!(pixel(300, 120), background);
    assert_eq!(pixel(300, 64), [0xFF, 0xD8, 0x4D]);
    assert_eq!(pixel(300, 58), background);
    // An ellipse fills its box from the top-left, not its corners.
    assert_eq!(pixel(180, 290), [0x8E, 0x6C, 0xFF]);
    assert_eq!(pixel(84, 244), background);
    // An arrow's shaft and head, with its tip at the end point.
    assert_eq!(pixel(480, 460), [0x4D, 0xD8, 0xC0]);
    assert_eq!(pixel(604, 466), [0x4D, 0xD8, 0xC0]);
    assert_eq!(pixel(480, 466), background);
    assert_eq!(pixel(624, 460), background);
}

#[test]
fn reports_audio_clips_per_frame_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-audio.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    // <Audio src="./voice.wav" startFrom={1} playbackRate={2} volume={0.5}
    // muted={false} /> is reported by every rendered frame's evaluation
    // (gathered during the same tree walk as the layers), and contributes no
    // visual layer itself.
    let evaluation = bridge.evaluate_at(Time::new(0, 30), None).unwrap();
    assert_eq!(evaluation.scene.layers.len(), 1);
    assert!(matches!(
        &evaluation.scene.layers[0].content,
        LayerContent::Text { .. }
    ));
    assert_eq!(evaluation.audio.len(), 1);
    let clip = &evaluation.audio[0];
    assert_eq!(clip.src, "./voice.wav");
    assert_eq!(clip.source_start, 1.0);
    assert_eq!(clip.playback_rate, Animatable::Static(2.0));
    assert_eq!(clip.volume, Animatable::Static(0.5));
    assert!(!clip.muted);
    // Bare <Audio> spans the whole composition.
    assert_eq!(clip.start, 0.0);
    assert_eq!(clip.duration, 1.0);
}

#[test]
fn batched_audio_graph_matches_full_frame_evaluation_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };
    for fixture in [
        "examples/title.tsx",
        "examples/with-audio.tsx",
        "examples/with-conditional-audio.tsx",
        "examples/with-sequence.tsx",
        "examples/with-volume-fade.tsx",
        "examples/with-transition-series.tsx",
        "test/fixtures/audio-collection.tsx",
    ] {
        let entry = package_root.join(fixture);
        let entry_dir = entry.parent().unwrap();
        let mut reference = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
        let mut batched = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
        let metadata = reference.metadata().clone();
        // Repeat on the same roots to exercise retained hook state and seeking
        // back from the final frame to the start of the next sweep.
        for _ in 0..2 {
            let mut reports = Vec::new();
            for frame in 0..metadata.duration_in_frames {
                let time = Time::frames(frame as i64, metadata.frame_rate).unwrap();
                reports.extend(reference.evaluate_at(time, None).unwrap().audio);
            }
            let expected = celesta_composition::AudioGraph {
                sample_rate: 44_100,
                master_volume: 0.75,
                clips: react_audio_clips(&reports, entry_dir),
            };
            let actual = batched
                .collect_audio_graph(44_100, 0.75, entry_dir)
                .unwrap();
            assert_eq!(actual, expected, "{fixture}");
        }
        let actual = batched.evaluate_at(Time::ZERO, None).unwrap();
        let expected = reference.evaluate_at(Time::ZERO, None).unwrap();
        assert_eq!(
            actual.scene, expected.scene,
            "rendering after collection: {fixture}"
        );
        assert_eq!(
            actual.audio, expected.audio,
            "audio after collection: {fixture}"
        );
    }
}

#[test]
fn collect_audio_graph_sweeps_every_frame_into_one_graph_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    // Bare <Audio>: reported by all 30 frames, merged into a single clip.
    let entry = package_root.join("examples/with-audio.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    let graph = bridge
        .collect_audio_graph(48_000, 1.0, entry.parent().unwrap())
        .unwrap();
    assert_eq!(graph.sample_rate, 48_000);
    assert_eq!(graph.clips.len(), 1);
    assert_eq!(graph.clips[0].id, "react-audio:0");
    assert_eq!(graph.clips[0].source_start.as_seconds().unwrap(), 1.0);
    let celesta_composition::AssetLocation::File { path } = &graph.clips[0].asset.location else {
        panic!("expected a file asset");
    };
    assert!(
        Path::new(path).is_absolute(),
        "relative <Audio> src is absolutized against the entry dir, got {path}"
    );

    // Conditionally rendered <Audio> (frame >= 15): still one merged clip,
    // because collect_audio_graph visits every frame and merges duplicates.
    let entry = package_root.join("examples/with-conditional-audio.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    let graph = bridge
        .collect_audio_graph(48_000, 1.0, entry.parent().unwrap())
        .unwrap();
    assert_eq!(graph.clips.len(), 1);
    assert!(matches!(&graph.clips[0].volume, Animatable::Keyframes(_)));
}

#[test]
fn shifts_media_inside_sequences_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-sequence.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    // Frame 30 starts the first sequence; the video inside it plays synced
    // to the sequence's own clock (startFrom={1} playbackRate={2}), so at
    // frame 45 — half a second into the sequence — sourceTimeSeconds is
    // 1 + 0.5 * 2 = 2.0, exactly what the same props produce unsequenced at
    // frame 15.
    let mid = bridge.evaluate_at(Time::new(45, 30), None).unwrap();
    let video = all_layers(&mid.scene.layers)
        .into_iter()
        .find_map(|layer| match &layer.content {
            LayerContent::Video { timing, .. } => Some(timing),
            _ => None,
        })
        .expect("the sequenced video renders from its sequence's window on");
    assert_eq!(video.source_time_seconds, 2.0);

    // The audio inside that sequence is audible from the sequence's start to
    // the end of the composition, starting at its own local zero.
    assert_eq!(mid.audio.len(), 1);
    assert_eq!(mid.audio[0].src, "./voice.wav");
    assert_eq!(mid.audio[0].start, 1.0);
    assert_eq!(mid.audio[0].duration, 2.0);
    assert_eq!(mid.audio[0].source_start, 0.0);

    // Before the sequence starts: neither the video nor the audio exists,
    // and neither does the second sequence's text (frames 60-89 only).
    let early = bridge.evaluate_at(Time::ZERO, None).unwrap();
    assert_eq!(early.scene.layers.len(), 0);
    assert_eq!(early.audio.len(), 0);

    // Inside the second sequence, useCurrentFrame() reports the shifted
    // local clock: at composition frame 75 the text says "frame 15 inside".
    let late = bridge.evaluate_at(Time::new(75, 30), None).unwrap();
    assert!(all_layers(&late.scene.layers).iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "frame 15 inside"
    )));
}

#[test]
fn collects_conditionally_rendered_audio_with_keyframed_volume_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-conditional-audio.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    // The <Audio> is behind `{frame >= 15 && ...}`, so frames before 15
    // report nothing while later frames report it with its keyframed volume
    // animation intact.
    let before = bridge.evaluate_at(Time::ZERO, None).unwrap();
    assert_eq!(before.audio.len(), 0);

    let after = bridge.evaluate_at(Time::new(20, 30), None).unwrap();
    assert_eq!(after.audio.len(), 1);
    let clip = &after.audio[0];
    assert_eq!(clip.src, "./voice.wav");
    assert_eq!(clip.start, 0.0);
    assert_eq!(clip.duration, 2.0);
    let Animatable::Keyframes(volume) = &clip.volume else {
        panic!("expected the declared keyframed volume to survive collection");
    };
    assert_eq!(volume.keyframes.len(), 2);
    assert_eq!(volume.keyframes[0].value, 0.25);
    assert_eq!(volume.keyframes[1].value, 1.0);
}

#[test]
fn transition_series_overlaps_scenes_and_cross_fades_their_audio_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    // Five 60-frame cards joined by a cut and three 20-frame transitions.
    let entry = package_root.join("examples/with-transition-series.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    assert_eq!(bridge.metadata().duration_in_frames, 240);

    // Frames 100-119 overlap the cut card with the cross-faded one; the cut
    // at frame 60 swaps cards with no shared frame.
    let scenes_at = |bridge: &mut ReactBridge, frame: i64| {
        bridge.scene_at(Time::new(frame, 30)).unwrap().layers.len()
    };
    assert_eq!(scenes_at(&mut bridge, 59), 1);
    assert_eq!(scenes_at(&mut bridge, 60), 1);
    assert_eq!(scenes_at(&mut bridge, 99), 1);
    assert_eq!(scenes_at(&mut bridge, 100), 2);
    assert_eq!(scenes_at(&mut bridge, 119), 2);
    assert_eq!(scenes_at(&mut bridge, 120), 1);

    let graph = bridge
        .collect_audio_graph(48_000, 1.0, entry.parent().unwrap())
        .unwrap();
    let mut clips: Vec<_> = graph.clips.iter().collect();
    clips.sort_by(|left, right| left.range.start.cmp_exact(right.range.start).unwrap());
    let starts: Vec<f64> = clips
        .iter()
        .map(|clip| (clip.range.start.as_seconds().unwrap() * 30.0).round())
        .collect();
    assert_eq!(starts, [0.0, 60.0, 100.0, 140.0, 180.0]);
    // The mixer evaluates volume at clip-local time: across the cross-fade
    // the two cards' volumes add up to the declared 0.8.
    let volume_at = |clip: &celesta_composition::AudioClip, frame: i64| {
        celesta_composition::evaluate_f64(&clip.volume, Time::new(frame, 30)).unwrap()
    };
    assert_eq!(volume_at(clips[0], 0), 0.8);
    assert_eq!(volume_at(clips[1], 0), 0.8);
    for frame in [100, 105, 110, 119] {
        let sum = volume_at(clips[1], frame - 60) + volume_at(clips[2], frame - 100);
        assert!((sum - 0.8).abs() < 1e-4, "frame {frame}: {sum}");
    }
    assert_eq!(volume_at(clips[2], 0), 0.0);
    // Keys are rounded to the microsecond, a hair after frame 20's exact time.
    assert!((volume_at(clips[2], 20) - 0.8).abs() < 1e-4);
}

#[test]
fn frame_keyframes_fade_audio_in_the_collected_graph_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-volume-fade.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();
    let graph = bridge
        .collect_audio_graph(48_000, 1.0, entry.parent().unwrap())
        .unwrap();
    assert_eq!(graph.clips.len(), 2);
    let mut clips: Vec<_> = graph.clips.iter().collect();
    clips.sort_by(|left, right| left.range.start.cmp_exact(right.range.start).unwrap());
    // The mixer evaluates volume with evaluate_f64 at clip-local time, for
    // preview and export.
    let volume_at = |clip: &celesta_composition::AudioClip, frame: i64| {
        celesta_composition::evaluate_f64(&clip.volume, Time::new(frame, 30)).unwrap()
    };

    // BGM: frames 0-15 fade in to 0.8, frames 75-90 fade out.
    let bgm = clips[0];
    assert_eq!(bgm.range.start.as_seconds().unwrap(), 0.0);
    assert_eq!(volume_at(bgm, 0), 0.0);
    assert_eq!(volume_at(bgm, 15), 0.8);
    assert_eq!(volume_at(bgm, 75), 0.8);
    assert!(volume_at(bgm, 80) < 0.8);
    assert_eq!(volume_at(bgm, 90), 0.0);

    // Voice: starts at frame 45 inside nested sequences; keys written in
    // composition frames with origin 45 fade in over its first 6 frames.
    let voice = clips[1];
    assert_eq!(voice.range.start.as_seconds().unwrap(), 1.5);
    assert_eq!(volume_at(voice, 0), 0.0);
    assert!((volume_at(voice, 3) - 0.5).abs() < 1e-4);
    assert_eq!(volume_at(voice, 6), 1.0);
}

#[test]
fn resolves_individual_components_through_the_bridge_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-registered-component.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let mut props = BTreeMap::new();
    props.insert("bossName".to_owned(), serde_json::json!("Golem"));
    props.insert("level".to_owned(), serde_json::json!(42));
    let requests = [
        ComponentResolutionRequest {
            component: "BossIntroduction",
            props: &props,
        },
        ComponentResolutionRequest {
            component: "SomeOtherThing",
            props: &BTreeMap::new(),
        },
    ];

    let resolved = bridge
        .resolve_components(&requests, Time::new(0, 30))
        .unwrap();
    assert_eq!(resolved.len(), 2);
    let registered = resolved[0]
        .as_ref()
        .expect("the registered component resolves");
    assert!(registered.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "Golem (Lv.42)"
    )));
    assert!(
        resolved[1].is_none(),
        "an unregistered name resolves to none"
    );
}

#[test]
fn reports_a_declared_project_property_schema_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-properties.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let schema = bridge
        .metadata()
        .project_property_schema
        .as_ref()
        .expect("the entry declares a project property schema");
    assert_eq!(
        schema.get("title"),
        Some(&ComponentPropertyField::String {
            label: Some("Title".to_owned()),
            default_value: "Celesta".to_owned(),
        })
    );
    assert_eq!(
        schema.get("accent"),
        Some(&ComponentPropertyField::Color {
            label: Some("Accent".to_owned()),
            default_value: "#ff8800".to_owned(),
        })
    );
    assert_eq!(
        schema.get("fontSize"),
        Some(&ComponentPropertyField::Number {
            label: Some("Font Size".to_owned()),
            default_value: 48.0,
            min: Some(8.0),
            max: Some(200.0),
            step: Some(2.0),
        })
    );
    assert_eq!(
        schema.get("showSubtitle"),
        Some(&ComponentPropertyField::Boolean {
            label: Some("Show Subtitle".to_owned()),
            default_value: false,
        })
    );
    assert_eq!(
        schema.get("weight"),
        Some(&ComponentPropertyField::Select {
            label: Some("Weight".to_owned()),
            default_value: "bold".to_owned(),
            options: vec!["bold".to_owned(), "light".to_owned()],
        })
    );

    // An entry that never calls defineProjectProperties() leaves the
    // metadata field None — distinct from a declared-but-empty schema.
    let plain = package_root.join("examples/title.tsx");
    let plain_bridge = ReactBridge::spawn(&node, &cli_script, &plain).unwrap();
    assert!(plain_bridge.metadata().project_property_schema.is_none());

    // The entry's own useProjectProperty() call reads its embedded project's
    // `properties.title` ("Chapter 3"), not the declared default.
    let scene = bridge.scene_at(Time::new(0, 30)).unwrap();
    assert!(scene.layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "Chapter 3"
    )));
}

#[test]
fn passes_project_property_inputs_over_the_provider_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };
    let entry = package_root.join("examples/with-properties.tsx");
    let text_of = |bridge: &mut ReactBridge| {
        let scene = bridge.scene_at(Time::new(0, 30)).unwrap();
        scene
            .layers
            .iter()
            .find_map(|layer| match &layer.content {
                LayerContent::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .expect("the entry draws its title")
    };

    // --props beats the companion project, which beats the in-source Provider;
    // the project's undeclared key is ignored rather than rejected.
    let project = BTreeMap::from([
        ("title".to_owned(), serde_json::json!("From project")),
        ("unrelated".to_owned(), serde_json::json!(true)),
    ]);
    let from_project = PropertyInputs::default().with_project(&project, None, &package_root);
    let mut bridge =
        ReactBridge::spawn_with_properties(&node, &cli_script, &entry, &from_project).unwrap();
    assert_eq!(text_of(&mut bridge), "From project");

    let inline = PropertyInputs::load(None, Some(r#"{"title":"From CLI"}"#))
        .unwrap()
        .with_project(&project, None, &package_root);
    let mut bridge =
        ReactBridge::spawn_with_properties(&node, &cli_script, &entry, &inline).unwrap();
    assert_eq!(text_of(&mut bridge), "From CLI");

    let invalid = PropertyInputs::load(None, Some(r#"{"fontSize":"big","titel":"x"}"#)).unwrap();
    match ReactBridge::spawn_with_properties(&node, &cli_script, &entry, &invalid) {
        Err(ReactBridgeError::InvalidProperties(issues)) => {
            let keys: Vec<_> = issues.iter().map(|issue| issue.key.as_str()).collect();
            assert_eq!(keys, ["fontSize", "titel"]);
            assert!(issues.iter().all(|issue| issue.source == "--props"));
        }
        Err(error) => panic!("expected invalid properties, got {error}"),
        Ok(_) => panic!("expected invalid properties to fail the spawn"),
    }
}

#[test]
fn resolves_components_against_the_requested_time_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-frame-component.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    // The entry's own <Composition> declares 640x360@30fps; a resolved
    // component's useVideoConfig() must see those same facts.
    let request = [ComponentResolutionRequest {
        component: "FrameCaption",
        props: &BTreeMap::new(),
    }];

    let at_frame_fifteen = bridge
        .resolve_components(&request, Time::new(15, 30))
        .unwrap();
    let layers = at_frame_fifteen[0]
        .as_ref()
        .expect("the registered component resolves");
    assert!(layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "frame 15 of 640 at 30fps"
    )));

    // A second call on the same persistent root follows the new time rather
    // than staying on the first one (hook state persists; the clock moves).
    let at_frame_seven = bridge
        .resolve_components(&request, Time::new(7, 30))
        .unwrap();
    let layers = at_frame_seven[0]
        .as_ref()
        .expect("the registered component resolves");
    assert!(layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "frame 7 of 640 at 30fps"
    )));
}

#[test]
fn embeds_pre_evaluated_project_layers_into_project_timeline_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-project.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let project_layer = Layer {
        id: "from-project".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: "from the project".to_owned(),
            style: TextStyle::default(),
            max_width: None,
            baseline_anchor: false,
        },
    };

    let layers = [project_layer];
    let tracks = no_tracks();
    let scene = bridge
        .scene_at_with_project(
            Time::new(0, 30),
            Some(ProjectFrame {
                layers: &layers,
                tracks: &tracks,
            }),
        )
        .unwrap();

    // <ProjectTimeline /> passes the given layers through untouched, and the
    // entry's own <Text> sibling still renders alongside them.
    assert!(scene.layers.iter().any(|layer| layer.id == "from-project"));
    assert!(scene.layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "React overlay"
    )));
}

#[test]
fn resolves_a_registered_component_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-registered-component.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let mut props = BTreeMap::new();
    props.insert("bossName".to_owned(), serde_json::json!("Golem"));
    props.insert("level".to_owned(), serde_json::json!(42));
    let registered_layer = Layer {
        id: "boss-intro".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::MissingComponent {
            component: "BossIntroduction".to_owned(),
            props,
        },
    };
    let unregistered_layer = Layer {
        id: "unregistered".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::MissingComponent {
            component: "SomeOtherThing".to_owned(),
            props: BTreeMap::new(),
        },
    };

    let layers = [registered_layer, unregistered_layer];
    let tracks = no_tracks();
    let scene = bridge
        .scene_at_with_project(
            Time::new(0, 30),
            Some(ProjectFrame {
                layers: &layers,
                tracks: &tracks,
            }),
        )
        .unwrap();

    // The registered component resolved to a real rendered subtree: a group
    // wrapping the Text it rendered, not the original missingComponent
    // layer.
    let resolved_group = scene
        .layers
        .iter()
        .find(|layer| layer.id == "boss-intro")
        .expect("resolved component layer");
    let LayerContent::Group {
        layers: children, ..
    } = &resolved_group.content
    else {
        panic!("expected the resolved component to render as a group");
    };
    assert!(children.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "Golem (Lv.42)"
    )));

    // The unregistered component name passes through unresolved.
    let unregistered = scene
        .layers
        .iter()
        .find(|layer| layer.id == "unregistered")
        .expect("unresolved component layer");
    assert!(matches!(
        &unregistered.content,
        LayerContent::MissingComponent { component, .. } if component == "SomeOtherThing"
    ));
}

#[test]
fn reports_a_registered_components_property_schema_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-registered-component.tsx");
    let bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let schemas = &bridge.metadata().component_schemas;
    let schema = schemas
        .get("BossIntroduction")
        .expect("BossIntroduction declares a schema");
    assert_eq!(
        schema.get("bossName"),
        Some(&ComponentPropertyField::String {
            label: Some("Boss Name".to_owned()),
            default_value: "Golem".to_owned(),
        })
    );
    assert_eq!(
        schema.get("level"),
        Some(&ComponentPropertyField::Number {
            label: Some("Level".to_owned()),
            default_value: 1.0,
            min: Some(1.0),
            max: Some(999.0),
            step: None,
        })
    );
}

#[test]
fn embeds_per_track_layers_for_use_project_track_when_node_is_available() {
    let Some((node, cli_script, package_root)) = live_react_runtime() else {
        return;
    };

    let entry = package_root.join("examples/with-project-track.tsx");
    let mut bridge = ReactBridge::spawn(&node, &cli_script, &entry).unwrap();

    let titles_layer = Layer {
        id: "title-1".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: "a title".to_owned(),
            style: TextStyle::default(),
            max_width: None,
            baseline_anchor: false,
        },
    };
    let overlay_layer = Layer {
        id: "overlay-1".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: "an overlay".to_owned(),
            style: TextStyle::default(),
            max_width: None,
            baseline_anchor: false,
        },
    };
    let mut tracks = BTreeMap::new();
    tracks.insert("titles".to_owned(), vec![titles_layer]);
    tracks.insert("overlays".to_owned(), vec![overlay_layer]);
    let layers: Vec<Layer> = Vec::new();

    let scene = bridge
        .scene_at_with_project(
            Time::new(0, 30),
            Some(ProjectFrame {
                layers: &layers,
                tracks: &tracks,
            }),
        )
        .unwrap();

    // useProjectTrack("titles") only saw that one track's layer count.
    assert!(scene.layers.iter().any(|layer| matches!(
        &layer.content,
        LayerContent::Text { text, .. } if text == "titles track has 1 layer(s)"
    )));
    // <ProjectTrack id="overlays" /> passed its track's layer through as-is.
    assert!(scene.layers.iter().any(|layer| layer.id == "overlay-1"));
    // Neither saw the other's track content directly.
    assert!(!scene.layers.iter().any(|layer| layer.id == "title-1"));
}

fn live_react_runtime() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let Some(node) = find_executable("CELESTA_NODE", "node") else {
        eprintln!("skipping live Node.js test: node was not found");
        return None;
    };

    let package_root = react_package_root();
    let cli_script = package_root.join("dist/cli.js");
    if !package_root.join("node_modules").is_dir() || !cli_script.is_file() {
        eprintln!(
            "skipping live Node.js test: run `pnpm install && pnpm run build` in {} first",
            package_root.display()
        );
        return None;
    }

    Some((node, cli_script, package_root))
}

fn react_package_root() -> PathBuf {
    // Not `canonicalize()`: on Windows it returns a `\\?\` verbatim path, and
    // the CLI then imports `@celesta/react` as a second module instance whose
    // React contexts the entry's hooks cannot see.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/react-bridge lives two levels below the workspace root")
        .join("packages/react")
}

fn find_executable(environment: &str, command: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(environment).map(PathBuf::from)
        && executable_works(&path)
    {
        return Some(path);
    }
    let command = PathBuf::from(command);
    executable_works(&command).then_some(command)
}

fn executable_works(path: &Path) -> bool {
    Command::new(path)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}
