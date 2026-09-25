# Build a macOS DMG

The DMG contains `Celesta.app` and a shortcut to Applications. Open the DMG,
drag Celesta to Applications, eject the disk image, and launch the installed
app. This is direct distribution outside the Mac App Store; no `.pkg` installer,
App Sandbox, or App Store provisioning profile is used.

## Build on a Mac

Use a native Apple Silicon or Intel Mac, Python 3.9+, the corresponding Rust
toolchain, Node.js, pnpm, and Xcode Command Line Tools. Install the native build
and packaging dependencies:

```sh
xcode-select --install
brew install ffmpeg@8 pkg-config dylibbundler
export PKG_CONFIG_PATH="$(brew --prefix ffmpeg@8)/lib/pkgconfig"
```

`ffmpeg@8` is keg-only, so `PKG_CONFIG_PATH` must point at it for the build.
Homebrew's unversioned `ffmpeg` formula is FFmpeg 9, which is not supported yet.

From the repository root:

```sh
python3 scripts/package-macos.py --version 0.1.0
```

Outputs go into `target/packages`:

- Apple Silicon: `Celesta-0.1.0-macos-arm64.dmg`
- Intel: `Celesta-0.1.0-macos-x64.dmg`

The script builds for the current Mac's architecture. Run it on each architecture
to produce both downloads; it does not create a universal binary. Rust and Node.js
must match the current architecture. Avoid Rosetta and mixed Homebrew prefixes.
Windows cannot run this packaging script or generate the macOS binaries.

Without signing options, the script applies an ad-hoc signature and produces a
development DMG. This does not satisfy Gatekeeper for a downloaded public release.

## What is included

```text
Celesta.app/
  Contents/
    Info.plist
    MacOS/
      Celesta
      Celesta-export
    Helpers/
      node
      esbuild
    Frameworks/
      (FFmpeg and its non-system shared-library dependencies)
    Resources/
      react/
      examples/
      licenses/
      NODE-LICENSE
```

Node.js 24.20.0 is downloaded from nodejs.org and verified against the official
SHA256 checksums. Use `--node-version` to select a different version. Only the
Node executable and license are extracted; npm is not included. JavaScript
dependencies are copied from the locked installation without pnpm symlinks.
Native esbuild lives in Helpers so all executable code can be signed explicitly.

macOS uses dynamically linked FFmpeg libraries. `dylibbundler` copies their
dependency closure into Frameworks and rewrites load paths. Packaging rejects
remaining external or unresolved library dependencies. Existing static linking
on Windows is preserved.

The minimum macOS version in Info.plist is the highest minimum required by any
bundled Mach-O binary. Homebrew bottles may raise this version. Building on a
newer Mac does not establish compatibility
with older macOS releases; test on the oldest version you intend to support.

`--skip-build` reuses the current `target/<native Rust target>/release` binaries
and `packages/react/dist`. Run the normal script once before using it. Downloads
are cached in `target/package-downloads`; each run gets a new staging directory.
Existing DMGs are never overwritten.

## Developer ID signing and notarization

For public downloads, use an Apple Developer Program **Developer ID Application**
certificate with its private key in your Keychain. Keep the bundle identifier
stable across releases; its default is `com.natsuneko.celesta`, configurable
with `--bundle-id`.

Create a notarytool Keychain profile once. This command prompts for credentials;
do not put passwords or private keys in the repository:

```sh
xcrun notarytool store-credentials Celesta-notary
```

Build, sign, and submit to Apple's notarization service:

```sh
python3 scripts/package-macos.py --version 0.1.0 \
  --sign-identity 'Developer ID Application: Your Name (TEAMID)' \
  --notary-profile Celesta-notary
```

All nested Mach-O files are signed before the app, using the hardened runtime
and secure timestamps. Only Node receives the JIT/executable-memory entitlements;
debugging and library-validation exceptions are not enabled. The script verifies
signatures, submits the app, checks acceptance, staples its ticket, then creates,
signs, notarizes, and staples the DMG. This gives both the disk image and the app
copied into Applications their tickets. `--sign-identity` alone signs but does
not notarize; `--notary-profile` explicitly enables uploading to Apple.

## Verification

Before creating the DMG, the script checks native architectures, dependency load
paths, minimum OS versions, and signatures. It then runs the shared React hooks
and MP4 export smoke test with Homebrew and Node.js removed from PATH and without
DYLD environment overrides.

After dragging the app into Applications, run that test against the installed copy:

```sh
/Applications/Celesta.app/Contents/Helpers/node \
  scripts/test-package.mjs /Applications/Celesta.app
```

Also check Finder installation, launch, preview, audio, and export on a Mac without
Homebrew or Node.js installed. A GPU is required for the export test. For a
notarized build, test a downloaded DMG with quarantine intact and verify offline
launch after copying to Applications. The script does not automate Finder or
change your Applications directory.

The initial implementation was prepared on Windows: portable validation and
shared runtime tests can run there, but DMG creation, macOS launch, signing, and
notarization still require validation on a Mac.

## Redistribution

The bundle includes available Rust and Homebrew license notices and dependency
inventories, plus the repository's root `LICENSE*` files (copied automatically)
and `packaging/GPL-SOURCE-OFFER.md`.

Homebrew's `ffmpeg@8` formula enables the `x264` encoder by default, which is
GPL-2.0-or-later licensed and makes this binary package as a whole
GPL-licensed in addition to Celesta's own MIT/Apache-2.0 source license; see
[`../GPL-SOURCE-OFFER.md`](../GPL-SOURCE-OFFER.md) and the root
[`README.md`](../../README.md#license). That file's written offer, together
with `LICENSE-GPL-2.0`, is how this package satisfies GPLv2 Section 3 without
bundling full FFmpeg/x264 source in every download. This is the same release
concern described in the Windows packaging guide.

References: [Apple's notarization guide](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution),
[code signing in depth](https://developer.apple.com/library/archive/technotes/tn2206/),
[dylibbundler](https://github.com/auriamg/macdylibbundler), and
[FFmpeg licensing](https://ffmpeg.org/legal.html).
