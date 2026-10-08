# Group Alpha / Luminance Masks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a `Group` show its children only where another drawn subtree (its mask) is drawn, by alpha or luminance, optionally inverted, in the CPU and GPU renderers and in `@celesta/react`.

**Architecture:** `LayerContent::Group` gains `mask: Option<GroupMask>`. Both renderers draw the mask subtree and the children onto separate transparent canvases, multiply the children by the mask value per pixel, and send the result through the existing isolated-group path (effects, then opacity and blend mode). React exposes `<Mask>` both as a child of `<Group>` and as `<Group mask={…}>`; both become one `'mask'` host child that the walker turns into `mask.layers`.

**Tech Stack:** Rust 2024 (serde, ts-rs 12, wgpu 30, WGSL), TypeScript (React 18 reconciler), vitest, pnpm.

**Spec:** `docs/superpowers/specs/2026-10-09-group-mask-design.md`

## Global Constraints

- The existing rectangle `clip` keeps its behavior and serialization.
- JSON: `mask` is omitted when `None`, `mode` when `alpha`, `invert` when false.
- Rust type names: `GroupMask` and `MaskMode` (not `Mask`, which is the React component).
- Luminance: `(0.2126 R + 0.7152 G + 0.0722 B) × A` on sRGB-encoded values, no linearization; `invert` gives `1 - m`.
- Order: children (isolated, clipped) → × mask → effects → opacity and blend mode onto the parent.
- Mask layers: group coordinate space, opacity 1, normal blending, no clip (neither the group's nor an ancestor's), never shown.
- Audio inside a mask plays as anywhere else.
- `<Mask>` only directly under `<Group>` (in the host tree); at most one mask per group.
- Scenes without masks take exactly the code paths they take today.
- Delivery: two PRs. PR 1 is Tasks 1–5, PR 2 is Tasks 6–11.
- Edit files with the Edit tool, not `sed`/`perl`/`python` rewrites.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Turning a mask on and off between frames** (`mask={on && <Circle/>}`) must keep the children's React state and tree-path ids. The ids matter because export keeps one video decode session per layer id. Pinned in Task 6.
2. **A blended layer inside the mask subtree** (for example `screen`) blends against the mask canvas on the GPU. That path copies a backdrop from the mask canvas, not the scene. Pinned in Task 4's parity cases.
3. **A masked group partly off the scene edge.** Canvas regions are clamped to the scene, and both canvases must use the same clamped region. Pinned in Task 4's parity cases.
4. **Effects on a masked group.** The shadow must follow the cut-out shape, and its canvas must reach past the mask region. Pinned in Task 3 (CPU) and Task 4 (GPU parity).
5. **The same video both in the mask and in the children** must get two distinct layer ids. Pinned in Task 6.

---

## PR 1: scene model and renderers

### Task 1: Scene model (`GroupMask`, `MaskMode`)

**Files:**
- Modify: `crates/composition/src/model.rs` (`LayerContent::Group`, `LayerContentDef::Group`, new types after `Clip`)
- Modify: `crates/composition/src/lib.rs:20-24` (re-exports)
- Test: `crates/composition/src/tests.rs`
- Modify (add `mask: None` to every `LayerContent::Group { … }` literal the compiler reports): `crates/evaluator/src/lib.rs:339`, `crates/editor/src/preview.rs:388`, `crates/bench/src/workloads.rs`, `crates/bench/examples/code-runs.rs`, `crates/gpu-renderer/examples/dense-geometry-bench.rs`, `crates/gpu-renderer/src/tests.rs`, `crates/renderer/src/tests.rs`, `crates/react-bridge/tests/node_integration.rs`, and any other site `cargo check` lists.

**Interfaces:**
- Produces: `celesta_composition::{GroupMask, MaskMode}`.
  - `pub struct GroupMask { pub layers: Vec<Layer>, pub mode: MaskMode, pub invert: bool }`
  - `pub enum MaskMode { Alpha, Luminance }`, `Copy`, default `Alpha`.
  - `LayerContent::Group { layers: Vec<Layer>, clip: Option<Clip>, mask: Option<GroupMask> }`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/composition/src/tests.rs`, and add `GroupMask, MaskMode` to the `use crate::{…}` list at the top:

```rust
#[test]
fn a_group_without_a_mask_keeps_its_json() {
    let content = LayerContent::Group {
        layers: Vec::new(),
        clip: None,
        mask: None,
    };
    let json = serde_json::to_value(&content).unwrap();
    assert_eq!(json, serde_json::json!({ "type": "group", "layers": [] }));
}

#[test]
fn a_groups_mask_round_trips_and_omits_its_defaults() {
    let json = serde_json::json!({
        "type": "group",
        "layers": [],
        "mask": { "layers": [] },
    });
    let content = serde_json::from_value::<LayerContent>(json.clone()).unwrap();
    let LayerContent::Group {
        mask: Some(mask), ..
    } = &content
    else {
        panic!("the mask was dropped: {content:?}");
    };
    assert_eq!(mask.mode, MaskMode::Alpha);
    assert!(!mask.invert);
    assert_eq!(serde_json::to_value(&content).unwrap(), json);

    let luminance = LayerContent::Group {
        layers: Vec::new(),
        clip: None,
        mask: Some(GroupMask {
            layers: Vec::new(),
            mode: MaskMode::Luminance,
            invert: true,
        }),
    };
    let json = serde_json::to_value(&luminance).unwrap();
    assert_eq!(
        json["mask"],
        serde_json::json!({ "layers": [], "mode": "luminance", "invert": true })
    );
    assert_eq!(serde_json::from_value::<LayerContent>(json).unwrap(), luminance);
}
```

Also add `mask: None` to the two existing `LayerContent::Group { layers: Vec::new(), clip: … }` literals in this file (`a_group_without_a_clip_serializes_and_parses_as_before`, `a_groups_clip_round_trips_and_defaults_its_corner_radius`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p celesta-composition`
Expected: compile errors `cannot find type GroupMask` and `struct LayerContent::Group has no field named mask`.

- [ ] **Step 3: Add the types and the field**

In `crates/composition/src/model.rs`, in `pub enum LayerContent`, replace the `Group` variant with:

```rust
    Group {
        layers: Vec<Layer>,
        /// Draws the children only inside this region, in the group's own
        /// coordinate space (the space the children's positions are given
        /// in), so it moves, scales, and rotates with the group.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        clip: Option<Clip>,
        /// Shows the children only where this subtree is drawn; see
        /// `GroupMask`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mask: Option<GroupMask>,
    },
```

In `enum LayerContentDef`, replace its `Group` variant with:

```rust
    Group {
        layers: Vec<Layer>,
        #[serde(default)]
        clip: Option<Clip>,
        #[serde(default)]
        mask: Option<GroupMask>,
    },
```

After `impl Clip { … }`, add:

```rust
/// Another drawn subtree that decides, per pixel, how much of a group shows.
/// Its layers are positioned in the group's own coordinate space, like the
/// group's children, and drawn at opacity 1 with normal blending and no clip;
/// they are never shown themselves. Where they draw nothing, nothing of the
/// group shows (everything does when inverted).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GroupMask {
    pub layers: Vec<Layer>,
    #[serde(default, skip_serializing_if = "MaskMode::is_alpha")]
    pub mode: MaskMode,
    /// Shows the group where the mask is not drawn instead: `1 - m`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invert: bool,
}

/// Which value of a mask pixel shows the group.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "codegen", derive(ts_rs::TS))]
#[cfg_attr(feature = "codegen", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum MaskMode {
    /// The mask's alpha.
    #[default]
    Alpha,
    /// The Rec. 709 luma of the mask's sRGB-encoded color times its alpha:
    /// `(0.2126 R + 0.7152 G + 0.0722 B) × A`, with no linearization.
    Luminance,
}

impl MaskMode {
    pub fn is_alpha(&self) -> bool {
        *self == Self::Alpha
    }
}
```

In `crates/composition/src/lib.rs`, add `GroupMask` and `MaskMode` to the `pub use model::{…}` list, keeping it sorted:

```rust
pub use model::{
    AssetLocation, AudioClip, AudioGraph, BlendMode, Clip, DEFAULT_MITER_LIMIT, EvaluatedTransform,
    GroupMask, ImageFit, Layer, LayerContent, LayerEffects, LayerGlow, LayerShadow, LineCap,
    LineJoin, MaskMode, MediaTiming, PathCommand, Point, ResolvedAsset, Scene,
};
```

- [ ] **Step 4: Run the composition tests**

Run: `cargo test -p celesta-composition`
Expected: PASS, including both new tests.

- [ ] **Step 5: Fix every other `Group` literal in the workspace**

Run: `cargo check --workspace --all-targets 2>&1 | grep -B2 -A8 "missing field \`mask\`"`

Each error names a `LayerContent::Group { layers, clip … }` literal. Add `mask: None` to each one, with the Edit tool. For example, `crates/evaluator/src/lib.rs:339` becomes:

```rust
        Ok(LayerContent::Group {
            layers,
            clip: None,
            mask: None,
        })
```

and `crates/editor/src/preview.rs:388` becomes:

```rust
                        layers[index].content = LayerContent::Group {
                            layers: children,
                            clip: None,
                            mask: None,
                        };
```

Patterns ending in `..` (`LayerContent::Group { layers, .. }`) still compile; leave them for Task 2. Repeat until `cargo check --workspace --all-targets` reports no errors.

- [ ] **Step 6: Check the TypeScript bindings generate**

Run: `cargo test -p celesta-composition --features codegen`
Expected: PASS. `packages/react/src/generated/GroupMask.ts` and `MaskMode.ts` exist. In `LayerContent.ts`, `mask?: GroupMask` is optional; in `GroupMask.ts`, `mode?` and `invert?` are optional, as `Layer.ts`'s `blendMode?` is. (`generated/` is gitignored; do not commit it.)

- [ ] **Step 7: Run the workspace tests and commit**

Run: `cargo test --workspace`
Expected: PASS.

```bash
git add -A crates
git commit -m "feat(composition): add GroupMask to groups (#146)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Traversals that must see mask layers

**Files:**
- Modify: `crates/exporter/src/project.rs:140-151` (`absolutize_layer_content`)
- Modify: `crates/editor/src/preview.rs:328-400` (`collect_component_requests`, `strip_missing_components`, `splice_resolved_components`)
- Modify: `crates/bench/src/workloads.rs:205` (`scale_effects`)
- Test: `crates/exporter/src/tests.rs`, `crates/editor/src/tests.rs`

**Interfaces:**
- Consumes: `GroupMask` (Task 1).
- Produces: no new API. These traversals visit `mask.layers` after `layers`, in that order.

- [ ] **Step 1: Write the failing tests**

Append to `crates/exporter/src/tests.rs`. Add `use crate::project::absolutize_layers;`, and add `EvaluatedTransform, GroupMask, Layer, MaskMode, ResolvedAsset` to the `celesta_composition` import.

```rust
#[test]
fn absolutizes_assets_inside_a_groups_mask() {
    let image = |path: &str| Layer {
        id: path.to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: Default::default(),
        effects: Default::default(),
        content: LayerContent::Image {
            asset: ResolvedAsset {
                id: path.to_owned(),
                location: AssetLocation::File {
                    path: path.to_owned(),
                },
            },
            width: None,
            height: None,
            fit: None,
        },
    };
    let mut layers = vec![Layer {
        content: LayerContent::Group {
            layers: vec![image("child.png")],
            clip: None,
            mask: Some(GroupMask {
                layers: vec![image("matte.png")],
                mode: MaskMode::Alpha,
                invert: false,
            }),
        },
        ..image("group")
    }];
    let root = Path::new("/project");
    absolutize_layers(&mut layers, root);
    let LayerContent::Group {
        layers: children,
        mask: Some(mask),
        ..
    } = &layers[0].content
    else {
        panic!("the group lost its mask");
    };
    for (layer, name) in [(&children[0], "child.png"), (&mask.layers[0], "matte.png")] {
        let LayerContent::Image { asset, .. } = &layer.content else {
            panic!("not an image");
        };
        assert_eq!(
            asset.location,
            AssetLocation::File {
                path: root.join(name).to_string_lossy().into_owned()
            }
        );
    }
}
```

Append to `crates/editor/src/tests.rs`. Add `use crate::preview::{collect_component_requests, strip_missing_components};`, and import `EvaluatedTransform, GroupMask, Layer, LayerContent, MaskMode` from `celesta_composition`.

```rust
fn missing(name: &str) -> Layer {
    Layer {
        id: name.to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: Default::default(),
        effects: Default::default(),
        content: LayerContent::MissingComponent {
            component: name.to_owned(),
            props: Default::default(),
        },
    }
}

