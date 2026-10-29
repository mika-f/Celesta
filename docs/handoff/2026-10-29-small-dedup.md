# Small duplicates and naming fixes (2026-10-29)

Issue #598, left over from the #569 sub-issues.

## Specification and implementation

- **`clamp01` / `clamp` in `packages/web`.** `filters.ts` imports both from
  `@celesta/math` instead of defining its own. The formulas were the same.
- **Keyframe interpolation.** `segment` in `crates/composition/src/animation.rs`
  turns a keyframe pair and a time into the two values and the eased progress,
  and `lerp` turns those into an `f64` (exactly `from` at progress 0).
  `keyframe_segment`, `evaluate_f64`, and `KeyframeCursor::evaluate` all go
  through them, so the cursor gives the same value as `evaluate_f64` by
  construction rather than by a copied formula.
- **Mixer gain.** `ClipGain` in `crates/media/src/mix.rs` replaces `DuckGain`.
  It holds the clip's volume, fade, and ducking cursor, and `at` returns
  `volume.clamp(0, 16) × fade × ducking` as `f32`. The plain and the stretched
  mixers (`mix.rs`, `stretch.rs`) both use it.
- **Audio cache key.** `crates/editor/src/audio_cache.rs` names entries with
  64-bit FNV-1a (the `fnv` crate, which `celesta-remote` already uses) over the
  canonical path's encoded bytes and then each identity field (length, device
  and inode on Unix, mtime seconds and nanoseconds, sample rate, channels) as
  little-endian `u64`. `Hasher::write` is fed explicit bytes rather than going
  through `Hash`, whose byte layout for `Path` and integers is not a stable
  contract either. Existing `.pcm` entries miss once and are decoded again;
  `prune` removes the old ones by size as before. The file format `VERSION` is
  unchanged.
- **`path_to_url` → `ffmpeg_path`.** In `celesta-media` and `celesta-exporter`.
  The function returns the path as the string FFmpeg opens, for inputs and
  outputs alike, so the name `ffmpeg_input` from the issue would not fit.

No user-facing behavior or composition API changes, so the user guide and the
Agent Skill are unchanged.

## Checks

On macOS (Apple M4) with `PKG_CONFIG_PATH` pointed at FFmpeg 8:

- `cargo test -p celesta-composition -p celesta-media -p celesta-editor -p celesta-exporter`
  passed, including the existing `KeyframeCursor` agreement test and the new
  fixed-value test `entry_keys_are_64_bit_fnv1a_over_path_and_identity_bytes`.
  Its expected key was computed independently with a Python FNV-1a.
- `cargo clippy` on those crates with `--all-targets` reported only the
  existing `large_enum_variant` warning in `layer_select.rs`.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- In `packages/web`, `tsc --noEmit` and `vitest run` passed (149 tests).
