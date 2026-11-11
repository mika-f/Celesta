# FFmpeg split: PR #810 review follow-up

## Specification

Address the [overall review](https://github.com/mika-f/Celesta/pull/810#pullrequestreview-5846338540)
and [startup-license comment](https://github.com/mika-f/Celesta/pull/810#issuecomment-6774632696).
Preserve the agreed four plugin IDs and separately downloadable optional
codecs. Update the PR through rebase, without amending commits or publishing a
release.

## Implementation

- Rebase onto main after AVFoundation (#799), Media Foundation (#801) and the
  timeline changes merged. Keep both system-codec plugins bundled, alongside
  only the standard FFmpeg build. Merge version tracking, release checks,
  manifests, dependency inventories and packaging smoke tests for all six
  first-party media plugins.
- Move FFmpeg license validation out of normal startup. The four programs
  share command handling: `--check-license` is explicit; `--check-build` opens
  their encoders. Optional discovery/startup refuses a build missing its
  encoder rather than printing a misleading static manifest. Diagnostics name
  the actual plugin, libraries and license strings.
- Add `--distribution` to the plugin builder. Every release packager uses it;
  license and actual encoder-open checks run before staging. Cross-compiled
  distributions fail closed if the target binary cannot be executed on the
  host. Local builds can use a system FFmpeg 8.1.x, including GPL builds.
  Standalone archives repeat both checks after native relocation without
  host library search overrides; app smoke tests check the staged standard
  program explicitly.
- Choose an initial export container whose declared default encoders are
  installed: standard-only installations choose MOV instead of an unusable
  MP4. Preserve explicit presets. Missing MP4/WebM defaults now have recovery
  hints naming the optional downloads and standard MOV alternative.
- Treat downloaded x264/Opus as user plugins in native-platform integration
  tests. Native bundled H.264 outranks downloaded x264; explicit new IDs still
  select the download. Generate optional fixtures through their independently
  linked sibling programs, including native decoder/process tests.
- Restore Renovate's Unix FFmpeg version declaration. Include runner image and
  installed codec versions in native caches, and those fingerprints in Rust
  caches. Windows FFmpeg remains controlled by the pinned vcpkg port.
- Record Linux native libraries' exact binary package, architecture, source
  package/version and distribution in `licenses/native/debian-packages.json`.
  Missing or ambiguous provenance fails packaging. Source notices include
  downstream patches and native inventories on all OSes, mention BSD Opus/VPX
  only for their respective downloads, and identify standalone builds without
  requiring an installed app.
- Keep testing later Cargo packages after a failure and return an overall
  failure; prerequisite builds preserve profile, target, target-directory and
  other relevant build flags. Do not test against stale prerequisites.
- Move the real VP9 transparent-project export/decode assertion to the VPX
  build, where libvpx decodes alpha. Remove the byte-pattern substitute.
  Additional encoded frames distinguish fully transparent, opaque and multiple
  translucent regions. Add Opus encode/WebM/decode coverage and full manifest
  plus encoder-open checks for all four programs.
- Share the small process entry point and sibling-executable lookup. Keep
  separate manifests/crate identities and readable protocol tests rather than
  introducing a registration macro for the thin independent wrappers. Remove
  the unused mux-helper parameter.
- Update HANDOFF's durable architecture/setup/check references, English and
  Japanese guides, README, Agent Skill, MCP schema examples and packaging
  instructions. Portable `--init` projects export MOV with the standard
  plugin. x264-specific command examples select its new ID explicitly.
  Correct obsolete links and historical handoff statements.

The review comment's suggested automatic `ffmpeg/libx264` exposure in local
GPL builds is intentionally not adopted: the user chose stable, disjoint
plugin/component identities. Local GPL linkage is allowed without restoring
aliases or changing the standard profile's components.

## Checks

- Per-profile Rust tests passed: standard FFmpeg (57), Opus (3), VPX (4),
  x264 (49), AVFoundation (78), and Media Foundation's non-Windows/stub tests
  (64). VPX includes the real rendered quadrant export/decode test with
  `CELESTA_REQUIRE_GPU=1`; the x264 and AVFoundation exports also required the
  local GPU. Native Windows implementation tests require CI's Windows host.
- Dialog model tests: 27 passed. Export report tests: 19 passed. Project
  template tests: 12 passed.
- Root Clippy for editor/exporter/MCP/react-bridge/plugin-host, and per-profile
  all-target Clippy for all six media plugins passed with `-D warnings`.
  The existing dependency `block 0.1.6` emits a future-incompatibility notice.
- TypeScript and plugin-script Vitest: 250 passed, including real Cargo cache
  invalidation, prerequisite profile/target forwarding, failure aggregation,
  and distribution refusal with explicit and environment-supplied targets.
  The scratch Cargo regression isolates CARGO_HOME and compiler wrappers to
  avoid unrelated developer cache locks.
- Website Vitest: 52 passed; test TypeScript passed. Content check passed with
  75 existing metadata warnings after the native-plugin docs merged.
- Portable Python packaging tests: 14 passed, including exact downstream
  source version tracking and failure on ambiguous/incomplete provenance.
- Actual native probes: a standard program linked against Homebrew GPL FFmpeg
  passed `--manifest` and encoder opens, retained only standard components,
  and failed explicit distribution license validation with all actual GPL
  license strings. An Opus program built against standard libraries failed
  both `--manifest` and encoder opens. Correct standard/Opus binaries were
  rebuilt and passed both explicit checks afterward.
- Built all three macOS arm64 release ZIPs into
  `target/packages/review-810-20261111/`. After extraction with `ditto` into a
  new Unicode path, verified executable modes, complete manifests, actual
  encoder opens, license checks, dependency inventories and code signatures
  without DYLD/LD_LIBRARY_PATH overrides. Development GPU/editor crates do not
  enter the standalone inventories.
- Renovate's existing regex extracts the same Unix FFmpeg version from the
  setup action, build script and Dockerfile. Release workflow actionlint
  passed with only the repository-specific `$/.github/actions/setup-ffmpeg`
  syntax excluded; action YAML and Bash/Node syntax checks passed.
- Root/plugin formatting and `git diff --check` passed.

Full installers, notarization, and Linux/Windows ZIP creation were not run
locally. The changed Linux provenance code is covered by portable tests; actual
native linkage and Windows plugin test paths are checked by PR CI. PR CI skips
the Package job, so it does not exercise the release builders, relocated ZIP
checks, installer smoke tests or Debian provenance on a real Linux host. No release was
published. The source-package fields follow the
[dpkg-query documentation](https://manpages.debian.org/bookworm/dpkg/dpkg-query.1.en.html).
No rendering algorithm, layout, theme, or visual component changed; the UI
change is the initial container choice. Model tests cover its standard-only
fallback and native-H.264 case. Existing keyboard/focus behavior is unchanged.

## CI fixture follow-up

Run `42962808943` exposed one MCP test expectation still naming plugin
`ffmpeg` after its JSON input example changed to `ffmpeg-x264/libx264`.
The parsed component correctly uses the new namespace. Update the expected
ComponentRef to match the migrated example. Full local MCP validation passed:
56 unit tests and 39 integration tests, with exporter and test-codec programs
built first. This is an additional test commit; the review commit is not
amended. The native-library cache is saved before replacing the CI run so
its cold FFmpeg builds can be reused.


## Additional review at 31da2ab8

- Correct remaining native-H.264 priority contradictions in both plugin guides
  and troubleshooting pages, exporter help, Skill starter output and the
  sibling-program inventory. Correct the macOS notice path to
  `licenses/homebrew.json`.
- Native H.264 unavailability now has the same x264/MOV recovery hint as a
  missing MP4 default. The export dialog includes report hints in its existing
  inline error text; no controls, layout or rendering algorithm changed.
- Failed sibling discovery retains the first stderr line (bounded and stripped
  of control characters), draining stdout/stderr concurrently within the
  existing timeout. Cover a noisy encoder-missing failure.
- Check the actual WebM video-stream `alpha_mode=1` metadata alongside decoded
  alpha pixels. Restore complete rollback file-list assertions in script tests.
- Keep Windows' image fingerprint for now: vcpkg pins the source, but the runner
  still supplies MSVC and the Windows SDK. Avoid asserting compiler/SDK cache
  compatibility without checking it; a future cache change can fingerprint
  those tools explicitly instead of the whole image.
- A manual CI dispatch already builds unsigned macOS/Windows apps and all three
  platforms' optional ZIPs. Add a dispatch-only Docker build with explicit
  license/encoder checks and a one-frame MOV export from the final image.
  Neither workflow publishes a release, image or signed distribution.

Additional local checks passed: plugin-host (112, then 8 focused discovery
regressions after the lint fix), export reports (19), dialog model (28), VPX
alpha encoding/muxing/metadata/decoding (1), bump-version Vitest (11), script
TypeScript, website Vitest (52) and content check (75 existing warnings).
Root editor/exporter/plugin-host and VPX all-target Clippy passed with
`-D warnings`; root/plugin formatting and whitespace checks passed. The new
Docker job passed actionlint in isolation; the full existing workflow uses
repository-specific background/wait syntax that this actionlint version does
not understand. Manual packaging and Docker checks remain to be observed on
the pushed branch; PR runs skip both.

## Docker smoke-check follow-up

Manual run `43011749360` built the complete Linux image, including the standard
program's distribution license/encoder checks. The final-image probe then
failed because its newly added hard-coded path used `linux-x64`, whereas the
plugin builder's executable key/directory is `linux-x86_64`. Resolve the program
from the staged manifest using the image's bundled Node.js instead of guessing
the platform directory. Keep the ongoing native packaging jobs running so
their checks and native caches are not discarded; repeat the corrected image
probe on the next pushed revision.

The local Docker check also identified a missing Node.js runtime dependency:
`ldd /usr/local/bin/node` in the actual x64 `node:26-bookworm-slim` image
requires `libatomic.so.1`, while the Ubuntu 26.04 base and the final stage's
installed package set do not include `libatomic1`. Add it explicitly to the
runtime stage. The first local arm64 image build hit an FFmpeg-download TLS
EOF; CI's image build had already passed that download/compilation. Retry the
local build from a tracked-files archive to avoid sending unrelated ignored
developer render output as Docker context.