fn masked_group(children: Vec<Layer>, mask: Vec<Layer>) -> Layer {
    Layer {
        content: LayerContent::Group {
            layers: children,
            clip: None,
            mask: Some(GroupMask {
                layers: mask,
                mode: MaskMode::Alpha,
                invert: false,
            }),
        },
        ..missing("group")
    }
}

#[test]
fn component_requests_reach_into_masks_after_the_children() {
    let layers = vec![masked_group(vec![missing("Child")], vec![missing("Matte")])];
    let mut requests = Vec::new();
    collect_component_requests(&layers, &mut requests);
    let names: Vec<_> = requests.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["Child", "Matte"]);
}

#[test]
fn strips_missing_components_inside_masks() {
    let mut layers = vec![masked_group(vec![missing("Child")], vec![missing("Matte")])];
    strip_missing_components(&mut layers);
    let LayerContent::Group {
        layers: children,
        mask: Some(mask),
        ..
    } = &layers[0].content
    else {
        panic!("the group lost its mask");
    };
    assert!(children.is_empty());
    assert!(mask.layers.is_empty());
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-exporter absolutizes_assets_inside_a_groups_mask`, then `cargo test -p celesta-editor component_requests_reach_into_masks strips_missing_components_inside_masks`
Expected: FAIL. The mask's asset path stays relative, the request list is `["Child"]`, and the mask keeps its layer.

- [ ] **Step 3: Recurse into masks, naming `mask` instead of `..`**

`crates/exporter/src/project.rs`, in `absolutize_layer_content`:

```rust
        LayerContent::Group {
            layers,
            clip: _,
            mask,
        } => {
            absolutize_layers(layers, asset_root);
            if let Some(mask) = mask {
                absolutize_layers(&mut mask.layers, asset_root);
            }
        }
```

`crates/editor/src/preview.rs`, `collect_component_requests`:

```rust
            LayerContent::Group {
                layers,
                clip: _,
                mask,
            } => {
                collect_component_requests(layers, out);
                if let Some(mask) = mask {
                    collect_component_requests(&mask.layers, out);
                }
            }
```

`strip_missing_components`:

```rust
            LayerContent::Group {
                layers: children,
                clip: _,
                mask,
            } => {
                strip_missing_components(children);
                if let Some(mask) = mask {
                    strip_missing_components(&mut mask.layers);
                }
            }
```

`splice_resolved_components`, in the inner `if let`:

```rust
                if let LayerContent::Group {
                    layers: children,
                    clip: _,
                    mask,
                } = &mut layers[index].content
                {
                    splice_resolved_components(children, resolutions, cursor, unresolved);
                    if let Some(mask) = mask {
                        splice_resolved_components(&mut mask.layers, resolutions, cursor, unresolved);
                    }
                }
```

`crates/bench/src/workloads.rs`, in `scale_effects`:

```rust
            if let LayerContent::Group {
                layers,
                clip: _,
                mask,
            } = &mut layer.content
            {
                scale_effects(layers, k);
                if let Some(mask) = mask {
                    scale_effects(&mut mask.layers, k);
                }
            }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p celesta-exporter -p celesta-editor -p celesta-bench`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/exporter crates/editor crates/bench
git commit -m "fix: walk mask layers wherever group children are walked (#146)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: CPU reference renderer

**Files:**
- Modify: `crates/renderer/src/layer.rs:66-109` (the `Group` arm)
- Modify: `crates/renderer/src/composite.rs` (add `mask_value`)
- Create: `crates/renderer/src/mask_tests.rs`
- Modify: `crates/renderer/src/lib.rs` (add `#[cfg(test)] mod mask_tests;`)

**Interfaces:**
- Consumes: `GroupMask`, `MaskMode` (Task 1).
- Produces: `pub(crate) fn mask_value(mode: MaskMode, invert: bool, matte: &[u8]) -> f64` in `composite.rs`; `CpuRenderer` renders masked groups. Task 4's parity tests compare against this.

- [ ] **Step 1: Write the failing tests**

Create `crates/renderer/src/mask_tests.rs`:

```rust
use crate::renderer::CpuRenderer;
use crate::types::{RenderOptions, RgbaFrame};
use celesta_composition::{
    BlendMode, Clip, DEFAULT_MITER_LIMIT, EvaluatedTransform, GroupMask, Layer, LayerContent,
    LayerShadow, LineCap, LineJoin, MaskMode, Paint, PathCommand, Point, Rational, Scene, Time,
};

fn scene(layers: Vec<Layer>) -> Scene {
    Scene {
        width: 40,
        height: 40,
        frame_rate: Rational::new(30, 1),
        time: Time::ZERO,
        fonts: Vec::new(),
        layers,
    }
}

/// A rect with its top-left corner at `(x, y)` in its parent.
fn rect(x: f64, y: f64, width: f64, height: f64, color: &str) -> Layer {
    Layer {
        id: format!("rect-{x}-{y}"),
        transform: EvaluatedTransform {
            position: Point { x, y },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Rect {
            width,
            height,
            fill: Some(Paint::Solid {
                color: color.to_owned(),
            }),
            stroke: None,
            corner_radius: 0.0,
        },
    }
}

fn red_square() -> Layer {
    rect(0.0, 0.0, 40.0, 40.0, "#FF0000FF")
}

fn mask(layers: Vec<Layer>) -> GroupMask {
    GroupMask {
        layers,
        mode: MaskMode::Alpha,
        invert: false,
    }
}

fn masked(transform: EvaluatedTransform, mask: GroupMask, layers: Vec<Layer>) -> Layer {
    Layer {
        id: "masked".to_owned(),
        transform,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Group {
            layers,
            clip: None,
            mask: Some(mask),
        },
    }
}

fn render(layers: Vec<Layer>) -> RgbaFrame {
    CpuRenderer::default().render(&scene(layers)).unwrap()
}

fn pixel(frame: &RgbaFrame, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * frame.width() + x) * 4) as usize;
    frame.pixels()[offset..offset + 4].try_into().unwrap()
}

const RED: [u8; 4] = [255, 0, 0, 255];

fn background() -> [u8; 4] {
    let color = RenderOptions::default().background;
    [color.red, color.green, color.blue, color.alpha]
}

fn assert_close(actual: [u8; 4], expected: [u8; 4]) {
    let close = actual
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= 1);
    assert!(close, "{actual:?} is not within 1 of {expected:?}");
}

#[test]
fn an_alpha_mask_shows_the_children_only_where_it_is_drawn() {
    let frame = render(vec![masked(
        EvaluatedTransform::default(),
        mask(vec![rect(10.0, 10.0, 20.0, 20.0, "#FFFFFFFF")]),
        vec![red_square()],
    )]);
    assert_eq!(pixel(&frame, 20, 20), RED);
    assert_eq!(pixel(&frame, 10, 10), RED);
    for (x, y) in [(9, 20), (30, 20), (20, 9), (20, 30), (0, 0)] {
        assert_eq!(pixel(&frame, x, y), background(), "({x}, {y}) leaked");
    }
}

#[test]
fn a_path_masks_a_group() {
    // A diamond touching the middle of each edge of the scene.
    let diamond = Layer {
        id: "diamond".to_owned(),
        transform: EvaluatedTransform::default(),
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Path {
            commands: vec![
                PathCommand::MoveTo { x: 20.0, y: 0.0 },
                PathCommand::LineTo { x: 40.0, y: 20.0 },
                PathCommand::LineTo { x: 20.0, y: 40.0 },
                PathCommand::LineTo { x: 0.0, y: 20.0 },
                PathCommand::Close,
            ],
            fill: Some(Paint::Solid {
                color: "#FFFFFFFF".to_owned(),
            }),
            stroke: None,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: DEFAULT_MITER_LIMIT,
        },
    };
    let frame = render(vec![masked(
        EvaluatedTransform::default(),
        mask(vec![diamond]),
        vec![red_square()],
    )]);
    assert_eq!(pixel(&frame, 20, 20), RED);
    assert_eq!(pixel(&frame, 2, 2), background());
    assert_eq!(pixel(&frame, 37, 37), background());
}

#[test]
fn a_luminance_mask_shows_the_children_by_the_masks_luma() {
    // Gray 0x80 has luma 128 / 255, so half of the red shows over the
    // background (20, 22, 28).
    let frame = render(vec![masked(
        EvaluatedTransform::default(),
        GroupMask {
            mode: MaskMode::Luminance,
            ..mask(vec![rect(0.0, 0.0, 40.0, 40.0, "#808080FF")])
        },
        vec![red_square()],
    )]);
    assert_close(pixel(&frame, 20, 20), [138, 11, 14, 255]);
}

#[test]
fn an_inverted_mask_shows_the_children_where_it_is_not_drawn() {
    let frame = render(vec![masked(
        EvaluatedTransform::default(),
        GroupMask {
            invert: true,
            ..mask(vec![rect(10.0, 10.0, 20.0, 20.0, "#FFFFFFFF")])
        },
        vec![red_square()],
    )]);
    assert_eq!(pixel(&frame, 20, 20), background());
    assert_eq!(pixel(&frame, 5, 5), RED);
}

#[test]
fn an_empty_mask_hides_the_group_unless_inverted() {
    let hidden = render(vec![masked(
        EvaluatedTransform::default(),
        mask(Vec::new()),
        vec![red_square()],
    )]);
    assert_eq!(pixel(&hidden, 20, 20), background());
    let shown = render(vec![masked(
        EvaluatedTransform::default(),
        GroupMask {
            invert: true,
            ..mask(Vec::new())
        },
        vec![red_square()],
    )]);
    assert_eq!(pixel(&shown, 20, 20), RED);
}

#[test]
fn a_mask_follows_the_groups_position_and_scale() {
    // The mask's 5x5 rect at the group's origin covers canvas 10..20.
    let frame = render(vec![masked(
        EvaluatedTransform {
            position: Point { x: 10.0, y: 10.0 },
            scale: Point { x: 2.0, y: 2.0 },
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        mask(vec![rect(0.0, 0.0, 5.0, 5.0, "#FFFFFFFF")]),
        vec![rect(-10.0, -10.0, 40.0, 40.0, "#FF0000FF")],
    )]);
    assert_eq!(pixel(&frame, 15, 15), RED);
    assert_eq!(pixel(&frame, 25, 15), background());
    assert_eq!(pixel(&frame, 5, 15), background());
}

#[test]
fn a_clip_and_a_mask_both_limit_the_children() {
    let mut layer = masked(
        EvaluatedTransform::default(),
        mask(vec![rect(5.0, 0.0, 20.0, 40.0, "#FFFFFFFF")]),
        vec![red_square()],
    );
    if let LayerContent::Group { clip, .. } = &mut layer.content {
        *clip = Some(Clip {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 40.0,
            corner_radius: 0.0,
        });
    }
    let frame = render(vec![layer]);
    assert_eq!(pixel(&frame, 7, 20), RED);
    assert_eq!(pixel(&frame, 3, 20), background());
    assert_eq!(pixel(&frame, 12, 20), background());
}

#[test]
fn a_masked_group_is_isolated() {
    // Multiplied with the dark background the blue would darken; isolated,
    // it multiplies with nothing and shows as itself.
    let frame = render(vec![masked(
        EvaluatedTransform::default(),
        mask(vec![rect(0.0, 0.0, 40.0, 40.0, "#FFFFFFFF")]),
        vec![Layer {
            blend_mode: BlendMode::Multiply,
            ..rect(0.0, 0.0, 40.0, 40.0, "#0000FFFF")
        }],
    )]);
    assert_eq!(pixel(&frame, 20, 20), [0, 0, 255, 255]);
}

#[test]
fn a_shadow_follows_the_cut_out_shape() {
    // The mask leaves x 0..10 of the red; a hard shadow 10 px to the right
    // covers x 10..20, not the uncut red's 10..50.
    let mut layer = masked(
        EvaluatedTransform::default(),
        mask(vec![rect(0.0, 0.0, 10.0, 40.0, "#FFFFFFFF")]),
        vec![red_square()],
    );
    layer.effects.shadow = Some(LayerShadow {
        color: "#000000FF".to_owned(),
        blur: 0.0,
        offset_x: 10.0,
        offset_y: 0.0,
    });
    let frame = render(vec![layer]);
    assert_eq!(pixel(&frame, 5, 20), RED);
    assert_eq!(pixel(&frame, 15, 20), [0, 0, 0, 255]);
    assert_eq!(pixel(&frame, 30, 20), background());
}

#[test]
fn nested_masks_intersect() {
    let inner = masked(
        EvaluatedTransform::default(),
        mask(vec![rect(10.0, 0.0, 30.0, 40.0, "#FFFFFFFF")]),
        vec![red_square()],
    );
    let frame = render(vec![masked(
        EvaluatedTransform::default(),
        mask(vec![rect(0.0, 0.0, 20.0, 40.0, "#FFFFFFFF")]),
        vec![inner],
    )]);
    assert_eq!(pixel(&frame, 15, 20), RED);
    assert_eq!(pixel(&frame, 5, 20), background());
    assert_eq!(pixel(&frame, 25, 20), background());
}

#[test]
fn group_opacity_applies_once_and_a_mask_layers_opacity_counts() {
    // 0.5 x 0.5 of the red over the background: 255 * 0.25 + 20 * 0.75.
    let mut layer = masked(
        EvaluatedTransform::default(),
        mask(vec![Layer {
            opacity: 0.5,
            ..rect(0.0, 0.0, 40.0, 40.0, "#FFFFFFFF")
        }]),
        vec![red_square()],
    );
    layer.opacity = 0.5;
    let frame = render(vec![layer]);
    assert_close(pixel(&frame, 20, 20), [79, 17, 21, 255]);
}
```

In `crates/renderer/src/lib.rs`, after `mod linebreak;`, add:

```rust
#[cfg(test)]
mod mask_tests;
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-renderer mask_tests`
Expected: FAIL. The children draw unmasked, so for example `an_alpha_mask_shows_the_children_only_where_it_is_drawn` finds red at `(0, 0)`.

- [ ] **Step 3: Add `mask_value`**

In `crates/renderer/src/composite.rs`, change the import to `use celesta_composition::{BlendMode, MaskMode};` and add after `blend`:

```rust
/// How much of a masked group shows through the straight-alpha mask pixel
/// `matte`, 0 to 1: its alpha, or the Rec. 709 luma of its sRGB-encoded
/// color times its alpha; `1 - m` when inverted.
pub(crate) fn mask_value(mode: MaskMode, invert: bool, matte: &[u8]) -> f64 {
    let alpha = f64::from(matte[3]) / 255.0;
    let value = match mode {
        MaskMode::Alpha => alpha,
        MaskMode::Luminance => {
            let [red, green, blue] = [matte[0], matte[1], matte[2]].map(|c| f64::from(c) / 255.0);
            (0.2126 * red + 0.7152 * green + 0.0722 * blue) * alpha
        }
    };
    if invert { 1.0 - value } else { value }
}
```

- [ ] **Step 4: Render masked groups**

In `crates/renderer/src/layer.rs`, change the import to `use crate::composite::{blend, blend_with_mode, mask_value};` and add `GroupMask` to the `celesta_composition` import. Replace the start of the `Group` arm, up to the `if !layer.blend_mode.is_normal()` line, with:

```rust
            LayerContent::Group { layers, clip, mask } => {
                let mut child_state = state.clone();
                if let Some(clip) = clip {
                    if clip.is_empty() {
                        return Ok(());
                    }
                    child_state.clip = Some(Arc::new(ClipNode {
                        region: ClipRegion::new(clip, state.position, state.scale),
                        parent: state.clip.clone(),
                    }));
                }
                if let Some(mask) = mask {
                    return self.render_masked_group(frame, layers, mask, &state, child_state);
                }
```

The rest of the arm stays as it is. Add this method to the `impl CpuRenderer` block, after `render_effect_layer`:

```rust
    /// Draws a group with a mask: the children and the mask each onto a
    /// transparent frame of their own, the children then shown through the
    /// mask onto `frame` with the group's opacity and blend mode. The
    /// children carry the clips; the mask draws without any.
    fn render_masked_group(
        &mut self,
        frame: &mut RgbaFrame,
        layers: &[Layer],
        mask: &GroupMask,
        state: &ParentState,
        child_state: ParentState,
    ) -> Result<(), RenderError> {
        let blank = || RgbaFrame {
            width: frame.width,
            height: frame.height,
            pixels: vec![0; frame.pixels.len()],
        };
        let mut children = blank();
        let inner = ParentState {
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            ..child_state
        };
        for child in layers {
            self.render_layer(&mut children, child, inner.clone())?;
        }
        let mut matte = blank();
        let matte_state = ParentState {
            opacity: 1.0,
            clip: None,
            blend_mode: BlendMode::Normal,
            ..state.clone()
        };
        for layer in &mask.layers {
            self.render_layer(&mut matte, layer, matte_state.clone())?;
        }
        for ((destination, source), matte) in frame
            .pixels
            .chunks_exact_mut(4)
            .zip(children.pixels.chunks_exact(4))
            .zip(matte.pixels.chunks_exact(4))
        {
            let shown = mask_value(mask.mode, mask.invert, matte);
            if shown == 0.0 {
                continue;
            }
            let source = Color::rgba(source[0], source[1], source[2], source[3]);
            blend_with_mode(destination, source, state.opacity * shown, state.blend_mode);
        }
        Ok(())
    }
```

`render_effect_layer` already draws the layer without its effects and then filters the result, so effects apply after the mask with no other change.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p celesta-renderer`
Expected: PASS, including every test in `mask_tests`. If `group_opacity_applies_once_and_a_mask_layers_opacity_counts` is off by more than 1, print the pixel and recompute the expected value from `blend`. Do not widen `assert_close`.

- [ ] **Step 6: Commit**

```bash
git add crates/renderer
git commit -m "feat(renderer): draw groups through alpha and luminance masks (#146)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: GPU renderer

**Files:**
- Create: `crates/gpu-renderer/src/mask.rs`, `crates/gpu-renderer/src/mask.wgsl`
- Modify: `crates/gpu-renderer/src/lib.rs` (add `mod mask;`)
- Modify: `crates/gpu-renderer/src/bounds.rs` (`PixelBounds::intersection`)
- Modify: `crates/gpu-renderer/src/layer.rs:22-37` (`PreparedItem`)
- Modify: `crates/gpu-renderer/src/plan.rs` (`plan_groups`, `GpuStep`)
- Modify: `crates/gpu-renderer/src/prepare.rs:76-158` (instance count and step loop), `:376-411` (`Group` arm)
- Modify: `crates/gpu-renderer/src/compositor.rs` (`Compositor`, `draw_canvas`, `draw_group`, `group_end`)
- Modify: `crates/gpu-renderer/src/encode.rs:84-92` (`Compositor` construction)
- Modify: `crates/gpu-renderer/src/renderer.rs` (`masks` field)
- Test: `crates/gpu-renderer/src/tests.rs`

**Interfaces:**
- Consumes: `GroupMask`, `MaskMode` (Task 1); `CpuRenderer` masks (Task 3) as the reference.
- Produces:
  - `mask::MaskSpec { mode: MaskMode, invert: bool }` (`Copy`, `PartialEq`), `MaskSpec::new(&GroupMask)`.
  - `mask::MaskPipelines::new(&wgpu::Device, &wgpu::BindGroupLayout)`, `apply(&self, &mut wgpu::CommandEncoder, children: &CanvasTexture, matte: &CanvasTexture, target: &CanvasTexture, MaskSpec)`.
  - `PreparedItem::{BeginMask, MaskContent, EndMask(PreparedLayer, MaskSpec)}`.
  - `GpuStep::{BeginMask { canvas: CanvasRegion }, MaskContent, EndMask { instance: u32, blend_mode: BlendMode, mask: MaskSpec, area: Option<[u32; 4]> }}`.
  - `PixelBounds::intersection(Option<Self>, Option<Self>) -> Option<Self>`.

- [ ] **Step 1: Write the failing parity tests**

In `crates/gpu-renderer/src/tests.rs`, add `GroupMask, MaskMode, LayerShadow` to the `celesta_composition` import, and append:

```rust
fn alpha_mask(layers: Vec<Layer>) -> GroupMask {
    GroupMask {
        layers,
        mode: MaskMode::Alpha,
        invert: false,
    }
}

fn masked_group(transform: EvaluatedTransform, mask: GroupMask, layers: Vec<Layer>) -> Layer {
    Layer {
        id: "masked".to_owned(),
        transform,
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Group {
            layers,
            clip: None,
            mask: Some(mask),
        },
    }
}

/// A disc of `radius` centred on `(x, y)`, drawn as a fully rounded rect.
fn disc(id: &str, x: f64, y: f64, radius: f64, color: &str) -> Layer {
    let mut layer = corner_rect(id, x - radius, y - radius, radius * 2.0, radius * 2.0, color);
    if let LayerContent::Rect { corner_radius, .. } = &mut layer.content {
        *corner_radius = radius;
    }
    layer
}

#[test]
fn masks_groups_like_the_cpu_renderer() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let red = || corner_rect("red", 0.0, 0.0, 64.0, 48.0, "#FF0000FF");
    let cases: Vec<(&str, Vec<Layer>)> = vec![
        (
            "alpha disc",
            vec![masked_group(
                EvaluatedTransform::default(),
                alpha_mask(vec![disc("matte", 30.0, 24.0, 14.0, "#FFFFFFFF")]),
                vec![red()],
            )],
        ),
        (
            "luminance",
            vec![masked_group(
                EvaluatedTransform::default(),
                GroupMask {
                    mode: MaskMode::Luminance,
                    ..alpha_mask(vec![
                        corner_rect("gray", 0.0, 0.0, 64.0, 48.0, "#808080FF"),
                        disc("white", 30.0, 24.0, 10.0, "#FFFFFFFF"),
                    ])
                },
                vec![red()],
            )],
        ),
        (
            "inverted",
            vec![masked_group(
                EvaluatedTransform::default(),
                GroupMask {
                    invert: true,
                    ..alpha_mask(vec![disc("matte", 30.0, 24.0, 14.0, "#FFFFFFFF")])
                },
                vec![red()],
            )],
        ),
        (
            "empty",
            vec![masked_group(
                EvaluatedTransform::default(),
                alpha_mask(Vec::new()),
                vec![red()],
            )],
        ),
        (
            "empty inverted",
            vec![masked_group(
                EvaluatedTransform::default(),
                GroupMask {
                    invert: true,
                    ..alpha_mask(Vec::new())
                },
                vec![red()],
            )],
        ),
        (
            "translated and scaled",
            vec![masked_group(
                EvaluatedTransform {
                    position: Point { x: 6.0, y: 4.0 },
                    scale: Point { x: 2.0, y: 2.0 },
                    anchor: Point { x: 0.0, y: 0.0 },
                    ..EvaluatedTransform::default()
                },
                alpha_mask(vec![disc("matte", 10.0, 8.0, 6.0, "#FFFFFFFF")]),
                vec![corner_rect("red", -4.0, -4.0, 40.0, 30.0, "#FF0000FF")],
            )],
        ),
        (
            "with a clip",
            vec![{
                let mut group = masked_group(
                    EvaluatedTransform::default(),
                    alpha_mask(vec![disc("matte", 30.0, 24.0, 18.0, "#FFFFFFFF")]),
                    vec![red()],
                );
                if let LayerContent::Group { clip, .. } = &mut group.content {
                    *clip = Some(Clip {
                        x: 4.0,
                        y: 4.0,
                        width: 30.0,
                        height: 40.0,
                        corner_radius: 6.0,
                    });
                }
                group
            }],
        ),
        (
            "isolated multiply over a backdrop",
            vec![
                corner_rect("backdrop", 0.0, 0.0, 64.0, 48.0, "#80C0E0FF"),
                masked_group(
                    EvaluatedTransform::default(),
                    alpha_mask(vec![disc("matte", 30.0, 24.0, 14.0, "#FFFFFFFF")]),
                    vec![Layer {
                        blend_mode: BlendMode::Multiply,
                        ..corner_rect("blue", 0.0, 0.0, 64.0, 48.0, "#3060F0FF")
                    }],
                ),
            ],
        ),
        (
            "a blended layer inside the mask",
            vec![masked_group(
                EvaluatedTransform::default(),
                GroupMask {
                    mode: MaskMode::Luminance,
                    ..alpha_mask(vec![
                        corner_rect("dark", 0.0, 0.0, 64.0, 48.0, "#404040FF"),
                        Layer {
                            blend_mode: BlendMode::Screen,
                            ..disc("light", 30.0, 24.0, 14.0, "#808080FF")
                        },
                    ])
                },
                vec![red()],
            )],
        ),
        (
            "off the scene edge",
            vec![masked_group(
                EvaluatedTransform {
                    position: Point { x: 40.0, y: 30.0 },
                    anchor: Point { x: 0.0, y: 0.0 },
                    ..EvaluatedTransform::default()
                },
                alpha_mask(vec![disc("matte", 16.0, 12.0, 20.0, "#FFFFFFFF")]),
                vec![corner_rect("red", 0.0, 0.0, 60.0, 60.0, "#FF0000FF")],
            )],
        ),
        (
            "nested",
            vec![masked_group(
                EvaluatedTransform::default(),
                alpha_mask(vec![corner_rect("outer", 0.0, 0.0, 36.0, 48.0, "#FFFFFFFF")]),
                vec![masked_group(
                    EvaluatedTransform::default(),
                    alpha_mask(vec![disc("inner", 30.0, 24.0, 16.0, "#FFFFFFFF")]),
                    vec![red()],
                )],
            )],
        ),
        (
            "group and mask layer opacity",
            vec![Layer {
                opacity: 0.5,
                ..masked_group(
                    EvaluatedTransform::default(),
                    alpha_mask(vec![Layer {
                        opacity: 0.5,
                        ..disc("matte", 30.0, 24.0, 14.0, "#FFFFFFFF")
                    }]),
                    vec![red()],
                )
            }],
        ),
    ];
    for (case, layers) in cases {
        let mut scene = empty_scene(64, 48);
        scene.layers = layers;
        let gpu = renderer.render(&scene).unwrap();
        let cpu = celesta_renderer::CpuRenderer::default()
            .render(&scene)
            .unwrap();
        let difference = max_channel_difference(&gpu, &cpu);
        assert!(difference <= 3, "{case}: channels differ by up to {difference}");
    }
}

#[test]
fn a_shadow_follows_a_masked_shape_like_the_cpu_renderer() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let mut group = masked_group(
        EvaluatedTransform::default(),
        alpha_mask(vec![disc("matte", 24.0, 24.0, 12.0, "#FFFFFFFF")]),
        vec![corner_rect("red", 0.0, 0.0, 64.0, 48.0, "#FF0000FF")],
    );
    group.effects.shadow = Some(LayerShadow {
        color: "#000000C0".to_owned(),
        blur: 2.0,
        offset_x: 12.0,
        offset_y: 4.0,
    });
    let mut scene = empty_scene(64, 48);
    scene.layers = vec![group];
    let gpu = renderer.render(&scene).unwrap();
    let cpu = celesta_renderer::CpuRenderer::default()
        .render(&scene)
        .unwrap();
    let difference = max_channel_difference(&gpu, &cpu);
    assert!(difference <= 5, "channels differ by up to {difference}");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p celesta-gpu-renderer masks_groups_like_the_cpu_renderer a_shadow_follows_a_masked_shape`
Expected: FAIL. The GPU ignores `mask` and draws the children whole. On a machine with no adapter, `renderer()` returns `None` and the test returns early, so run this where `cargo test -p celesta-gpu-renderer` normally exercises the GPU (macOS Metal, or Linux with lavapipe).

- [ ] **Step 3: Add the mask pass**

Create `crates/gpu-renderer/src/mask.wgsl`:

```wgsl
// Multiplies a masked group's canvas by its mask, texel for texel. Both
// canvases hold premultiplied alpha and cover the same part of the scene.

@group(0) @binding(0)
var children: texture_2d<f32>;

@group(1) @binding(0)
var matte: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> VertexOutput {
    let vertices = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return VertexOutput(vec4<f32>(vertices[index], 0.0, 1.0));
}

// Rec. 709 luma of sRGB-encoded values. On a premultiplied color it already
// includes the alpha, as `celesta_renderer`'s `mask_value` multiplies it in.
const LUMA = vec3<f32>(0.2126, 0.7152, 0.0722);

fn masked(position: vec4<f32>, luminance: bool, invert: bool) -> vec4<f32> {
    let texel = vec2<i32>(floor(position.xy));
    let color = textureLoad(children, texel, 0);
    let mask = textureLoad(matte, texel, 0);
    var shown = mask.a;
    if luminance {
        shown = dot(mask.rgb, LUMA);
    }
    if invert {
        shown = 1.0 - shown;
    }
    return color * clamp(shown, 0.0, 1.0);
}

@fragment
fn alpha(input: VertexOutput) -> @location(0) vec4<f32> {
    return masked(input.position, false, false);
}

@fragment
fn alpha_inverted(input: VertexOutput) -> @location(0) vec4<f32> {
    return masked(input.position, false, true);
}

@fragment
fn luminance(input: VertexOutput) -> @location(0) vec4<f32> {
    return masked(input.position, true, false);
}

@fragment
fn luminance_inverted(input: VertexOutput) -> @location(0) vec4<f32> {
    return masked(input.position, true, true);
}
```

Create `crates/gpu-renderer/src/mask.rs`:

```rust
use crate::compositor::begin_pass;
use crate::texture::CanvasTexture;
use celesta_composition::{GroupMask, MaskMode};

/// How a masked group's canvas is multiplied by its mask's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MaskSpec {
    pub(crate) mode: MaskMode,
    pub(crate) invert: bool,
}

impl MaskSpec {
    pub(crate) fn new(mask: &GroupMask) -> Self {
        Self {
            mode: mask.mode,
            invert: mask.invert,
        }
    }

    /// The index of this spec's entry point in `ENTRY_POINTS`.
    fn index(self) -> usize {
        let mode = match self.mode {
            MaskMode::Alpha => 0,
            MaskMode::Luminance => 2,
        };
        mode + usize::from(self.invert)
    }
}

/// `mask.wgsl`'s fragment entry points, one per `MaskSpec`.
const ENTRY_POINTS: [&str; 4] = ["alpha", "alpha_inverted", "luminance", "luminance_inverted"];

/// The pass that shows a group's children through its mask.
pub(crate) struct MaskPipelines {
    pipelines: [wgpu::RenderPipeline; 4],
}

impl MaskPipelines {
    /// Reads both canvases through `texture_layout` (`layer.wgsl`'s), so a
    /// canvas's own bind group serves every shader.
    pub(crate) fn new(device: &wgpu::Device, texture_layout: &wgpu::BindGroupLayout) -> Self {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Celesta mask pipeline layout"),
            bind_group_layouts: &[Some(texture_layout), Some(texture_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("mask.wgsl"));
        let pipelines = ENTRY_POINTS.map(|entry_point| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Celesta mask pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
        Self { pipelines }
    }

    /// Writes `children` shown through `matte` onto `target`. All three
    /// cover the same region, so the pass covers the whole target.
    pub(crate) fn apply(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        children: &CanvasTexture,
        matte: &CanvasTexture,
        target: &CanvasTexture,
        spec: MaskSpec,
    ) {
        let mut pass = begin_pass(
            encoder,
            &target.view,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        pass.set_pipeline(&self.pipelines[spec.index()]);
        pass.set_bind_group(0, &children.bind_group, &[]);
        pass.set_bind_group(1, &matte.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
```

In `crates/gpu-renderer/src/lib.rs`, add `mod mask;` after `mod layer;`.

In `crates/gpu-renderer/src/renderer.rs`, add the field after `pub(crate) effects: EffectProcessor,`:

```rust
    /// Shows masked groups' children through their masks.
    pub(crate) masks: MaskPipelines,
```

Next to `let effects = EffectProcessor::new(&device, &texture_bind_group_layout);`, add `let masks = MaskPipelines::new(&device, &texture_bind_group_layout);`, put `masks,` after `effects,` in the struct literal, and add `use crate::mask::MaskPipelines;`.

- [ ] **Step 4: Plan masked groups**

`crates/gpu-renderer/src/bounds.rs`, in `impl PixelBounds` after `union`:

```rust
    /// The part both cover; `None` when either is `None` or they do not
    /// overlap.
    pub(crate) fn intersection(a: Option<Self>, b: Option<Self>) -> Option<Self> {
        let (Self(a), Self(b)) = (a?, b?);
        let bounds = [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
        (bounds[2] > bounds[0] && bounds[3] > bounds[1]).then_some(Self(bounds))
    }
```

`crates/gpu-renderer/src/layer.rs`: add `use crate::mask::MaskSpec;` and, in `PreparedItem` after `EndEffect`, add:

```rust
    /// The items up to the matching `MaskContent` draw a group's mask onto a
    /// transparent canvas, those from there to the matching `EndMask` its
    /// children onto another covering the same region.
    BeginMask,
    MaskContent,
    /// Draws the children shown through the mask onto the group's parent.
    EndMask(PreparedLayer, MaskSpec),
```

`crates/gpu-renderer/src/plan.rs`: add `use crate::mask::MaskSpec;`. In `plan_groups`, declare next to `open`:

```rust
    // What each open mask drew, once its group's children have begun.
    let mut masks: Vec<Option<PixelBounds>> = Vec::new();
```

Replace the `PreparedItem::BeginGroup` arm with one that also begins masks, and add the mask arms:

```rust
            PreparedItem::BeginGroup | PreparedItem::BeginMask => {
                open.push((plans.len(), None));
                plans.push(plan(None, None));
            }
            PreparedItem::MaskContent => {
                let (index, mask) = open.pop().expect("every mask was begun");
                masks.push(mask);
                open.push((index, None));
            }
            PreparedItem::EndMask(_, spec) => {
                let (index, children) = open.pop().expect("every mask was begun");
                let mask = masks.pop().expect("every mask has content");
                // Inverted, the children show wherever the mask is not
                // drawn, which can be anywhere they are.
                let shown = if spec.invert {
                    children
                } else {
                    PixelBounds::intersection(children, mask)
                };
                plans[index] = plan(shown, shown);
                cover(&mut open, shown);
            }
```

Update `GroupPlan`'s doc comment from "The canvas an isolated group or an effect draws its layers onto" to "The canvas an isolated group, an effect, or a mask and its group's children draw their layers onto". In `pub(crate) enum GpuStep`, after `EndEffect { … }`, add:

```rust
    /// Starts drawing a mask onto a fresh transparent canvas covering
    /// `canvas`; the steps from the matching `MaskContent` draw the group's
    /// children onto a second one covering the same region.
    BeginMask { canvas: CanvasRegion },
    MaskContent,
    /// Draws the children shown through the mask onto the parent with
    /// `instance`.
    EndMask {
        instance: u32,
        blend_mode: BlendMode,
        mask: MaskSpec,
        /// As for `EndGroup`.
        area: Option<[u32; 4]>,
    },
```

- [ ] **Step 5: Prepare masked groups**

`crates/gpu-renderer/src/prepare.rs`: add `use crate::mask::MaskSpec;`. In `prepare_layer`'s `Group` arm, change the pattern to `LayerContent::Group { layers, clip, mask } => {` and insert after the `if let Some(clip) = clip { … }` block, before `if blend_mode.is_normal()`:

```rust
                if let Some(mask) = mask {
                    // Always isolated: the mask, drawn without clips, then
                    // the children, which carry them, each onto a canvas of
                    // their own; the children shown through the mask then
                    // draw onto the parent as one layer.
                    output.push(PreparedItem::BeginMask);
                    let matte = LayerState {
                        opacity: 1.0,
                        clip: None,
                        ..state
                    };
                    for layer in &mask.layers {
                        self.prepare_layer(layer, matte, output)?;
                    }
                    output.push(PreparedItem::MaskContent);
                    let inner = LayerState {
                        opacity: 1.0,
                        ..child_state
                    };
                    for child in layers {
                        self.prepare_layer(child, inner, output)?;
                    }
                    output.push(PreparedItem::EndMask(
                        PreparedLayer::canvas(
                            LayerState {
                                transform: Affine::IDENTITY,
                                opacity: state.opacity,
                                clip: None,
                            },
                            blend_mode,
                            true,
                        ),
                        MaskSpec::new(mask),
                    ));
                    return Ok(());
                }
```

In `prepare_draws` (the function that builds `instances` and `steps`), change the instance count's filter so items without an instance are skipped:

```rust
        let instance_count = items
            .iter()
            .filter(|item| {
                !matches!(
                    item,
                    PreparedItem::BeginGroup | PreparedItem::BeginMask | PreparedItem::MaskContent
                )
            })
            .count()
```

Leave the rest of that expression unchanged. In the `for item in items` loop's `match item`, add these arms after `PreparedItem::BeginGroup => { … }`:

```rust
                PreparedItem::BeginMask => {
                    let group = groups.next().expect("plan_groups plans every mask");
                    steps.push(GpuStep::BeginMask {
                        canvas: group.canvas,
                    });
                    open.push(group);
                    continue;
                }
                // The children draw onto a canvas covering the mask's region.
                PreparedItem::MaskContent => {
                    steps.push(GpuStep::MaskContent);
                    continue;
                }
                PreparedItem::EndMask(layer, mask) => {
                    let group = open.pop().expect("every mask was begun");
                    let target = open.last().expect("the root canvas is always open").canvas;
                    layer.write_instance(target, group.canvas, &mut instances);
                    steps.push(GpuStep::EndMask {
                        instance: index,
                        blend_mode: layer.blend_mode,
                        mask,
                        area: group
                            .drawn
                            .then(|| target.local_area(group.canvas.bounds()))
                            .flatten(),
                    });
                    index += 1;
                    continue;
                }
```

`composited` already becomes true for any non-`Layer` item, so a frame with a mask composites through canvases.

- [ ] **Step 6: Composite masked groups**

`crates/gpu-renderer/src/compositor.rs`: add `use crate::mask::MaskPipelines;`, and add a field to `Compositor` after `effects`:

```rust
    pub(crate) masks: &'a MaskPipelines,
```

In `draw_canvas`, replace `GpuStep::BeginGroup { .. } => {` with `GpuStep::BeginGroup { .. } | GpuStep::BeginMask { .. } => {`, and add this arm to the `match &steps[end]` inside it:

```rust
                        GpuStep::EndMask {
                            instance,
                            blend_mode,
                            area,
                            ..
                        } => (*instance, *blend_mode, *area),
```

Replace the final `GpuStep::EndGroup { .. } | GpuStep::EndEffect { .. } => { unreachable!(…) }` arm with:

```rust
                GpuStep::EndGroup { .. }
                | GpuStep::EndEffect { .. }
                | GpuStep::MaskContent
                | GpuStep::EndMask { .. } => {
                    unreachable!("draw_group consumes every group's steps")
                }
```

In `draw_group`, replace the `let (Some(GpuStep::BeginGroup { canvas: region }), Some(end)) = … else { … };` statement with the version below. Then insert the mask branch right after it, before the existing `let canvas = self.effects.take_canvas(…)`:

```rust
        let (
            Some(GpuStep::BeginGroup { canvas: region } | GpuStep::BeginMask { canvas: region }),
            Some(end),
        ) = (steps.first(), steps.last())
        else {
            unreachable!("a group runs from its BeginGroup or BeginMask to its end");
        };
        if let GpuStep::EndMask { mask, .. } = end {
            let split = mask_content(steps);
            // Copied out of `self`, so the closure borrows nothing of it
            // while `draw_canvas` takes `&mut self`.
            let (device, layout) = (self.device, self.texture_layout);
            let take = |effects: &mut crate::effect::EffectProcessor| {
                effects.take_canvas(device, layout, region.width, region.height)
            };
            let matte = take(self.effects);
            self.draw_canvas(encoder, &matte, wgpu::Color::TRANSPARENT, &steps[1..split]);
            let children = take(self.effects);
            self.draw_canvas(
                encoder,
                &children,
                wgpu::Color::TRANSPARENT,
                &steps[split + 1..steps.len() - 1],
            );
            let result = take(self.effects);
            self.masks.apply(encoder, &children, &matte, &result, *mask);
            self.effects.recycle(matte);
            self.effects.recycle(children);
            return Some(result);
        }
```

Update `draw_group`'s doc comment to "Composites the group `steps` (from its `BeginGroup` or `BeginMask` to its end) onto a canvas of its own and applies its effect or mask, …".

Replace `group_end` with one that counts masks, and add `mask_content` below it:

```rust
/// The index of the step that ends the group `steps[begin]` begins.
pub(crate) fn group_end(steps: &[GpuStep], begin: usize) -> usize {
    let mut depth = 0;
    for (index, step) in steps.iter().enumerate().skip(begin) {
        match step {
            GpuStep::BeginGroup { .. } | GpuStep::BeginMask { .. } => depth += 1,
            GpuStep::EndGroup { .. } | GpuStep::EndEffect { .. } | GpuStep::EndMask { .. } => {
                depth -= 1;
                if depth == 0 {
                    return index;
                }
            }
            GpuStep::Draw(_) | GpuStep::Blend { .. } | GpuStep::MaskContent => {}
        }
    }
    unreachable!("every group has an end")
}

/// The index of the `MaskContent` that ends the mask `steps[0]` begins.
fn mask_content(steps: &[GpuStep]) -> usize {
    let mut depth = 0;
    for (index, step) in steps.iter().enumerate() {
        match step {
            GpuStep::BeginGroup { .. } | GpuStep::BeginMask { .. } => depth += 1,
            GpuStep::EndGroup { .. } | GpuStep::EndEffect { .. } | GpuStep::EndMask { .. } => {
                depth -= 1;
            }
            GpuStep::MaskContent if depth == 1 => return index,
            GpuStep::Draw(_) | GpuStep::Blend { .. } | GpuStep::MaskContent => {}
        }
    }
    unreachable!("every mask has its content")
}
```

`crates/gpu-renderer/src/encode.rs`: in the `Compositor { … }` literal in `encode_composited`, add `masks: &self.masks,` after `effects: &mut self.effects,`.

- [ ] **Step 7: Build and run the GPU tests**

Run: `cargo test -p celesta-gpu-renderer`
Expected: PASS, including `masks_groups_like_the_cpu_renderer` and `a_shadow_follows_a_masked_shape_like_the_cpu_renderer`, and every existing test. Some existing tests match on `PreparedItem` or `GpuStep`; if one fails to compile with "non-exhaustive patterns", add the new variants to its arm, next to `BeginGroup` / `EndGroup`.

If one case exceeds the tolerance of 3, print `max_channel_difference` for that case alone and the first differing pixel. A difference at the anti-aliased rim of a scaled disc is rasterization, as in `clips_groups_like_the_cpu_renderer`: compare away from the rim for that case only, with a comment saying so. A difference inside a flat area is a real bug: fix it.

- [ ] **Step 8: Run clippy and commit**

Run: `cargo clippy -p celesta-gpu-renderer --all-targets -- -D warnings`
Expected: no warnings.

```bash
git add crates/gpu-renderer
git commit -m "feat(gpu-renderer): draw groups through alpha and luminance masks (#146)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: GPU masks of paths, text, and images, rotation, and planning

**Files:**
- Test: `crates/gpu-renderer/src/tests.rs`
- Modify (only if a test exposes a bug): files from Task 4

**Interfaces:**
- Consumes: `masked_group`, `alpha_mask`, and `disc` (Task 4 test helpers), plus the existing `path_layer`, `assert_paths_match_cpu`, `polyline`, `corner_rect`, `pixel_at`, `renderer`, and `empty_scene`.

- [ ] **Step 1: Write the tests**

Append to `crates/gpu-renderer/src/tests.rs`:

```rust
#[test]
fn a_path_masks_a_group_like_the_cpu_renderer() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let diamond = path_layer(
        "diamond",
        polyline(&[(48.0, 4.0), (88.0, 32.0), (48.0, 60.0), (8.0, 32.0)], true),
        Some(Paint::Solid {
            color: "#FFFFFFFF".to_owned(),
        }),
        None,
    );
    assert_paths_match_cpu(
        &mut renderer,
        "path mask",
        vec![masked_group(
            EvaluatedTransform::default(),
            alpha_mask(vec![diamond]),
            vec![corner_rect("red", 0.0, 0.0, 96.0, 64.0, "#FF0000FF")],
        )],
    );
}

