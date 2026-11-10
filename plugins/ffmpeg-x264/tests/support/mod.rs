use celesta_media::MediaProcess;
use std::path::PathBuf;

pub fn executable(id: &str) -> PathBuf {
    let name = format!("celesta-codec-{id}{}", std::env::consts::EXE_SUFFIX);
    PathBuf::from(env!("CARGO_BIN_EXE_celesta-codec-ffmpeg-x264")).with_file_name(name)
}

pub fn process(id: &str) -> MediaProcess {
    let executable = executable(id);
    MediaProcess::start(
        &executable,
        executable.parent().unwrap(),
        &Default::default(),
    )
    .unwrap()
}
