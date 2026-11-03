//! The frame's canvas, camera, and lights, as `data` asks for them.
//!
//! - **Canvas:** `data.canvas`, else the model's [`CANVAS`]; the frame
//!   answers it, and the presets frame its aspect.
//! - **Presets** frame a vertical span `[y0, y1]` of the avatar at rest,
//!   10% larger, through the presets' 30° lens, looking down −Z: `full`
//!   the framing bounds' height at their x centre, `bust` from the spine
//!   up and `face` from the neck up (without a neck, from half the head's
//!   height above it below the head), both at the head's x. The height
//!   fits from the head's z. `full` also fits the bounds' width, measured
//!   from their front (`max.z`), where the widest points nearest the camera
//!   are.
//! - **Explicit cameras** look from `position` at `target`, up +Y (−Z when
//!   they look straight up or down), through `fov` degrees.
//! - **Lights:** the reference light and ambient (Unity's defaults, see
//!   [`REFERENCE_LIGHT`]), or `data.lights`.
//!
//! `data` is not trusted: a number that is not finite, or out of its
//! range, is a `FrameError` naming its key.

use crate::humanoid::{HumanBone, Humanoid};
use crate::model::{CANVAS, FrameError};
use celesta_plugin::scene::MAX_FRAME_CANVAS_SIDE;
use celesta_plugin::{Camera, DirectionalLight, Lights, Projection};
use glam::{Mat4, Vec3};
use serde::Deserialize;

/// A framing of the humanoid, or a camera of its own.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(
    untagged,
    expecting = "camera is \"full\", \"bust\", \"face\", or { position, target, fov?, near?, far? } with finite numbers"
)]
pub enum CameraData {
    Preset(CameraPreset),
    Explicit(ExplicitCamera),
}

/// A framing of the humanoid at rest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CameraPreset {
    /// The whole avatar.
    #[default]
    Full,
    /// From the spine up.
    Bust,
    /// From the neck up.
    Face,
}

impl CameraPreset {
    /// Its name in `data`.
    fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Bust => "bust",
            Self::Face => "face",
        }
    }
}

/// A camera placed in model space.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExplicitCamera {
    pub position: [f32; 3],
    /// The point it looks at; not `position`.
    pub target: [f32; 3],
    /// The vertical field of view in degrees. Default 30.
    #[serde(default)]
    pub fov: Option<f32>,
    /// Default 0.01.
    #[serde(default)]
    pub near: Option<f32>,
    /// Default 100.
    #[serde(default)]
    pub far: Option<f32>,
}

/// Lights in place of the reference light.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LightsData {
    /// In linear colour. Celesta takes at most 4.
    #[serde(default)]
    pub directional: Vec<DirectionalLight>,
    /// A flat ambient, in linear colour. Default black.
    #[serde(default)]
    pub ambient: [f32; 3],
}

/// What the presets frame: the rest-pose bounds of what a third-person
/// camera sees, in model space, found when the model opens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Framing {
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

/// The reference light: Unity's default directional light, rotation
/// (50°, 150°, 0°) and colour sRGB (255, 244, 214) at intensity 1, in the
/// scene's axes and linear colour.
pub const REFERENCE_LIGHT: DirectionalLight = DirectionalLight {
    direction: [-0.3213938, -0.7660444, -0.5566704],
    color: [1.0, 0.9046612, 0.6724432],
    intensity: 1.0,
};

/// The reference ambient: Unity's ambient mode Color, sRGB (54, 58, 66), in
/// linear colour. It is the constant coefficient (`Lights::ambient[3]`).
pub const REFERENCE_AMBIENT: [f32; 3] = [0.0368895, 0.0423114, 0.0544803];

/// The presets' vertical field of view, and an explicit camera's default,
/// in degrees.
const FOV: f32 = 30.0;
const NEAR: f32 = 0.01;
const FAR: f32 = 100.0;
/// How much larger than the span a preset frames.
const MARGIN: f32 = 1.1;

/// The reference light and ambient, which light a frame whose `data` has no
/// `lights`.
pub fn reference_lights() -> Lights {
    let mut ambient = [[0.0; 3]; 9];
    ambient[3] = REFERENCE_AMBIENT;
    Lights {
        directional: vec![REFERENCE_LIGHT],
        ambient,
    }
}