#[test]
fn text_masks_a_group() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let text = Layer {
        id: "word".to_owned(),
        transform: EvaluatedTransform {
            position: Point { x: 48.0, y: 32.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Text {
            text: "MASK".to_owned(),
            style: TextStyle {
                font_size: Some(36.0),
                ..TextStyle::default()
            },
            max_width: None,
            baseline_anchor: false,
        },
    };
    let mut scene = empty_scene(96, 64);
    scene.layers = vec![masked_group(
        EvaluatedTransform::default(),
        alpha_mask(vec![text]),
        vec![corner_rect("red", 0.0, 0.0, 96.0, 64.0, "#FF0000FF")],
    )];
    let frame = renderer.render(&scene).unwrap();
    let red = frame
        .pixels()
        .chunks_exact(4)
        .filter(|pixel| *pixel == [255, 0, 0, 255])
        .count();
    // The glyphs show some of the red, and their gaps hide the rest.
    assert!(red > 50, "only {red} red pixels");
    assert!(red < 96 * 64 / 2, "{red} red pixels: the text did not mask");
    assert_ne!(pixel_at(&frame, 0, 0), [255, 0, 0, 255]);
}

#[test]
fn an_image_masks_a_group_by_luminance() {
    let Some(renderer) = renderer(GpuRenderOptions {
        background: Color::TRANSPARENT,
        ..GpuRenderOptions::default()
    }) else {
        return;
    };
    let mut renderer = renderer.with_asset_root(env!("CARGO_MANIFEST_DIR"));
    // The 2x2 checker holds red, green / blue, white, row by row.
    let checker = Layer {
        id: "checker".to_owned(),
        transform: EvaluatedTransform {
            anchor: Point { x: 0.0, y: 0.0 },
            ..EvaluatedTransform::default()
        },
        opacity: 1.0,
        blend_mode: BlendMode::Normal,
        effects: Default::default(),
        content: LayerContent::Image {
            width: None,
            height: None,
            fit: None,
            asset: ResolvedAsset {
                id: "checker".to_owned(),
                location: AssetLocation::File {
                    path: "tests/assets/checker.ppm".to_owned(),
                },
            },
        },
    };
    let mut scene = empty_scene(2, 2);
    scene.layers = vec![masked_group(
        EvaluatedTransform::default(),
        GroupMask {
            mode: MaskMode::Luminance,
            ..alpha_mask(vec![checker])
        },
        vec![corner_rect("white", 0.0, 0.0, 2.0, 2.0, "#FFFFFFFF")],
    )];
    let frame = renderer.render(&scene).unwrap();
    // White shown through each texel's luma: premultiplied, every channel
    // is the alpha.
    for ((x, y), luma) in [((0, 0), 54), ((1, 0), 182), ((0, 1), 18), ((1, 1), 255)] {
        let pixel = pixel_at(&frame, x, y);
        for channel in pixel {
            assert!(
                channel.abs_diff(luma) <= 1,
                "({x}, {y}): {pixel:?}, expected {luma} in every channel"
            );
        }
    }
}

#[test]
fn a_mask_rotates_with_its_group() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    // As in `a_clip_rotates_with_its_group`: turned 90 degrees clockwise
    // about (20, 20), the mask's 20x10 rect lands on canvas x 10..20,
    // y 20..40.
    let mut scene = empty_scene(40, 40);
    scene.layers = vec![masked_group(
        EvaluatedTransform {
            position: Point { x: 20.0, y: 20.0 },
            rotation: 90.0,
            ..EvaluatedTransform::default()
        },
        alpha_mask(vec![corner_rect("matte", 0.0, 0.0, 20.0, 10.0, "#FFFFFFFF")]),
        vec![corner_rect("red", -40.0, -40.0, 80.0, 80.0, "#FF0000FF")],
    )];
    let frame = renderer.render(&scene).unwrap();
    assert_eq!(pixel_at(&frame, 15, 30), [255, 0, 0, 255]);
    let background = renderer.options().background;
    let background = [
        background.red,
        background.green,
        background.blue,
        background.alpha,
    ];
    for (x, y) in [(8, 30), (22, 30), (15, 18)] {
        assert_eq!(pixel_at(&frame, x, y), background, "({x}, {y}) leaked");
    }
}

