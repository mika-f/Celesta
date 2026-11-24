# Linux Docker plugin permissions

The Linux exporter image must load the bundled FFmpeg plugin for MOV
export. The build script copies plugin files with their source modes, so
a group-writable checkout can stage a license such as `LICENSE-APACHE`
with mode 0664. On Unix the host checks the entire bundled plugin tree
and rejects group- or other-writable entries, including licenses.

In `packaging/linux/Dockerfile`, remove group and other write permissions
recursively from `/opt/celesta/plugins` after the native-library license
files are added. This keeps owner permissions and the codec executable's
execute bits as staged. Plugin host checks and other packages are unchanged.

The manually dispatched Linux exporter image job sets `LICENSE-APACHE`
to 0664 before building so its MOV smoke test exercises this case even
on a checkout with mode 0644. The test exports a one-frame React fixture
using the image's entrypoint and no explicit plugin path.
Check the completed image's plugin tree for bits matching `022`, and use
`ffprobe` to confirm the output is a one-frame ProRes MOV.

Verified with `docker build --progress=plain -f packaging/linux/Dockerfile -t celesta-exporter:plugin-perms .`.
The checkout's license was 0664 and the image's copy was root-owned 0644.
Restoring group write in a disposable container caused the host to reject
the plugin and MOV export to fail. An unmodified container running as
UID/GID 1000 exported successfully with `ffmpeg/mov` and `ffmpeg/prores`;
`ffprobe` reported one 64 × 64 ProRes frame (`ap4h`).
`cargo test --locked -j 2 -p celesta-plugin-host` passed all 185 tests.