/// The frame's canvas: `canvas`, else [`CANVAS`]. Each side must be finite,
/// positive, and at most Celesta's `MAX_FRAME_CANVAS_SIDE`.
pub(crate) fn frame_canvas(canvas: Option<[f32; 2]>) -> Result<[f32; 2], FrameError> {
    let Some(canvas) = canvas else {
        return Ok(CANVAS);
    };
    if canvas
        .iter()
        .all(|side| side.is_finite() && *side > 0.0 && *side <= MAX_FRAME_CANVAS_SIDE)
    {
        Ok(canvas)
    } else {
        Err(FrameError(format!(
            "data canvas is {canvas:?}: each side must be finite, positive, and at most {MAX_FRAME_CANVAS_SIDE}"
        )))
    }
}

/// The camera `data` asks for, and its eye: a preset framed on a canvas of
/// `aspect` (width over height) from `framing` and the humanoid's bones at
/// `rest` (world matrices by node), or an explicit camera.
pub(crate) fn camera(
    data: Option<&CameraData>,
    framing: &Framing,
    humanoid: &Humanoid,
    rest: &[Mat4],
    aspect: f32,
) -> Result<(Camera, Vec3), FrameError> {
    match data {
        None => preset(CameraPreset::Full, framing, humanoid, rest, aspect),
        Some(CameraData::Preset(name)) => preset(*name, framing, humanoid, rest, aspect),
        Some(CameraData::Explicit(camera)) => explicit(camera),
    }
}

/// A preset's camera: the span it frames, from the bones at rest, fit
/// on a canvas of `aspect`.
fn preset(
    name: CameraPreset,
    framing: &Framing,
    humanoid: &Humanoid,
    rest: &[Mat4],
    aspect: f32,
) -> Result<(Camera, Vec3), FrameError> {
    let at = |bone: HumanBone| rest[humanoid.required(bone)].w_axis.truncate();
    let (min, max) = (framing.bounds_min, framing.bounds_max);
    let head = at(HumanBone::Head);
    let (x, bottom) = match name {
        CameraPreset::Full => ((min.x + max.x) / 2.0, min.y),
        CameraPreset::Bust => (head.x, at(HumanBone::Spine).y),
        CameraPreset::Face => {
            let bottom = match humanoid.node(HumanBone::Neck) {
                Some(neck) => rest[neck].w_axis.y,
                None => head.y - (max.y - head.y) / 2.0,
            };
            (head.x, bottom)
        }
    };
    let top = max.y;
    let half_fov = (FOV / 2.0).to_radians().tan();
    // The height fits from the head's z; `full`'s width from the bounds'
    // front, where its widest points nearest the camera are.
    let mut z = head.z + MARGIN * (top - bottom) / 2.0 / half_fov;
    if name == CameraPreset::Full {
        z = z.max(max.z + MARGIN * (max.x - min.x) / 2.0 / (half_fov * aspect));
    }
    let eye = Vec3::new(x, (bottom + top) / 2.0, z);
    let name = name.name();
    if !eye.is_finite() {
        return Err(FrameError(format!(
            "the {name} camera's eye {eye} is not finite: the avatar at rest is past f32's range"
        )));
    }
    if top <= bottom {
        return Err(FrameError(format!(
            "the {name} camera frames nothing: the avatar's top ({top}) is not above where it starts ({bottom})"
        )));
    }
    Ok((
        Camera {
            view: Mat4::from_translation(-eye).to_cols_array(),
            projection: projection(FOV, NEAR, FAR),
        },
        eye,
    ))
}

/// An explicit camera, its numbers checked.
fn explicit(camera: &ExplicitCamera) -> Result<(Camera, Vec3), FrameError> {
    let position = Vec3::from(camera.position);
    let target = Vec3::from(camera.target);
    for (name, point) in [("position", position), ("target", target)] {
        if !point.is_finite() {
            return Err(FrameError(format!("camera {name} is not finite: {point}")));
        }
    }
    let Some(direction) = (target - position).try_normalize() else {
        return Err(FrameError(format!(
            "camera position and target are the same point, {position}, so it looks nowhere"
        )));
    };
    let fov = camera.fov.unwrap_or(FOV);
    if !(fov > 0.0 && fov < 180.0) {
        return Err(FrameError(format!(
            "camera fov is {fov}: degrees between 0 and 180"
        )));
    }
    let (near, far) = (camera.near.unwrap_or(NEAR), camera.far.unwrap_or(FAR));
    if !(near > 0.0 && near < far && far.is_finite()) {
        return Err(FrameError(format!(
            "camera near is {near} and far {far}: they must be finite, with 0 < near < far"
        )));
    }
    let up = if direction.abs_diff_eq(Vec3::Y, 1e-6) || direction.abs_diff_eq(Vec3::NEG_Y, 1e-6) {
        Vec3::NEG_Z
    } else {
        Vec3::Y
    };
    Ok((
        Camera {
            view: glam::camera::rh::view::look_to_mat4(position, direction, up).to_cols_array(),
            projection: projection(fov, near, far),
        },
        position,
    ))
}