#[test]
fn a_mask_off_the_children_draws_nothing_unless_inverted() {
    let Some(mut renderer) = renderer(GpuRenderOptions::default()) else {
        return;
    };
    let group = |invert| {
        masked_group(
            EvaluatedTransform::default(),
            GroupMask {
                invert,
                ..alpha_mask(vec![corner_rect("matte", 30.0, 30.0, 8.0, 8.0, "#FFFFFFFF")])
            },
            vec![corner_rect("red", 0.0, 0.0, 16.0, 16.0, "#FF0000FF")],
        )
    };
    let mut scene = empty_scene(40, 40);
    scene.layers = vec![group(false)];
    let hidden = renderer.render(&scene).unwrap();
    assert_ne!(pixel_at(&hidden, 8, 8), [255, 0, 0, 255]);
    scene.layers = vec![group(true)];
    let shown = renderer.render(&scene).unwrap();
    assert_eq!(pixel_at(&shown, 8, 8), [255, 0, 0, 255]);
    assert_ne!(pixel_at(&shown, 34, 34), [255, 0, 0, 255]);
}
```

- [ ] **Step 2: Run them**

Run: `cargo test -p celesta-gpu-renderer path_masks text_masks image_masks mask_rotates mask_off_the_children`
Expected: PASS. If `an_image_masks_a_group_by_luminance` fails, check two things before touching the renderer:

- **Texel order.** `decodes_and_rotates_a_nested_image_on_the_gpu` shows the checker rotated 180°. If the order differs, fix the comment and the expectations.
- **Readback format.** The tests' comment says readback is premultiplied. If a transparent frame reads back straight instead, expect `[255, 255, 255, luma]`. If any other test fails, debug it with superpowers:systematic-debugging before changing Task 4's code.

- [ ] **Step 3: Run the whole workspace and commit**

Run: `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, no warnings.

