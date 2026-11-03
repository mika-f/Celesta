//! The VRM plugin as Celesta runs it: the plugin host finds it through a
//! plugin directory, starts its process, and gets the same answers as the
//! provider gives in-process.

use celesta_plugin::{
    ContentFrame, ContentModel, ContentProvider, PROTOCOL_VERSION, TextureSource,
};
use celesta_plugin_host::PluginHost;
use celesta_plugin_vrm::synthetic::SyntheticVrm;
use celesta_plugin_vrm::{PLUGIN, VrmProvider};
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

#[test]
fn the_process_answers_as_the_provider_does() {
    let files = tempfile::tempdir().unwrap();
    let path = files.path().join("avatar.vrm");
    std::fs::write(&path, SyntheticVrm::avatar().glb()).unwrap();
    let assets: BTreeMap<String, PathBuf> = [("model".to_owned(), path)].into();
    // Both write textures to one fresh cache, never the user's: the
    // process by the variable it inherits, the provider by being told.
    // The scenes name the files, so they must be in the same place.
    let cache = tempfile::tempdir().unwrap();
    // SAFETY: this binary's only test sets it before starting the process,
    // and no other thread reads the environment.
    unsafe { std::env::set_var("CELESTA_VRM_TEXTURE_CACHE", cache.path()) };

    let directory = plugin_directory();
    let mut host = PluginHost::discover(&[directory.path().to_path_buf()]);
    let mut provider = VrmProvider::with_texture_cache(cache.path());
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
        assert!(path.starts_with(cache.path()), "{}", path.display());
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