fn projection(fov: f32, near: f32, far: f32) -> Projection {
    Projection::Perspective {
        yfov: fov.to_radians(),
        near,
        far,
    }
}

/// The lights `data` asks for: the reference ones, or its directional
/// lights and its ambient as the constant coefficient alone.
pub(crate) fn lights(data: Option<&LightsData>) -> Lights {
    let Some(data) = data else {
        return reference_lights();
    };
    let mut ambient = [[0.0; 3]; 9];
    ambient[3] = data.ambient;
    Lights {
        directional: data.directional.clone(),
        ambient,
    }
}

/// Why `data`'s lights cannot be drawn: a number that is not finite, or a
/// light without a direction. Celesta checks how many there are.
pub(crate) fn check_lights(data: &LightsData) -> Result<(), FrameError> {
    for (index, light) in data.directional.iter().enumerate() {
        let DirectionalLight {
            direction,
            color,
            intensity,
        } = light;
        let named: [(&str, &[f32]); 3] = [
            ("direction", direction),
            ("color", color),
            ("intensity", std::slice::from_ref(intensity)),
        ];
        for (name, values) in named {
            if values.iter().any(|value| !value.is_finite()) {
                return Err(FrameError(format!(
                    "lights.directional[{index}].{name} is not finite: {values:?}"
                )));
            }
        }
        if *direction == [0.0; 3] {
            return Err(FrameError(format!(
                "lights.directional[{index}].direction is [0, 0, 0], which points nowhere"
            )));
        }
    }
    if data.ambient.iter().any(|value| !value.is_finite()) {
        return Err(FrameError(format!(
            "lights.ambient is not finite: {:?}",
            data.ambient
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VrmModel;
    use crate::provider::PerformanceData;
    use crate::synthetic::{SyntheticVrm, node};
    use celesta_plugin::SceneFrame;
    use serde_json::{Value, json};
    use tempfile::TempDir;

    fn load(glb: &[u8]) -> VrmModel {
        let cache = TempDir::new().unwrap();
        VrmModel::load_slice(glb, cache.path()).unwrap()
    }

    fn avatar() -> VrmModel {
        load(&SyntheticVrm::avatar().glb())
    }

    fn data(value: Value) -> PerformanceData {
        serde_json::from_value(value).unwrap()
    }

    fn frame(model: &mut VrmModel, value: Value) -> SceneFrame {
        model.frame(0.0, &data(value)).unwrap()
    }

    fn refusal(model: &mut VrmModel, value: Value) -> String {
        match model.frame(0.0, &data(value)) {
            Ok(_) => panic!("the frame was made"),
            Err(error) => error.to_string(),
        }
    }

    fn view(frame: &SceneFrame) -> Mat4 {
        Mat4::from_cols_array(&frame.camera.view)
    }

    /// The eye of a view that only translates, as the presets' do.
    fn preset_eye(frame: &SceneFrame) -> Vec3 {
        let view = view(frame);
        let turn = Mat4::from_mat3(glam::Mat3::from_mat4(view));
        assert!(turn.abs_diff_eq(Mat4::IDENTITY, 1e-6), "{view}");
        -view.w_axis.truncate()
    }

    fn near(actual: Vec3, expected: Vec3, tolerance: f32) -> bool {
        (actual - expected).abs().max_element() < tolerance
    }

    #[test]
    fn framing_is_the_visible_draws_at_rest() {
        let model = avatar();
        let framing = model.framing();
        assert!(near(framing.bounds_min, Vec3::new(-0.2, 0.0, -0.1), 1e-6));
        assert!(near(framing.bounds_max, Vec3::new(0.2, 1.6, 0.1), 1e-6));
        // The hidden quad does not count, wherever it is; seen, it does.
        let far_quad = |root: &mut Value| {
            root["nodes"][node::FIRST_PERSON_ONLY]["translation"] = json!([0.0, 3.0, 0.0])
        };
        let model = load(&SyntheticVrm::avatar().glb_with(far_quad));
        assert_eq!(*model.framing(), *avatar().framing());
        let model = load(&SyntheticVrm::avatar().glb_with(|root| {
            far_quad(root);
            root["extensions"]["VRMC_vrm"]["firstPerson"]["meshAnnotations"][0]["type"] =
                json!("both");
        }));
        assert!((model.framing().bounds_max.y - 3.05).abs() < 1e-6);
        // Skinned at rest: Body follows its joints, not its node.
        let model = load(&SyntheticVrm::avatar().glb_with(|root| {
            root["nodes"][node::BODY]["translation"] = json!([5.0, 0.0, 0.0]);
        }));
        assert_eq!(*model.framing(), *avatar().framing());
    }

    #[test]
    fn presets_frame_the_humanoid() {
        let mut model = avatar();
        let expected = [
            ("full", Vec3::new(0.0, 0.8, 3.2842047)),
            ("bust", Vec3::new(0.0, 1.3, 1.2315768)),
            ("face", Vec3::new(0.0, 1.45, 0.6157884)),
        ];
        // 16:9 and 9:16: the 0.44 m width does not bind.
        for canvas in [[1920.0, 1080.0], [1080.0, 1920.0]] {
            for (preset, eye) in expected {
                let frame = frame(&mut model, json!({ "canvas": canvas, "camera": preset }));
                let actual = preset_eye(&frame);
                assert!(near(actual, eye, 1e-4), "{preset} at {canvas:?}: {actual}");
                assert_eq!(
                    frame.camera.projection,
                    Projection::Perspective {
                        // 0.5235988, 30°.
                        yfov: std::f32::consts::FRAC_PI_6,
                        near: 0.01,
                        far: 100.0
                    }
                );
                assert_eq!(frame.canvas, Some(canvas));
            }
        }
        // The default is `full`, on the model's canvas.
        let default = frame(&mut model, json!({}));
        assert_eq!(default, frame(&mut model, json!({ "camera": "full" })));
        assert_eq!(default.canvas, Some(CANVAS));
        // At 1:10 the width binds, measured from the bounds' front.
        let frame = frame(&mut model, json!({ "canvas": [100.0, 1000.0] }));
        let eye = preset_eye(&frame);
        let z = 0.1 + 0.22 / (15f32.to_radians().tan() * 0.1);
        assert!((z - 8.3104).abs() < 1e-3, "{z}");
        assert!(near(eye, Vec3::new(0.0, 0.8, z), 1e-3), "{eye}");
    }

    #[test]
    fn face_without_a_neck_starts_half_the_head_below_it() {
        let mut vrm = SyntheticVrm::avatar();
        vrm.omit_bone = Some("neck".into());
        let mut model = load(&vrm.glb());
        // Head 1.4, top 1.6: from 1.3, as with the neck.
        let eye = preset_eye(&frame(&mut model, json!({ "camera": "face" })));
        assert!(near(eye, Vec3::new(0.0, 1.45, 0.6157884), 1e-4), "{eye}");
    }

    #[test]
    fn explicit_camera_looks_at_its_target() {
        let mut model = avatar();
        let frame = frame(
            &mut model,
            json!({ "camera": { "position": [0, 1.5, 2], "target": [0, 1.5, 0], "fov": 40 } }),
        );
        assert!(
            view(&frame).abs_diff_eq(Mat4::from_translation(Vec3::new(0.0, -1.5, -2.0)), 1e-6),
            "{}",
            view(&frame)
        );
        assert_eq!(
            frame.camera.projection,
            Projection::Perspective {
                yfov: 0.6981317,
                near: 0.01,
                far: 100.0
            }
        );
        // Straight down: up is −Z.
        let frame = super::tests::frame(
            &mut model,
            json!({ "camera": { "position": [0, 3, 0], "target": [0, 0, 0], "near": 0.5, "far": 10 } }),
        );
        let view = view(&frame);
        assert!(view.is_finite(), "{view}");
        let down = view.transform_vector3(Vec3::NEG_Y);
        assert!(near(down, Vec3::NEG_Z, 1e-6), "{down}");
        assert_eq!(
            frame.camera.projection,
            Projection::Perspective {
                yfov: std::f32::consts::FRAC_PI_6,
                near: 0.5,
                far: 10.0
            }
        );
    }

    #[test]
    fn bad_explicit_cameras_are_refused() {
        let mut model = avatar();
        let camera = |extra: Value| {
            let mut camera = json!({ "position": [0, 1.5, 2], "target": [0, 1.5, 0] });
            camera.as_object_mut().unwrap().extend(
                extra
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
            json!({ "camera": camera })
        };
        let cases = [
            (
                json!({ "target": [0, 1.5, 2] }),
                "camera position and target",
            ),
            (json!({ "fov": 0 }), "fov"),
            (json!({ "fov": 180 }), "fov"),
            (json!({ "fov": 1e39 }), "fov"),
            (json!({ "near": 0 }), "near"),
            (json!({ "near": 2, "far": 1 }), "near"),
            (json!({ "far": 1e39 }), "far"),
            // 1e39 is past f32: it reads as infinity.
            (json!({ "position": [0, 1e39, 2] }), "camera position"),
            (json!({ "target": [1e39, 0, 0] }), "camera target"),
        ];
        for (extra, expected) in cases {
            let message = refusal(&mut model, camera(extra.clone()));
            assert!(message.contains(expected), "{extra}: {message}");
        }
    }

    #[test]
    fn bad_canvases_and_lights_are_refused() {
        let mut model = avatar();
        for canvas in [
            json!([0, 1080]),
            json!([1920, -1]),
            json!([20000, 10]),
            json!([1e39, 1]),
        ] {
            let message = refusal(&mut model, json!({ "canvas": canvas }));
            assert!(message.contains("canvas"), "{canvas}: {message}");
        }
        let light = |light: Value| json!({ "lights": { "directional": [light] } });
        let cases = [
            (
                light(json!({ "direction": [0, 0, 0], "color": [1, 1, 1], "intensity": 1 })),
                "lights.directional[0].direction",
            ),
            (
                light(json!({ "direction": [0, 1e39, 0], "color": [1, 1, 1], "intensity": 1 })),
                "lights.directional[0].direction",
            ),
            (
                light(json!({ "direction": [0, -1, 0], "color": [1e39, 1, 1], "intensity": 1 })),
                "lights.directional[0].color",
            ),
            (
                light(json!({ "direction": [0, -1, 0], "color": [1, 1, 1], "intensity": 1e39 })),
                "lights.directional[0].intensity",
            ),
            (
                json!({ "lights": { "ambient": [0, 1e39, 0] } }),
                "lights.ambient",
            ),
        ];
        for (value, expected) in cases {
            let message = refusal(&mut model, value.clone());
            assert!(message.contains(expected), "{value}: {message}");
        }
    }

    #[test]
    fn reference_lights_by_default() {
        let mut ambient = [[0.0; 3]; 9];
        ambient[3] = [0.0368895, 0.0423114, 0.0544803];
        let reference = Lights {
            directional: vec![DirectionalLight {
                direction: [-0.3213938, -0.7660444, -0.5566704],
                color: [1.0, 0.9046612, 0.6724432],
                intensity: 1.0,
            }],
            ambient,
        };
        assert_eq!(lights(None), reference);
        assert_eq!(reference_lights(), reference);
        let given = LightsData {
            directional: Vec::new(),
            ambient: [0.1, 0.2, 0.3],
        };
        let lit = lights(Some(&given));
        assert!(lit.directional.is_empty());
        for (k, coefficient) in lit.ambient.iter().enumerate() {
            let expected = if k == 3 { [0.1, 0.2, 0.3] } else { [0.0; 3] };
            assert_eq!(*coefficient, expected, "ambient[{k}]");
        }
        // Through a frame.
        let mut model = avatar();
        assert_eq!(frame(&mut model, json!({})).lights, reference);
        let light = json!({ "direction": [0, -1, 0], "color": [1, 0.5, 0.25], "intensity": 2 });
        let lit = frame(&mut model, json!({ "lights": { "directional": [light] } })).lights;
        assert_eq!(
            lit.directional,
            [DirectionalLight {
                direction: [0.0, -1.0, 0.0],
                color: [1.0, 0.5, 0.25],
                intensity: 2.0,
            }]
        );
        assert_eq!(lit.ambient, [[0.0; 3]; 9]);
    }

    #[test]
    fn data_as_vrm_sends_it() {
        // `vrmData`'s keys and units (plugins/vrm/js/src/index.ts).
        let mut model = avatar();
        let frame = frame(
            &mut model,
            json!({
                "canvas": [1080, 1920],
                "expressions": { "blink": 0.5, "happy": 1 },
                "lookAt": "camera",
                "camera": "face",
                "lights": {
                    "directional": [{ "direction": [1, 0, 0], "color": [1, 1, 1], "intensity": 1 }],
                    "ambient": [1, 1, 1],
                },
            }),
        );
        assert_eq!(frame.canvas, Some([1080.0, 1920.0]));
        assert!(near(
            preset_eye(&frame),
            Vec3::new(0.0, 1.45, 0.6157884),
            1e-4
        ));
        assert_eq!(frame.lights.ambient[3], [1.0; 3]);
        let message = match serde_json::from_value::<PerformanceData>(json!({ "camera": "side" })) {
            Ok(_) => panic!("the data was read"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("camera is \"full\""), "{message}");
    }
}
