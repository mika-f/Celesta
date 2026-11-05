//! The temporary plugin folder a test gives a child process, so the child
//! reads and writes media with the test codec's program.

use celesta_codec_test::{PLUGIN_ID, PROGRAM, TestCodecPlugin, profile_dir};
use celesta_media::CodecProcess;
use celesta_plugin_host::InstalledPlugins;
use std::path::Path;

#[test]
fn the_folder_is_a_codec_plugin_for_this_platform() {
    let plugin = TestCodecPlugin::new();
    let folder = plugin.folder();

    let (installed, errors) = InstalledPlugins::find(&[folder.to_path_buf()]);
    assert!(errors.is_empty(), "{errors:?}");
    let codec = installed.codec().expect("the folder's codec");
    assert_eq!(codec.id, PLUGIN_ID);
    assert!(
        codec.executable.starts_with(folder),
        "{}",
        codec.executable.display()
    );
    assert!(
        codec
            .executable
            .ends_with(format!("{PROGRAM}{}", std::env::consts::EXE_SUFFIX)),
        "{}",
        codec.executable.display()
    );
    // The program in the folder runs and answers as a codec.
    CodecProcess::start(&codec.executable, &codec.directory).unwrap();
}

#[test]
fn a_missing_program_says_to_build_it() {
    let empty = tempfile::tempdir().unwrap();
    let error = TestCodecPlugin::in_profile(empty.path()).unwrap_err();
    assert!(
        error.contains("cargo build -p celesta-codec-test"),
        "{error}"
    );

    assert_eq!(
        profile_dir(Path::new("/t/debug/deps/x")),
        Path::new("/t/debug")
    );
    assert_eq!(profile_dir(Path::new("/t/debug/x")), Path::new("/t/debug"));
}
