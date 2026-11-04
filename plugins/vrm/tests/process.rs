//! The VRM plugin as Celesta runs it: the plugin host finds it through a
//! plugin directory, starts its process, and gets the same answers as the
//! provider gives in-process.
//!
//! The child finds its texture cache by the variable it inherits, which
//! only `set_var` can give it: the host passes a plugin no environment of
//! its own. `set_var` is sound only while no other thread reads the
//! environment, so this file has no test harness (`harness = false` in
//! `Cargo.toml`): `main` sets the variable before any thread exists, then
//! runs the check. A failed check panics, which fails the test.

use celesta_plugin::{
    ContentFrame, ContentModel, ContentProvider, PROTOCOL_VERSION, TextureSource,
};
use celesta_plugin_host::PluginHost;
use celesta_plugin_vrm::synthetic::{SyntheticVrm, SyntheticVrma, node};
use celesta_plugin_vrm::{PLUGIN, VrmProvider};
use glam::{Mat4, Quat};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A plugin directory holding this build's executable.
fn plugin_directory() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let plugin = directory.path().join(PLUGIN);
    std::fs::create_dir(&plugin).unwrap();
    // A manifest names files inside its directory only.
    let executable = Path::new(env!("CARGO_BIN_EXE_celesta-plugin-vrm"));
    let name = executable.file_name().unwrap().to_str().unwrap();
    std::fs::copy(executable, plugin.join(name)).unwrap();
    let (os, arch) = celesta_plugin_host::platform();
    std::fs::write(
        plugin.join("celesta-plugin.json"),
        serde_json::json!({
            "id": PLUGIN,
            "version": "0.1.0",
            "protocol": PROTOCOL_VERSION,
            "executable": { format!("{os}-{arch}"): name },
        })
        .to_string(),
    )
    .unwrap();
    directory
}

fn main() {
    // Both write textures to one fresh cache, never the user's: the
    // process by the variable it inherits, the provider by being told.
    // The scenes name the files, so they must be in the same place.
    let cache = tempfile::tempdir().unwrap();
    // SAFETY: without a test harness, `main` is the only thread, so
    // nothing reads the environment while it is set.
    unsafe { std::env::set_var("CELESTA_VRM_TEXTURE_CACHE", cache.path()) };
    the_process_answers_as_the_provider_does(cache.path());
    println!("test the_process_answers_as_the_provider_does ... ok");
    motions_cross_the_process(cache.path());
    println!("test motions_cross_the_process ... ok");
}

fn the_process_answers_as_the_provider_does(cache: &Path) {
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("avatar.vrm");
    std::fs::write(&path, SyntheticVrm::avatar().glb()).unwrap();
    let assets: BTreeMap<String, PathBuf> = [("model".to_owned(), path)].into();

    let directory = plugin_directory();
    let mut host = PluginHost::discover(&[directory.path().to_path_buf()]);
    let mut provider = VrmProvider::with_texture_cache(cache);
    let remote = host.open(PLUGIN, &assets).unwrap();
    let local = provider.open(PLUGIN, &assets).unwrap();
    assert!(
        matches!(remote.content, ContentModel::Scene(_)),
        "{:?}",
        remote.content
    );
    assert_eq!(remote.content, local.content);
    let ContentModel::Scene(scene) = &remote.content else {
        unreachable!("checked above");
    };
    for texture in &scene.textures {
        let TextureSource::Path(path) = &texture.source else {
            panic!("a VRM texture is a file: {:?}", texture.source);
        };
        assert!(path.starts_with(cache), "{}", path.display());
    }
    assert_eq!(remote.notices, local.notices);
    assert!(!remote.notices.is_empty());

    let (remote, local) = (remote.model, local.model);
    let data = serde_json::json!({
        "expressions": { "happy": 0.5 },
        "lookAt": { "yaw": 10, "pitch": 0 },
        "camera": "bust",
    });
    // Out of order, as a renderer may ask.
    for time in [2.0, 0.0, 2.0] {
        let frame = host.frame(remote, time, &data).unwrap();
        assert!(matches!(frame, ContentFrame::Scene(_)), "{frame:?}");
        assert_eq!(frame, provider.frame(local, time, &data).unwrap());
    }
}

/// Motions and spring bones answered alike by the process and in process,
/// and by a fresh process asked the later frames only, as after a restart:
/// a frame depends on its time and data, never on what was asked before.
fn motions_cross_the_process(cache: &Path) {
    let mut vrm = SyntheticVrm::avatar();
    let joint = |node: usize| serde_json::json!({ "node": node, "stiffness": 0.5, "dragForce": 0.4, "gravityPower": 1.0 });
    vrm.extensions.insert(
        "VRMC_springBone".into(),
        serde_json::json!({
            "specVersion": "1.0",
            "springs": [{ "joints": [joint(node::HAIR0), joint(node::HAIR1), joint(node::HAIR2)] }],
        }),
    );
    let files = tempfile::tempdir().unwrap();
    let model = files.path().join("avatar.vrm");
    // The hair nodes are no skin's joints, so a second skin of them, at
    // identity inverse binds, sends their worlds in each frame.
    let glb = vrm.glb_with(|root| {
        root["skins"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({ "joints": [node::HAIR0, node::HAIR1, node::HAIR2] }));
    });
    std::fs::write(&model, glb).unwrap();
    let clip = files.path().join("clip.vrma");
    std::fs::write(&clip, SyntheticVrma::clip().glb()).unwrap();
    let assets: BTreeMap<String, PathBuf> =
        [("model".to_owned(), model), ("motion0".to_owned(), clip)].into();
    let data = serde_json::json!({ "motions": [{ "clip": "motion0", "loop": true }] });

    let directory = plugin_directory();
    let mut host = PluginHost::discover(&[directory.path().to_path_buf()]);
    let mut provider = VrmProvider::with_texture_cache(cache);
    let remote = host.open(PLUGIN, &assets).unwrap().model;
    let local = provider.open(PLUGIN, &assets).unwrap().model;
    let mut frames = BTreeMap::new();
    for time in [2.0, 0.25, 2.0] {
        let frame = host.frame(remote, time, &data).unwrap();
        assert_eq!(frame, provider.frame(local, time, &data).unwrap());
        frames.insert(time.to_bits(), frame);
    }
    // The hair swung: `hair1` turned from its rest in `hair0`'s frame.
    let ContentFrame::Scene(scene) = &frames[&2.0f64.to_bits()] else {
        panic!("a VRM frame is a scene");
    };
    let rotation = |joint: usize| {
        let world = Mat4::from_cols_array(&scene.skins[1][joint]);
        world.to_scale_rotation_translation().1
    };
    let local = rotation(0).inverse() * rotation(1);
    assert!(
        local.angle_between(Quat::IDENTITY) > 1e-3,
        "the hair did not swing: {local:?}"
    );
    drop(host);

    let mut restarted = PluginHost::discover(&[directory.path().to_path_buf()]);
    let remote = restarted.open(PLUGIN, &assets).unwrap().model;
    for time in [0.25, 2.0] {
        let frame = restarted.frame(remote, time, &data).unwrap();
        assert_eq!(
            frame,
            frames[&time.to_bits()],
            "at {time} s after a restart"
        );
    }
}
