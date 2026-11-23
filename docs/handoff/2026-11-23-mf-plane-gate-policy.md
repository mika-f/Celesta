# PR 888: MF plane gate policy and 320x180 fixtures

## Owner specification

1. Remove the `skip_loop_filter=all` reference. MF's plane gate again compares
   exactly with normal FFmpeg only. No MF quality investigation.
2. Regenerate MF's 709-limited, untagged, and 709-full H.264 fixtures at 320x180
   instead of 64x36. If every frame matches exactly on Windows, retain the
   exact gate on all fixtures (option 2).
3. If 320x180 still differs, use option 1: retain the normal-FFmpeg exact plane
   gate for 1080p only. Small fixtures retain logged plane differences and the
   mandatory own-RGBA max <=4/channel plus exact-alpha gate. Document results.
4. Confirm macOS stays green. New commits, push, signed recreation, all CI.

## Implementation (option 2 retained)

- `plugins/mediafoundation/tests/support/yuv_decode.rs`: removes the alternate
  decoder/reference/selector tests, restoring the original normal-FFmpeg
  exact comparison and metadata/dimension checks. Only this test's H.264
  generation call changes to 320x180; other decode/fallback fixtures retain
  their dimensions. The generated size is independently asserted via FFmpeg.
- Every one of the eight 4fps frames now receives an NV12 comparison. Additional
  requests cover frames initially tested with RGBA-only/I420-only/empty offers,
  retaining offer changes, prefetch, seeks/EOF, and both transports. Plane and
  own-RGBA logs identify dimensions, transport, fixture and requested time.
- The two native test helpers generate independent fixtures; no shared assets
  or fixture generator exists between AVFoundation and MF. AVFoundation's
  fixtures and normal-only exact gate remain unchanged.
- Production code and the own-RGBA/alpha gate are untouched. The scoped nextest
  successful-output setting from the previous commit remains, exposing these
  native metrics in CI.

## Evidence before this trial

At 64x36, the first MF frame differed from normal FFmpeg by max 4, mean
0.638310185, 1632 bytes, and exactly matched the no-loop-filter reference.
At t=0.25 neither reference matched; normal-reference max 4, mean 0.476273148,
1359 bytes. Job `137329508126` confirms that accepting either reference does
not resolve the full sequence. The 1080p frame matched normal FFmpeg exactly
over both transports, own-RGBA max 2, alpha exact. Earlier diagnostics found
a single decoder, no inserted video processor/resizer, and correct cropping
from the 64x48 coded allocation to 64x36. The owner's fallback policy treats
small-frame differences as decoder-dependent rather than adding a tolerance
or further alternate decoder modes to the plane gate.

## Checks and outcome

First Windows trial (`e90cf225`, job `137333823771`): every limited-range frame
at 320x180 matched normal FFmpeg exactly, max 0, mean 0, differing bytes 0,
over both transports. Own-RGBA max 2, alpha exact. The test then encountered
`unexpected Media Foundation NV12 type, color or aperture` opening the untagged
fixture, before any plane comparison. This is not a plane mismatch and does
not yet trigger option 1. The error now includes the negotiated type/color
values to identify the failing validation, and test errors identify the path,
transport and time. No decoder behavior or quality setting is changed.

The diagnostic run (`876dd70b`, job `137336972772`) identifies the untagged
current type as NV12, 320x192, matrix absent, range 2, chroma 5; expected
BT.601/limited/left. The existing response metadata parser maps an absent
matrix to BT.601, matching FFmpeg's untagged rule, but the preceding validation
required an explicit value 2. Validation now uses the same untagged rule:
absent/2 matches BT.601, 1 matches BT.709, other explicit values and mismatches
fail. Portable tests cover missing, supported, mismatched and unsupported
values. Range/chroma/subtype/aperture validation and every pixel gate remain
unchanged. This fixes the branch's untagged negotiation rejection; it does not
change pixel data or decoder settings.

## Final result: option 2

Signed code commit `faa878a39d619cf4d1dc78f4bf1fda56da1d3d5b` passed all CI:
10 passed, 0 failed, 0 pending, 5 skipped. Windows plugin job `137340986159`
reports **141 MF tests passed**. macOS plugins (`137340986015`) and root tests
(`137340985959`) passed with unchanged AVFoundation fixtures/exact gates.

All three 320x180 variants match normal FFmpeg exactly on every frame, both
transports, with prefetch and the offer-change/seek/EOF requests. There are 12
NV12 comparisons per variant per transport (all eight frames plus repeats and
EOF clamping), **72 comparisons**, each max 0, mean 0, differing bytes 0.
Both 1080p comparisons are also exact. **Option 1 is not needed or implemented:
the normal-FFmpeg exact plane gate remains on every eligible MF fixture.**

The mandatory own-RGBA/exact-alpha gate passes every comparison. Across each
variant's requests, the observed worst RGB maximum/mean/differing-pixel count
(the individual maxima need not refer to the same request) are:

| Fixture | Max RGB | Largest mean | Largest differing-pixel count |
| --- | ---: | ---: | ---: |
| 320x180 709 limited | 2 | 0.084699074 | 13187 |
| 320x180 untagged | 2 | 0.090659722 | 13954 |
| 320x180 709 full | 2 | 0.058356481 | 9207 |
| 1080p | 2 | 0.055932034 | 326878 |

Alpha is exact throughout. Optional HEVC/10-bit and unsupported ProRes/odd-size
capability skips remain as before; no skipped fixture is claimed bit-exact.
There is no alternate loop-filter reference and no MF quality-setting change.

Local checks after the matrix validation fix: MF portable library tests
**71 passed, 0 failed** (0.13s); MF all-target Clippy `-D warnings` completed
(0.33s); isolated actual Windows decoder/API test-config Clippy completed
(0.40s). Gate-helper Clippy/fixture-retag audit passed earlier after removing
the alternate reference. Formatting/diff checks exited 0. Isolated audits do
not establish native parity; the Windows CI results above do.

Logs, metrics and signing records are retained under
`/home/ubuntu/research/native-yuv/320-*`. The final handoff response records the
signed documentation HEAD and its settled checks. Native before/after
screenshots remain unavailable on Linux; no pixel conversion/rendering
behavior or fixture data changed, apart from the source-consistent untagged
validation fix and generated H.264 fixture dimensions described above.
