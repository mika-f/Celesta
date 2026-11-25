# YUV decode fixtures

The `yuv-*` files are synthetic `testsrc2` patterns generated for decode tests
using the installed FFmpeg 4.4.2 CLI and libx264/FFV1/rawvideo encoders. Tests
decode them with the plugin's linked FFmpeg 8.1 libraries, not the system CLI.
They contain no third-party media. Encoded fixtures are checked in so tests
do not require a separate encoder or CLI installation.

| File | Dimensions | Encoding / tags |
| --- | --- | --- |
| `yuv-709-limited.mp4` | 1920×1080 | one H.264 I420 frame, BT.709 limited |
| `yuv-untagged.mp4` | 1920×1080 | one H.264 I420 frame, untagged |
| `yuv-709-full.mp4` | 1920×1080 | one H.264 full-range frame, BT.709 |
| `yuv-odd-chroma.mp4` | 1918×1078 | one H.264 I420 frame, BT.709 limited |
| `yuv-odd-444.mp4` | 1919×1079 | one H.264 4:4:4 frame; RGBA fallback |
| `yuv-odd-i420.mkv` | 1919×1079 | one FFV1 I420 frame; RGBA fallback |
| `yuv-native-nv12.nut` | 128×72 | one raw NV12 frame, untagged |
| `yuv-sequence.mp4` | 128×72 | eight H.264 I420 frames at 4 fps, BT.709 limited |

To generate equivalent test inputs, use `-f lavfi -i
"testsrc2=size=WIDTHxHEIGHT:rate=RATE"` and `-frames:v N`. H.264 uses
`-c:v libx264 -crf 18 -pix_fmt yuv420p`; 4:4:4 uses `yuv444p`. Tag 709 with
`-colorspace bt709 -color_primaries bt709 -color_trc bt709`, and use
`-color_range tv` or `-color_range pc`. The odd I420 fixture instead uses
`-c:v ffv1 -pix_fmt yuv420p`. Raw NV12 uses
`-c:v rawvideo -pix_fmt nv12 -f nut`. Different encoders/versions may produce
different bytes; the test oracle always decodes each fixture's current RGBA
answer and compares against that, rather than storing an RGB golden image.

Libx264 cannot encode odd-width/height I420; the odd H.264 case deliberately
uses 4:4:4, and the separate FFV1 case verifies genuinely odd I420 fallback.

`covered-audio.mp3` is a short synthetic sine tone with a 16×16 PNG
embedded as ID3 cover art (`attached_pic`), not a video track.
