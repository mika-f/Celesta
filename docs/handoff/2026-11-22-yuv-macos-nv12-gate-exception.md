# macOS arm64 NV12 swscale gate exception

Owner option 1 for PR #885, branch `devin/1795326315-yuv-decode`:
omit only `yuv-native-nv12.nut`'s swscale comparison, only on macOS aarch64.
The compile-time guard is immediately before `diff` in
`native_yuv_meets_the_rgba_pixel_gate_over_both_transports`. Both native decode
transports and format assertions remain exercised. The separate max-0
independent-reference test is unchanged and still includes NV12 on every
platform. No production conversion or RGBA path was changed.

**Unconfirmed:** FFmpeg's aarch64 NEON NV12→RGBA path appears to skip vertical
chroma reconstruction; CI job `137199105972` measured max 76. This is not
native dispatch instrumentation. The issue and narrow owner-approved exception
are also recorded under Known issues in `2026-11-22-yuv-decode.md`.

Local validation uses the FFmpeg plugin runner with FFmpeg 8.1.3, `--locked`:
tests, all-target Clippy with `-D warnings`, root/plugin formatting checks, and
`git diff --check`. Native macOS execution is delegated to PR CI; Linux keeps
the NV12 swscale comparison enabled. Logs live under
`/home/ubuntu/research/yuv-gate/macos-nv12-exception-{tests,clippy}.log`.

Results: all 107 plugin tests pass, including 9 YUV tests; NV12 independent
reference is max/mean/differing 0/0/0, while Linux swscale remains enabled and
reports max 3 / mean 0.372541 / differing 6968 over each transport. Clippy:
`Finished dev profile [unoptimized + debuginfo] target(s) in 0.36s`.
Formatting and diff checks exit 0. Signing, CI and review results are reported
after publication.
