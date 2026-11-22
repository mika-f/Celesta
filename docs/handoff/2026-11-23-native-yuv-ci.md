# Native YUV: PR 888 CI and colour review

## Scope

Watch PR 888, fix branch-caused compiler/runtime failures, and never loosen
exact-plane or own-RGBA max <=4 gates. Stop for a pixel-gate failure.
Also restrict AVFoundation and Media Foundation NV12 primaries/transfer
to compatible BT.601/709 or absent metadata; P3 and linear stay RGBA.

## Signed fixes

- `4c451e693ef4391add13f10613c0d7714cef58a2`: colour eligibility,
  portable metadata/VUI tests, P3/linear native fallback fixtures, guides.
- `82fc482b218aabaf2a324a86ca566502e75b2861`: restrict the stale
  record_units dead-code expectation to non-Windows builds and print
  labelled exact-plane max/mean/differing-byte metrics before asserting.

Both commits were recreated through createCommitOnBranch, verified valid,
and pushed with matching trees. The original review head cff8ed5 was
checked before edits. No review comments were answered.

## Windows runtime follow-up

On 82fc482b, Windows Plugins job 137304265525 completed Clippy, then ran
139 Media Foundation tests: 136 passed, 3 failed. These failed before
any plane/RGBA comparison, so no pixel-gate numbers were produced:

```
Codec("SetCurrentMediaType(NV12) failed: 0xC00D36B4")
Unsupported { path: "...odd.avi", tried: [] }
```

The NV12 selector submitted colour attributes on a partial output type.
It now discovers the complete NV12 output type with the existing
major/subtype selector first, then applies source-consistent colour
attributes and renegotiates. Processing remains disabled; the final
negotiated attributes and delivered sample subtype are still verified.
Whether this resolves the native rejection is left to the next CI run,
not inferred from Linux API checking.

The raw odd-sized AVI was refused before decoding. Replace it with an
odd-sized MJPEG AVI (63x35), generated using linked libavcodec. Keep all
dimension, RGBA fallback and both-transport equality assertions unchanged;
do not skip the test. Native support is likewise checked by the next run.

## Checks

Mac Plugins job 137304265702 on 82fc482b: AVFoundation 89/89 passed,
including exact H.264/HEVC planes, own-RGBA gates and P3/linear fallback.
Nextest hides passing-test stdout, so no max/mean/count table is available.
All other checks on that head passed; only Windows Plugins failed.
Devin Review completed; git_view_pr plus review/review-comment streams
showed no published findings beyond the relayed colour finding.

Linux review-fix tests: AVFoundation 25 passed; Media Foundation 69 passed.
For the runtime follow-up, a fixture-only audit decoded the new AVI with
FFmpeg and asserted its 63x35 dimensions (1 passed), and checked P3/linear
tags. The actual helper passed Clippy with Linux scaffolding. The Windows
decoder passed an isolated Windows-target API/Clippy audit with a public
entry point keeping decode reachable. These are not native runtime tests.
Plugin formatting and git diff whitespace checks passed. Native gate
results after the follow-up are deferred to CI; do not claim Linux parity.