```bash
git add crates/gpu-renderer
git commit -m "test(gpu-renderer): mask groups with paths, text, images, and rotation (#146)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

**End of PR 1.** Push the branch and open the PR with `Closes` left out (PR 2 closes #146). The body describes the scene JSON, the order of application, and the test coverage, and ends with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

---

## PR 2: React API, example, bench, docs

Start PR 2 on a branch from `main` after PR 1 merges. Build the runtime first:

```bash
cd packages/react && pnpm install && pnpm run codegen && pnpm run build
```

### Task 6: `<Mask>` and `<Group mask>`

**Files:**
- Modify: `packages/react/src/components.ts` (`MaskProps`, `Mask`, `GroupProps.mask`, `Group`)
- Modify: `packages/react/src/render.ts` (`HOST_TYPES`, `buildLayer`'s group branch, `walkNode`, new `buildMask`/`extractMaskMode`)
- Modify: `packages/react/src/scene.ts` (re-export `GroupMask`, `MaskMode`)
- Modify: `packages/react/src/index.ts` (export `Mask`, `MaskProps`, `GroupMask`, `MaskMode`)
- Test: `packages/react/test/mask.test.mjs`

**Interfaces:**
- Consumes: generated `GroupMask` and `MaskMode` (Task 1).
- Produces: `Mask(props: MaskProps)`, which renders a `'mask'` host element; `GroupProps.mask?: ReactNode`. The scene's group gets `mask: { layers, mode?, invert? }`.

- [ ] **Step 1: Write the failing tests**

Create `packages/react/test/mask.test.mjs`:

```js
import { expect, test } from 'vitest';
import React from 'react';

