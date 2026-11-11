use super::executable;
use celesta_media::{MediaProcess, RegisteredPlugin};
pub fn plugins() -> Vec<RegisteredPlugin> {
    [
        celesta_codec_ffmpeg::registered(),
        celesta_codec_ffmpeg_x264::registered(),
    ]
    .into_iter()
    .map(|mut plugin| {
        let executable = executable(&plugin.id);
        let directory = executable.parent().unwrap().to_path_buf();
        plugin.factory = MediaProcess::factory(executable, directory);
        plugin
    })
    .collect()
}
