# Unified Celesta CLI and MP4 in Docker

The headless image accepts `doctor`, `init`, `plugin`, `info`, `templates`,
and `export` directly after the image name. The entrypoint forwards command
names and top-level help/version to `celesta` and other arguments to
`celesta-exporter`, preserving positional and flag-first export commands.
If an input's bare name matches a command, use an explicit path or `export`.
The tag and `/work` mount are unchanged.

The app and standard LGPL FFmpeg plugin keep their Ubuntu 26.04 build.
A separate stage on Ubuntu 22.04 uses the Linux release packager to build
`ffmpeg-x264` against its own GPL FFmpeg prefix, and copies the complete
plugin folder into the final image. The plugin carries libx264, the GPL
text and corresponding-source offer, and the exact Ubuntu binary/source
package versions and copyright notices. The app does not link either
FFmpeg build. The desktop packages and separate codec downloads are unchanged.

## Checks

The existing manual Linux exporter image CI job builds the image, validates
both codec builds and the x264 provenance, and runs `init`, `plugin --help`,
and a `doctor` MP4 dry run. It checks PNG, MOV, WebM, H.264/MP4 (including the
encoder ID in the JSON report) and positional/flag-first legacy exports.
Only the project folder is mounted; there is no codec ZIP dependency.

The local x86-64 build and those smoke checks passed using software Vulkan
(llvmpipe). The MP4 report named `ffmpeg-x264/libx264` (`avc1`), and a JSON
project also exported to MP4 with only the two legacy positional arguments.