import { Audio, Composition, FreezeFrame, Group, Mask, Rect, Sequence, Text, Video, useCurrentFrame } from '../dist/index.js';
import { mount } from '../dist/render.js';

const h = React.createElement;
const at = (frame) => ({ value: frame, timescale: 30 });

function scene(children, frame = 0) {
  const Root = () => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 30 }, children);
  return mount(Root).renderAt(at(frame), null);
}

const square = (fill) => h(Rect, { width: 10, height: 10, fill });

test('a <Mask> child becomes the group mask at any sibling position', () => {
  for (const children of [
    [h(Mask, { key: 'm' }, square('#ffffff')), h(Rect, { key: 'r', width: 50, height: 50, fill: '#ff0000' })],
    [h(Rect, { key: 'r', width: 50, height: 50, fill: '#ff0000' }), h(Mask, { key: 'm' }, square('#ffffff'))],
  ]) {
    const [group] = scene(h(Group, null, ...children)).scene.layers;
    expect(group.content.layers).toHaveLength(1);
    expect(group.content.layers[0].content.fill).toEqual({ type: 'solid', color: '#ff0000' });
    expect(group.content.mask).toEqual({ layers: [expect.objectContaining({ content: expect.objectContaining({ type: 'rect' }) })] });
  }
});

test('the mask prop, a <Mask> in the prop, and a <Mask> child serialize alike', () => {
  const matte = () => square('#ffffff');
  const child = () => h(Rect, { width: 50, height: 50, fill: '#ff0000' });
  const groups = scene([
    h(Group, { key: 1, mask: matte() }, child()),
    h(Group, { key: 2, mask: h(Mask, null, matte()) }, child()),
    h(Group, { key: 3 }, child(), h(Mask, null, matte())),
  ]).scene.layers;
  // Layer ids are tree paths, so only the rest of each mask is compared.
  const [bare, wrapped, nested] = groups.map(({ content }) => ({
    ...content.mask,
    layers: content.mask.layers.map(({ content: layer }) => layer),
  }));
  expect(wrapped).toEqual(bare);
  expect(nested).toEqual(bare);
  expect('mode' in bare).toBe(false);
  expect('invert' in bare).toBe(false);

  const [luminance] = scene(h(Group, { mask: h(Mask, { mode: 'luminance', invert: true }, matte()) }, child())).scene.layers;
  expect(luminance.content.mask.mode).toBe('luminance');
  expect(luminance.content.mask.invert).toBe(true);
});

test('null and false give no mask; an empty <Mask> gives an empty one', () => {
  for (const mask of [null, undefined, false]) {
    const [group] = scene(h(Group, { mask }, square('#ff0000'))).scene.layers;
    expect('mask' in group.content).toBe(false);
  }
  const [group] = scene(h(Group, { mask: h(Mask) }, square('#ff0000'))).scene.layers;
  expect(group.content.mask).toEqual({ layers: [] });
});

test('toggling the mask prop keeps the children ids and state', () => {
  const seen = [];
  function Counter() {
    // The initializer runs once per mount.
    React.useState(() => seen.push('mount'));
    return h(Rect, { width: 10, height: 10, fill: '#ff0000' });
  }
  function Root() {
    const frame = useCurrentFrame();
    return h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 30 },
      h(Group, { mask: frame % 2 === 1 && square('#ffffff') },
        h(Video, { src: './a.mp4' }), h(Counter)));
  }
  const mounted = mount(Root);
  const ids = [0, 1, 2].map((frame) => mounted.renderAt(at(frame), null).scene.layers[0].content.layers.map((l) => l.id));
  expect(ids[0]).toEqual(ids[1]);
  expect(ids[1]).toEqual(ids[2]);
  expect(seen).toEqual(['mount']);
});

test('mask layers get their own tree-path ids and inherit lang', () => {
  const [group] = scene(h(Group, { lang: 'ja', mask: h(Video, { src: './a.mp4' }) }, h(Video, { src: './a.mp4' }))).scene.layers;
  const childId = group.content.layers[0].id;
  const maskId = group.content.mask.layers[0].id;
  expect(childId).not.toBe(maskId);
  const [texted] = scene(h(Group, { lang: 'ja', mask: h(Text, null, 'あ') }, square('#ff0000'))).scene.layers;
  expect(texted.content.mask.layers[0].content.style.lang).toBe('ja');
});

test('a misplaced or second mask throws', () => {
  const misplaced = [
    h(Mask, null, square('#ffffff')),
    h(Sequence, null, h(Mask, null, square('#ffffff'))),
    h(FreezeFrame, { frame: 0 }, h(Mask, null, square('#ffffff'))),
    h(Group, { mask: h(Mask, null, h(Mask, null, square('#ffffff'))) }),
  ];
  for (const children of misplaced) {
    expect(() => scene(children)).toThrow('`<Mask>` must be a direct child of `<Group>`');
  }
  expect(() => scene(h(Group, null, h(Mask), h(Mask)))).toThrow('a <Group> takes at most one mask');
  expect(() => scene(h(Group, { mask: square('#ffffff') }, h(Mask)))).toThrow('a <Group> takes at most one mask');
  expect(() => scene(h(Group, { mask: h(Mask, { mode: 'red' }) }))).toThrow(
    'unknown mask mode "red"; expected one of alpha, luminance',
  );
});

test('audio inside a mask plays once, in renderAt and in collectAudio', () => {
  const Root = () => h(Composition, { width: 100, height: 100, fps: 30, durationInFrames: 2 },
    h(Group, { mask: h(Audio, { src: 'matte.wav' }) }, h(Audio, { src: 'child.wav' })));
  const mounted = mount(Root);
  const frame = mounted.renderAt(at(0), null).audio.map(({ src }) => src);
  expect(frame.sort()).toEqual(['child.wav', 'matte.wav']);
  const all = mount(Root).collectAudio().map(({ src }) => src);
  expect(all.filter((src) => src === 'matte.wav')).toHaveLength(2);
  expect(all.filter((src) => src === 'child.wav')).toHaveLength(2);
});
```

`renderAt` throws walker errors directly; the CLI-based tests (`clip.test.mjs`) see them as `frame.error` instead. A string `fill` serializes as `{ type: 'solid', color }`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cd packages/react && pnpm run build && pnpm exec vitest run test/mask.test.mjs`
Expected: FAIL. `Mask` is not exported.

- [ ] **Step 3: Add `Mask` and the `mask` prop**

In `packages/react/src/components.ts`, after `ClipRect`:

```ts
export interface MaskProps {
  /** What decides visibility: the mask's alpha (default) or its luminance. */
  mode?: 'alpha' | 'luminance';
  /** Shows the group where the mask is not drawn instead. Defaults to false. */
  invert?: boolean;
  children?: ReactNode;
}
```

In `GroupProps`, after `clip?: ClipRect;`:

```ts
  /**
   * Shows the children only where this is drawn: by its alpha, or as a
   * `<Mask>` element sets. Drawn in the group's own coordinate space and
   * never shown itself. `null`, `undefined`, and booleans mean no mask; an
   * empty `<Mask />` hides the group (shows it whole when inverted).
   * Masking isolates the group, like a blend mode other than `'normal'`.
   */
  mask?: ReactNode;
```

Replace `Group` with:

```ts
export function Group({ mask, children, ...props }: GroupProps): ReturnType<typeof React.createElement> {
  const runtime = React.useContext(CompositionRuntimeContext);
  const lang = resolveTextLanguage(props.lang, runtime?.lang);
  // The mask goes after the children, always in the same slot, so turning it
  // on or off changes neither their tree-path ids nor their state.
  const element = React.createElement('group', props, children, maskElement(mask));
  return runtime && props.lang !== undefined
    ? React.createElement(CompositionRuntimeContext.Provider, { value: { ...runtime, lang } }, element)
    : element;
}

/** A `mask` prop as a `<Mask>` element, or null without a mask. */
function maskElement(mask: ReactNode): ReturnType<typeof React.createElement> | null {
  if (mask === null || mask === undefined || typeof mask === 'boolean') {
    return null;
  }
  return React.isValidElement(mask) && mask.type === Mask ? mask : React.createElement(Mask, null, mask);
}

/**
 * Decides, per pixel, how much of the enclosing `<Group>` shows: by alpha, or
 * by luminance with `mode="luminance"`. Put it directly in a `<Group>`, or
 * pass it as the group's `mask` prop. Its children are positioned in the
 * group's coordinate space and never shown themselves.
 */
export function Mask(props: MaskProps): ReturnType<typeof React.createElement> {
  return React.createElement('mask', props);
}
```

