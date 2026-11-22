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

## Implementation of option 2 trial

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

Windows/macOS CI determines whether option 2 succeeds or option 1 is needed.
The final handoff response records signed SHA, metrics, chosen option and all
settled checks. Native before/after screenshots remain unavailable on Linux;
this step changes test policy/fixtures, not production rendering behavior.
