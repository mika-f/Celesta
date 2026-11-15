# Pre-release tags and 0.10.0-alpha.1 (2026-11-15)

Celesta 0.10.0-alpha.1 is the first release with the plugin registry client
and the first-party publishing workflow (`plugin-release.yml`). The release
tooling took only `vX.Y.Z` until now, while `plugin-release.yml` already took
pre-release tags, so a `v0.10.0-alpha.1` tag would have published the plugins
and failed `package.yml`'s validate job.

## Specification

- A release version is semver's `MAJOR.MINOR.PATCH` with an optional
  pre-release (`0.10.0-alpha.1`, `0.10.0-rc.1`), without build metadata and
  without leading zeros. `package.yml`'s tag check is now the one
  `plugin-release.yml` uses, so both workflows accept and refuse the same tags.
- `package.yml`'s validate job outputs `prerelease` (`true` when the version
  has a `-`).
  - The release job passes `--prerelease` to `gh release create` for such a
    version, so GitHub never marks it the latest release.
  - `deploy-website` is skipped for a pre-release: its download buttons point
    at the release it deploys for, and the website should stay on the latest
    full release.
  - The crates and plugin crates jobs publish pre-release versions as they
    publish any other; Cargo picks a pre-release only when a requirement names
    one.
- `scripts/plugins/bump-version.mts` accepts `X.Y.Z-PRERELEASE` and refuses
  anything else (`0.10`, `v0.10.0`, `0.010.0`, `0.10.0-01`, `0.10.0+build.1`)
  before writing a file. Pins become `=0.10.0-alpha.1`.
- `scripts/package-macos.py --version` and `scripts/package-windows.ps1
  -Version` accept a pre-release; the Node.js version stays `X.Y.Z`. The
  macOS `Info.plist` carries the full version in `CFBundleVersion` and
  `CFBundleShortVersionString`, as Electron's packagers do for notarized
  pre-releases; Inno Setup's `AppVersion` is free text.
- A plugin built against `0.10.0-alpha.1` gets the `celesta` range
  `>=0.10.0-alpha.1, <0.11.0` (`celestaRange`, `celesta_range_for`, which
  already handled pre-releases), so the alpha host and the later 0.10.x
  releases accept it.

## Tests that pinned Celesta's version

A pre-release is in a range only when a bound names a pre-release of the
same version, so `*` and `>=0.9.0, <0.10.0` both refuse `0.10.0-alpha.1`.
Two test fixtures assumed the running Celesta was a 0.9 release:

- `celesta_registry_client::testing::CELESTA_RANGE` (`>=0.9.0, <0.10.0`) is
  now `celesta_range()`, `celesta_range_for` of the running
  `CELESTA_VERSION`, so the CLI's registry tests resolve at any version (the
  bump to 0.10.0 would have broken them too).
- `celesta_plugin_host::registry::tests::lock` locked each plugin with
  `celesta: *`; it now uses `COMPATIBLE` (`=<CARGO_PKG_VERSION>`), as the
  manifests in those tests do.

## Checks

- `pnpm --dir scripts/plugins exec vitest run test/bump-version.test.mts`:
  bumps to `0.10.0-alpha.1`, and refuses the malformed versions above with
  nothing written.
- The tag regex, run in bash against `v0.10.0-alpha.1`, `v0.10.0`,
  `v0.10.0-rc.1` (accepted) and `v0.10`, `v0.10.0+b`, `v0.010.0`,
  `v0.10.0-01`, `v0.10.0-` (refused).
- `cargo test -p celesta-plugin-host -p celesta-registry-client
  -p celesta-registry-format -p celesta-cli` at `0.10.0-alpha.1`.