In `packages/react/src/index.ts`, add `Mask` to the `export { … } from './components'` list and `MaskProps` to the `export type { … } from './components'` list. Add `GroupMask` and `MaskMode` to the `export type { … }` list that re-exports `./scene`. In `packages/react/src/scene.ts`, add:

```ts
export type { GroupMask } from './generated/GroupMask';
export type { MaskMode } from './generated/MaskMode';
```

- [ ] **Step 4: Teach the walker masks**

In `packages/react/src/render.ts`, add `'mask'` to `HOST_TYPES`, and add `GroupMask` and `MaskMode` to the type imports from `./scene` (or from `./generated/…`, matching how `Clip` is imported in this file). Replace the `layers: walkChildren(node, path, context, audio),` of the `group`/`sequence` branch in `buildLayer` with a walk that takes the mask out:

```ts
  if (node.type === 'group' || node.type === 'sequence') {
    const clip = node.type === 'group' ? extractClip(props.clip) : undefined;
    let mask: GroupMask | undefined;
    // Siblings in order, so audio is collected in the order the audio-only
    // walk collects it.
    const layers = node.children.flatMap((child, index) => {
      const childPath = [path, index].join('.');
      if (node.type === 'group' && child.type === 'mask') {
        if (mask) {
          throw new Error('a <Group> takes at most one mask');
        }
        mask = buildMask(child, childPath, context, audio);
        return [];
      }
      return walkNode(child, childPath, context, audio);
    });
    content = {
      type: 'group',
      layers,
      ...(clip ? { clip } : {}),
      ...(mask ? { mask } : {}),
    };
```

Next to `extractClip`, add:

```ts
const MASK_MODES: readonly MaskMode[] = ['alpha', 'luminance'];

/** A group's `<Mask>` host child as the scene's `GroupMask`. */
function buildMask(node: HostNode, path: string, context: WalkContext, audio: AudioClipDescriptor[]): GroupMask {
  const mode = extractMaskMode(node.props.mode);
  return {
    layers: walkChildren(node, path, context, audio),
    ...(mode !== 'alpha' ? { mode } : {}),
    ...(node.props.invert === true ? { invert: true } : {}),
  };
}

function extractMaskMode(value: unknown): MaskMode {
  if (value === undefined) {
    return 'alpha';
  }
  if (!MASK_MODES.includes(value as MaskMode)) {
    throw new Error(`unknown mask mode ${JSON.stringify(value)}; expected one of ${MASK_MODES.join(', ')}`);
  }
  return value as MaskMode;
}
```

In `walkNode`, after the `rawLayers` early return, add:

```ts
  if (node.type === 'mask') {
    // A group's mask is taken out by buildLayer; anywhere else is misplaced.
    // The audio-only walk does not check placement, the visual one does.
    if (audioOnly) {
      return walkChildren(node, path, context, audio, true);
    }
    throw new Error('`<Mask>` must be a direct child of `<Group>`');
  }
```

`buildMask` walks the mask's children with `walkChildren`, so a `<Mask>` inside a `<Mask>` reaches `walkNode` and throws.

- [ ] **Step 5: Run the tests**

Run: `cd packages/react && pnpm run build && pnpm exec vitest run`
Expected: PASS, including every existing test. If `audio-collection.test.mjs`'s first test now fails, the visual walk's audio order no longer matches the audio-only walk. Check that the group branch walks siblings in order, as Step 4 does.

- [ ] **Step 6: Check that no new React warning appears**

`Group` now passes every group's children to `createElement` as a vararg instead of `props.children`, which re-runs React's dev-mode child-key validation on them. The CLI forwards console output to Rust, and every composition goes through this path, so check it on an existing multi-child example:

```bash
cd packages/react && echo '{"time":{"value":0,"timescale":1}}' | node bin/celesta-react-render.js examples/with-shapes.tsx 2>&1 >/dev/null | grep -i "key" ; echo "exit $?"
```

Expected: no line mentioning a `key` (grep finds nothing, `exit 1`). Run the same command on `main`'s build too, if a warning does appear, to tell a new warning from an existing one. If the change adds one, pass the children inside a fragment slot instead: `React.createElement('group', props, React.createElement(React.Fragment, null, children), maskElement(mask))`. Then rerun the tests.

- [ ] **Step 7: Commit**

```bash
git add packages/react/src packages/react/test/mask.test.mjs
git commit -m "feat(react): add <Mask> and <Group mask> (#146)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Text language and compact transforms reach mask layers

**Files:**
- Modify: `packages/react/src/render.ts` (`inheritTextLanguage`)
- Modify: `packages/react/src/cli.ts` (`CompactLayer`, `compactLayers`)
- Test: `packages/react/test/mask.test.mjs`, `packages/react/test/compact-transforms.test.mjs`

**Interfaces:**
- Consumes: Task 6's walker.
- Produces: no new API.

- [ ] **Step 1: Write the failing tests**

Append to `packages/react/test/mask.test.mjs`. Text supplied as `rawLayers` (project layers) inherits `lang` through `inheritTextLanguage`.

```js
test('raw project layers inside a mask inherit the group lang', () => {
  const text = { id: 't', transform: { position: { x: 0, y: 0 }, scale: { x: 1, y: 1 }, rotation: 0, anchor: { x: 0.5, y: 0.5 } },
    opacity: 1, content: { type: 'text', text: 'あ', style: {} } };
  const masked = { ...text, id: 'g', content: { type: 'group', layers: [], mask: { layers: [text] } } };
  const [group] = scene(h(Group, { lang: 'ja' }, h('rawLayers', { layers: [masked] }))).scene.layers;
  expect(group.content.layers[0].content.mask.layers[0].content.style.lang).toBe('ja');
});
```

In `packages/react/test/compact-transforms.test.mjs`, add a masked group as the entry's last layer. In the `writeFileSync(entry, …)` template, after the `</Group>` that closes `id="group"`, add:

```tsx
        <Group id="masked" mask={<Rect id="matte" x={5} y={6} width={1} height={1} fill="#ffffff" />}>
          <Rect id="child" width={1} height={1} fill="#ff0000" />
        </Group>
