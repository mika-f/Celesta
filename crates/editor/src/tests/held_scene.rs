//! Tests of the preview worker's held scene and its live text override
//! (`preview`).

use super::*;
use crate::preview::{
    HeldKey, PreviewRequest, SceneSource, TextOverride, replace_text, scene_source, shift_runs,
};
use celesta_composition::{TextColorRun, TextFontRun, TextStyle};
use celesta_gpu_renderer::RenderQuality;

fn text_layer(id: &str, text: &str, style: TextStyle) -> Layer {
    Layer {
        content: LayerContent::Text {
            text: text.to_owned(),
            style: Box::new(style),
            max_width: None,
            baseline_anchor: false,
        },
        ..missing(id)
    }
}

fn scene_of(layers: Vec<Layer>) -> Scene {
    Scene {
        width: 16,
        height: 16,
        frame_rate: Rational::new(30, 1),
        time: Time::new(0, 30),
        fonts: Vec::new(),
        shaders: Vec::new(),
        layers,
    }
}

fn edit(layer: &str, original: &str, text: &str) -> TextOverride {
    TextOverride {
        layer: layer.to_owned(),
        original: original.to_owned(),
        text: text.to_owned(),
    }
}

/// The text and style of text layer `layer` (`layers[index]`).
fn text_at(layers: &[Layer], index: usize) -> (&str, &TextStyle) {
    let LayerContent::Text { text, style, .. } = &layers[index].content else {
        panic!("layer {index} is not a text");
    };
    (text, style)
}

/// `runs`, as `(start, end)`, moved from `original` onto `edited`.
fn shifted(runs: &[(usize, usize)], original: &str, edited: &str) -> Vec<(usize, usize)> {
    shift_runs(
        runs,
        |&(start, end)| start..end,
        |_, span| (span.start, span.end),
        original,
        edited,
    )
}

#[test]
fn replace_text_reaches_a_layer_inside_a_group_and_a_mask() {
    let mut scene = scene_of(vec![masked_group(
        vec![text_layer("child", "Sale", TextStyle::default())],
        vec![text_layer("matte", "Mask", TextStyle::default())],
    )]);

    assert!(replace_text(&mut scene, &edit("child", "Sale", "Sold")));
    assert!(replace_text(&mut scene, &edit("matte", "Mask", "Matte")));

    let LayerContent::Group {
        layers,
        mask: Some(mask),
        ..
    } = &scene.layers[0].content
    else {
        panic!("the group lost its mask");
    };
    assert_eq!(text_at(layers, 0).0, "Sold");
    assert_eq!(text_at(&mask.layers, 0).0, "Matte");
}

#[test]
fn replace_text_clears_visible_characters() {
    let mut scene = scene_of(vec![text_layer(
        "title",
        "Hello",
        TextStyle {
            visible_characters: Some(2),
            ..TextStyle::default()
        },
    )]);

    assert!(replace_text(&mut scene, &edit("title", "Hello", "Hello!")));

    let (text, style) = text_at(&scene.layers, 0);
    assert_eq!(text, "Hello!");
    assert_eq!(style.visible_characters, None);
}

#[test]
fn replace_text_carries_the_color_and_font_runs() {
    let mut scene = scene_of(vec![text_layer(
        "title",
        "abcdef",
        TextStyle {
            color_runs: vec![TextColorRun {
                start: 2,
                end: 4,
                color: "#ff0000".to_owned(),
            }],
            font_runs: vec![TextFontRun {
                start: 0,
                end: 2,
                font_weight: Some(700),
                ..TextFontRun::default()
            }],
            ..TextStyle::default()
        },
    )]);

    // Typed inside the colored "cd".
    assert!(replace_text(
        &mut scene,
        &edit("title", "abcdef", "abcXdef")
    ));

    let (_, style) = text_at(&scene.layers, 0);
    assert_eq!(
        style.color_runs,
        [TextColorRun {
            start: 2,
            end: 5,
            color: "#ff0000".to_owned(),
        }]
    );
    assert_eq!(
        style.font_runs,
        [TextFontRun {
            start: 0,
            end: 2,
            font_weight: Some(700),
            ..TextFontRun::default()
        }]
    );
}