```

The existing tests destructure only the first two layers, so they are unaffected. Append:

```js
test('compactTransforms reaches the layers of a mask', () => {
  const [, , masked] = render({ time: { value: 0, timescale: 1 }, compactTransforms: true });
  // React's default anchor is the top-left corner, Rust's is the centre.
  assert.deepEqual(masked.content.layers[0].transform, { anchor: { x: 0, y: 0 } });
  assert.deepEqual(masked.content.mask.layers[0].transform, {
    position: { x: 5, y: 6 },
    anchor: { x: 0, y: 0 },
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd packages/react && pnpm run build && pnpm exec vitest run test/mask.test.mjs test/compact-transforms.test.mjs`
Expected: FAIL. The mask's text has no `lang`, and the mask layer keeps every transform field.

- [ ] **Step 3: Recurse into masks**

`packages/react/src/render.ts`, in `inheritTextLanguage`, replace the `group` case with:

```ts
    if (content.type === 'group') {
      return {
        ...layer,
        content: {
          ...content,
          layers: inheritTextLanguage(content.layers, lang),
          ...(content.mask ? { mask: { ...content.mask, layers: inheritTextLanguage(content.mask.layers, lang) } } : {}),
        },
      };
    }
```

`packages/react/src/cli.ts`: import `GroupMask` with the other types from `./scene`, and replace `CompactLayer` with:

```ts
/** A layer whose transform leaves out fields equal to the Rust defaults. */
type CompactLayer = Omit<Layer, 'transform' | 'content'> & {
  transform: Partial<EvaluatedTransform>;
  content:
    | Exclude<LayerContent, { type: 'group' }>
    | (Omit<Extract<LayerContent, { type: 'group' }>, 'layers' | 'mask'> & {
        layers: CompactLayer[];
        mask?: Omit<GroupMask, 'layers'> & { layers: CompactLayer[] };
      });
};
```

In `compactLayers`, replace the `content:` line with:

```ts
      content:
        content.type === 'group'
          ? {
              ...content,
              layers: compactLayers(content.layers),
              ...(content.mask ? { mask: { ...content.mask, layers: compactLayers(content.mask.layers) } } : {}),
            }
          : content,
```

- [ ] **Step 4: Run the tests and commit**

Run: `cd packages/react && pnpm run build && pnpm exec vitest run`
Expected: PASS.

```bash
git add packages/react/src packages/react/test
git commit -m "fix(react): carry lang and compact transforms into mask layers (#146)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Export a masked React entry

**Files:**
- Test: `crates/exporter/tests/png_integration.rs`

**Interfaces:**
- Consumes: Task 6's React API, PR 1's GPU renderer, and the test file's `root()` and `pixels()` helpers.

- [ ] **Step 1: Write the test**

Append to `crates/exporter/tests/png_integration.rs`:

```rust
#[test]
fn react_png_draws_a_masked_group() {
    let runtime_path = root().join("packages/react/dist/cli.js");
    if !runtime_path.exists() {
        eprintln!("skipping React PNG test: build packages/react first");
        return;
    }
    let dir = tempfile::tempdir_in(root().join("packages/react")).unwrap();
    let entry = dir.path().join("film.tsx");
    std::fs::write(
        &entry,
        r##"
import { Composition, Group, Rect } from '@celesta/react';
export default function Root() {
  return <Composition width={3} height={3} fps={4} durationInFrames={1}>
    <Group mask={<Rect width={1} height={3} fill="#ffffff" />}>
      <Rect width={3} height={3} fill="#ff0000" />
    </Group>
  </Composition>;
}
"##,
    )
    .unwrap();
    Exporter::new(ExportOptions::default())
        .export_react_png(
            &entry,
            &ReactRuntimeOptions::new("node", runtime_path),
            None,
            &[0],
            &dir.path().join("check.png"),
            |_| {},
        )
        .unwrap();
    let pixels = pixels(&dir.path().join("check-000000.png"));
    // The mask's first column shows the red; the last column does not.
    assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
    assert_ne!(&pixels[8..12], &[255, 0, 0, 255]);
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p celesta-exporter --test png_integration react_png_draws_a_masked_group -- --nocapture`
Expected: PASS, and no "skipping" line. If it prints "skipping", run `pnpm run build` in `packages/react` first.

- [ ] **Step 3: Commit**

```bash
git add crates/exporter/tests/png_integration.rs
git commit -m "test(exporter): export a masked group from a React entry (#146)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Example

**Files:**
- Create: `packages/react/examples/with-mask.tsx`

**Interfaces:**
- Consumes: `Mask`, `Group mask`, `Circle`, `Video`, `Text`, `Sequence`, `interpolate`, `Easings` from `@celesta/react`. Before writing, check the exact export names in `packages/react/src/index.ts`, and copy how `packages/react/examples/with-video.tsx` and `with-shapes.tsx` import and use them.

- [ ] **Step 1: Write the example**

```tsx
// Two uses of a group mask: video inside letters, then a circular wipe with
// a soft edge between two scenes. Put a clip.mp4 next to this file, as for
// with-video.tsx.
import { Circle, Composition, Group, Mask, Rect, Sequence, Text, Video, interpolate, useCurrentFrame } from '@celesta/react';

const WIDTH = 1920;
const HEIGHT = 1080;

function Letters() {
  return (
    <Group mask={
      <Text x={WIDTH / 2} y={HEIGHT / 2} anchorX={0.5} anchorY={0.5}
        style={{ fontSize: 360, fontWeight: 900, fill: { type: 'solid', color: '#ffffff' } }}>
        CELESTA
      </Text>
    }>
      <Video src="./clip.mp4" />
    </Group>
  );
}

function Wipe() {
  const frame = useCurrentFrame();
  const radius = interpolate(frame, [0, 45], [0, Math.hypot(WIDTH, HEIGHT) / 2 + 80], {
    extrapolateRight: 'clamp',
  });
  return (
    <>
      <Rect width={WIDTH} height={HEIGHT} fill="#1d1b2e" />
      <Group>
        <Mask>
          <Circle x={WIDTH / 2} y={HEIGHT / 2} anchorX={0.5} anchorY={0.5} radius={radius} fill="#ffffff" blur={24} />
        </Mask>
        <Rect width={WIDTH} height={HEIGHT} fill="#f2c14e" />
      </Group>
    </>
  );
}

export default function WithMask() {
  return (
    <Composition width={WIDTH} height={HEIGHT} fps={30} durationInFrames={150}>
      <Sequence durationInFrames={75}><Letters /></Sequence>
      <Sequence from={75} durationInFrames={75}><Wipe /></Sequence>
    </Composition>
  );
}
```

Adjust `interpolate`'s options and `Circle`'s props to the signatures in `packages/react/src`, if they differ.

- [ ] **Step 2: Type-check and render two stills**

Run the type check the other examples use (see `packages/react/tsconfig*.json` and `package.json` scripts for how `examples/` are checked). Then render frames 40 and 110 with the exporter's PNG mode:

```bash
cargo run -p celesta-exporter --release -- --react packages/react/examples/with-mask.tsx --frames 40,110 /tmp/with-mask.png
```

Expected: frame 40 shows the letters filled with video (or empty letters without `clip.mp4`), and frame 110 shows a yellow disc with a soft edge over the dark background. Look at both PNGs.

- [ ] **Step 3: Check the preview**

Open `packages/react/examples/with-mask.tsx` in the editor (`cargo run -p celesta-editor --release`, File > Open). Scrub through both halves, and confirm the preview matches the stills.

- [ ] **Step 4: Commit**

```bash
git add packages/react/examples/with-mask.tsx
git commit -m "docs(examples): video inside letters and a circular wipe (#146)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Bench workload and performance note

**Files:**
- Modify: `crates/bench/src/workloads.rs` (a `masks` workload)
- Create: `docs/performance/group-masks.md`

**Interfaces:**
- Consumes: the `Workload`, `Source::Synthetic`, `Canvas`, `layer`, `at`, `seconds`, and `solid` helpers in `workloads.rs`.

- [ ] **Step 1: Add the workload**

In `WORKLOADS`, after `shapes`:

```rust
    Workload {
        name: "masks",
        description: "text masking moving stripes, and a blurred circular wipe",
        source: Source::Synthetic(masks),
    },
```

Add the function after `text`:

```rust
fn masks(canvas: &Canvas, frame: usize) -> Vec<Layer> {
    let t = seconds(frame);
    let stripes: Vec<Layer> = (0..24)
        .map(|index| {
            let x = (index as f64 * 90.0 + t * 240.0) % 2160.0 - 120.0;
            layer(
                format!("stripe-{index}"),
                EvaluatedTransform {
                    anchor: Point { x: 0.0, y: 0.0 },
                    ..at(canvas.at(x, 0.0))
                },
                LayerContent::Rect {
                    width: canvas.len(45.0),
                    height: canvas.len(540.0),
                    fill: Some(solid(if index % 2 == 0 { "#F2C14E" } else { "#4E8CF2" })),
                    stroke: None,
                    corner_radius: 0.0,
                },
            )
        })
        .collect();
    let letters = layer(
        "letters".to_owned(),
        at(canvas.at(960.0, 270.0)),
        LayerContent::Text {
            text: "CELESTA".to_owned(),
            style: TextStyle {
                font_size: Some(canvas.len(300.0)),
                fill: Some(solid("#FFFFFF")),
                ..TextStyle::default()
            },
            max_width: None,
            baseline_anchor: false,
        },
    );
    let radius = canvas.len(100.0 + 400.0 * (0.5 + 0.5 * (t * PI).sin()));
    let mut disc = layer(
        "wipe-disc".to_owned(),
        at(canvas.at(960.0, 810.0)),
        LayerContent::Rect {
            width: radius * 2.0,
            height: radius * 2.0,
            fill: Some(solid("#FFFFFF")),
            stroke: None,
            corner_radius: radius,
        },
    );
    disc.effects.blur = canvas.len(24.0);
    let masked = |id: &str, mask: Vec<Layer>, layers: Vec<Layer>| {
        layer(
            id.to_owned(),
            EvaluatedTransform::default(),
            LayerContent::Group {
                layers,
                clip: None,
                mask: Some(GroupMask {
                    layers: mask,
                    mode: MaskMode::Alpha,
                    invert: false,
                }),
            },
        )
    };
    let lower = layer(
        "lower".to_owned(),
        EvaluatedTransform {
            anchor: Point { x: 0.0, y: 0.0 },
            ..at(canvas.at(0.0, 540.0))
        },
        LayerContent::Rect {
            width: canvas.len(1920.0),
            height: canvas.len(540.0),
            fill: Some(solid("#E85D75")),
            stroke: None,
            corner_radius: 0.0,
        },
    );
    vec![
        masked("text-mask", vec![letters], stripes),
        masked("wipe", vec![disc], vec![lower]),
    ]
}
```

Add `GroupMask, MaskMode` to the `celesta_composition` import. Check how `layer`, `at`, and `solid` are declared (lines ~246–270) and adapt the calls if their signatures differ.

- [ ] **Step 2: Run the workload**

Run: `cargo run -p celesta-bench --release -- --help` to find the run syntax. Then run `masks` and `text` in one invocation, three times. Expected: both workloads run, and `masks` renders without errors.

- [ ] **Step 3: Compare against main**

Run: `scripts/bench.py compare --base origin/main` (time mode on this Mac)
Expected: the existing workloads stay within noise of `main`, since a scene without masks takes the same code paths. `masks` exists only on the head.

- [ ] **Step 4: Write `docs/performance/group-masks.md`**

Follow the structure of `docs/performance/gaussian-blur-pairing.md`. Fill it with the measured numbers:

- What `masks` draws and why: one text mask and one blurred-disc mask per frame.
- Reproduction steps: the exact `celesta-bench` and `scripts/bench.py` commands, the machine, and the commit.
- The `masks` / `text` ms-per-frame ratio within one run, as a median of three runs. This Mac's absolute timings drift with heat, so report ratios, not absolute times alone.
- The `compare` table against `main`, showing the existing workloads unchanged.

- [ ] **Step 5: Commit**

```bash
git add crates/bench/src/workloads.rs docs/performance/group-masks.md
git commit -m "perf(bench): add a masks workload (#146)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Docs

**Files:**
- Modify: `packages/website/src/docs-content.tsx` (after the "Clip a group" block, around line 261)
- Modify: `packages/website/src/locales/en.json`, `packages/website/src/locales/ja.json` (keys next to `docs.chapters.layout.clip-a-group`)
- Modify: `skills/celesta/references/react-core.md` ("Group and clipping", around line 226)
- Create: `docs/handoff/<date>-group-masks.md`

- [ ] **Step 1: Website**

In `docs-content.tsx`, after the "clip a group" `<DocCode … />` line, add:

```tsx
      <h3>{text('docs.chapters.layout.mask-a-group')}</h3><p>{text('docs.chapters.layout.give-a-a-to-show-its-children', [<code>Group</code>, <code>mask</code>, <code>{'<Mask mode="luminance" invert>'}</code>])}</p>
      <DocCode label={t('docs.chapters.layout.video-inside-letters')} language="tsx" code={"<Group mask={\n  <Text x={960} y={540} anchorX={0.5} anchorY={0.5}\n    style={{ fontSize: 360, fontWeight: 900 }}>\n    CELESTA\n  </Text>\n}>\n  <Video src=\"./clip.mp4\" />\n</Group>"} />
```

In `en.json`, after `"text-sliding-up-from-behind-an-edge"`:

```json
        "mask-a-group": "Mask a group",
        "give-a-a-to-show-its-children": "Give a <slot0/> a <slot1/> to show its children only where another drawing is: text, a path, an image, or a whole subtree, in the group’s own coordinates. The mask is never shown itself. Wrap it in <slot2/> to use its brightness instead of its alpha, or to show the children where the mask is not drawn. A masked group is isolated, so its children’s blend modes mix only with each other, and its blur, shadow, and glow follow the cut-out shape.",
        "video-inside-letters": "Video inside letters",
```

In `ja.json`, at the same place:

```json
        "mask-a-group": "グループをマスクする",
        "give-a-a-to-show-its-children": "<slot0/> に <slot1/> を指定すると、別に描いたもの（テキスト、パス、画像、部分木全体）がある場所にだけ子を表示します。マスクはグループの座標系に置かれ、それ自体は表示されません。<slot2/> で包むと、alpha の代わりに明るさで表示したり、マスクのない場所に表示したりできます。マスクしたグループは分離されるため、子の描画モードは子どうしでだけ混ざり、ぼかし・影・グローは切り抜いた形に付きます。",
        "video-inside-letters": "文字の中に映像",
```

Run the website's build or type check (see `packages/website/package.json`). Expected: no missing-key errors.

- [ ] **Step 2: Skill reference**

In `skills/celesta/references/react-core.md`, under "## Group and clipping", add a `mask` row to the props table after the `clip` row:

```markdown
| `mask` | A drawing (or `<Mask mode invert>`) that decides where the children show; see below. |
```

Then add after that section's example:

````markdown
### Masks

`<Group mask={…}>` shows the children only where the mask is drawn: text,
a path, an image, or any subtree, positioned in the group's own coordinates.
The mask itself is never shown. `<Mask>` sets how it is read, and works as
the prop's value or as a direct child of the `<Group>`:

```tsx
<Group mask={<Text style={{ fontSize: 360, fontWeight: 900 }}>CELESTA</Text>}>
  <Video src="./clip.mp4" />
</Group>

<Group>
  <Mask mode="luminance" invert>
    <Circle x={960} y={540} anchorX={0.5} anchorY={0.5} radius={r} fill="#ffffff" blur={24} />
  </Mask>
  <SceneB />
</Group>
```

- `mode="alpha"` (default) uses the mask's alpha. `mode="luminance"` uses
  `(0.2126 R + 0.7152 G + 0.0722 B) × A` of its sRGB values. `invert` flips
  the mask.
- Where the mask draws nothing, nothing shows. `mask={null}` or
  `mask={false}` turns the mask off, and an empty `<Mask />` hides the group.
- A masked group is isolated, like one with a blend mode: children's blend
  modes mix only with each other. Its blur, shadow, and glow apply after the
  mask, so they follow the cut-out shape. Its `opacity` and `blendMode`
  apply once to the result.
- The group's `clip` limits the children, not the mask.
- `<Mask>` only works directly in a `<Group>` (not in `<Sequence>` or
  `<FreezeFrame>`), and a group takes one mask.
- Audio inside a mask plays normally.
````

- [ ] **Step 3: Handoff note**

Create `docs/handoff/<today's date>-group-masks.md` in the style of `docs/handoff/2026-10-06-freeze-frame.md`. Write it from what was built, not from this plan:

- Link to the spec.
- Where each piece lives: `GroupMask` in `crates/composition/src/model.rs`; `render_masked_group` and `mask_value` in the CPU renderer; `BeginMask`/`MaskContent`/`EndMask`, `plan_groups`' intersection, `MaskPipelines`, and `mask.wgsl` in the GPU renderer; `Mask`/`maskElement` in `components.ts`; `buildMask` in `render.ts`.
- The decisions a later change could break, each with its reason:
  - mask layers draw without clips;
  - the mask prop's child slot is fixed;
  - the walker collects audio in sibling order;
  - the region is the intersection unless inverted.
- Test entry points and the bench workload.

- [ ] **Step 4: Commit**

```bash
git add packages/website skills docs/handoff
git commit -m "docs: document group masks (#146)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

**End of PR 2.** Run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `pnpm exec vitest run` in `packages/react`. Push the branch and open the PR with `Closes #146`. The body lists the API, links the spec and the PR 1 PR, includes the bench table, and ends with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