#[test]
fn shift_runs_keeps_the_prefix_and_shifts_the_suffix() {
    // The colored "cd" of "abcdef".
    let colored = [(2, 4)];
    // Inside the run: it stretches over the typed text.
    assert_eq!(shifted(&colored, "abcdef", "abcXdef"), [(2, 5)]);
    // Before it and after it: the run keeps its own characters.
    assert_eq!(shifted(&colored, "abcdef", "abXcdef"), [(3, 5)]);
    assert_eq!(shifted(&colored, "abcdef", "abcdXef"), [(2, 4)]);
    // Deleting "de", across the run's end, cuts it to "c".
    assert_eq!(shifted(&colored, "abcdef", "abcf"), [(2, 3)]);
    // Deleting all of it drops it.
    assert!(shifted(&colored, "abcdef", "af").is_empty());
    // Typing over exactly the run keeps it on the new text.
    assert_eq!(shifted(&colored, "abcdef", "abXYZef"), [(2, 5)]);
    // Runs inside a replaced span are dropped, the ones beside it kept.
    assert_eq!(
        shifted(&[(0, 2), (2, 3), (3, 4), (4, 6)], "abcdef", "abXYef"),
        [(0, 2), (4, 6)]
    );
    // Indices count code points, not bytes.
    let kana = [(1, 3)];
    assert_eq!(shifted(&kana, "あいうえお", "Xあいうえお"), [(2, 4)]);
    assert_eq!(shifted(&kana, "あいうえお", "あいう〜えお"), [(1, 3)]);
}

#[test]
fn replace_text_refuses_a_missing_or_non_text_layer() {
    let mut scene = scene_of(vec![
        missing("Badge"),
        text_layer("title", "Hello", TextStyle::default()),
    ]);
    let before = scene.clone();

    assert!(!replace_text(&mut scene, &edit("Badge", "Hello", "Bye")));
    assert!(!replace_text(&mut scene, &edit("absent", "Hello", "Bye")));
    assert_eq!(scene, before);
}

fn request(
    time: Time,
    react_reload: u64,
    react: Option<ReactPreviewContext>,
    reuse_scene: bool,
) -> PreviewRequest {
    PreviewRequest {
        generation: 1,
        scene: Scene {
            time,
            ..scene_of(Vec::new())
        },
        asset_root: PathBuf::from("."),
        react,
        react_mode: ReactPreviewMode::WholeScene,
        react_reload,
        render_quality: RenderQuality::Final,
        source_of: None,
        prop_overrides: Vec::new(),
        frame_of: None,
        space_of: None,
        added_under: None,
        timeline_frame: 0,
        text_override: Some(edit("title", "Hello", "Hello!")),
        reuse_scene,
    }
}

#[test]
fn a_reused_request_without_a_held_scene_evaluates() {
    let keystroke = request(Time::new(0, 30), 0, Some(react_context(true)), true);

    assert_eq!(scene_source(&keystroke, None), SceneSource::Evaluate);
}

#[test]
fn a_reused_request_for_another_frame_or_reload_evaluates() {
    let evaluated = request(Time::new(0, 30), 0, Some(react_context(true)), false);
    let held = HeldKey::of(&evaluated);

    let keystroke = request(Time::new(0, 30), 0, Some(react_context(true)), true);
    assert_eq!(scene_source(&keystroke, Some(&held)), SceneSource::Held);
    // Only a keystroke's request reuses the held scene.
    assert_eq!(scene_source(&evaluated, Some(&held)), SceneSource::Evaluate);

    let seeked = request(Time::new(1, 30), 0, Some(react_context(true)), true);
    assert_eq!(scene_source(&seeked, Some(&held)), SceneSource::Evaluate);
    let reloaded = request(Time::new(0, 30), 1, Some(react_context(true)), true);
    assert_eq!(scene_source(&reloaded, Some(&held)), SceneSource::Evaluate);
    let respawned = request(Time::new(0, 30), 0, Some(react_context(false)), true);
    assert_eq!(scene_source(&respawned, Some(&held)), SceneSource::Evaluate);
    let without_runtime = request(Time::new(0, 30), 0, None, true);
    assert_eq!(
        scene_source(&without_runtime, Some(&held)),
        SceneSource::Evaluate
    );
}
