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
brew install pkg-config nasm dylibbundler libvpx opus
PKG_CONFIG_PATH="$(brew --prefix libvpx)/lib/pkgconfig:$(brew --prefix opus)/lib/pkgconfig" \
  scripts/build-ffmpeg-linux.sh "$PWD/plugins/target/ffmpeg/standard"
export PKG_CONFIG_PATH="$PWD/plugins/target/ffmpeg/standard/lib/pkgconfig"
```

Run the commands from the repository root. The app bundles only the standard
LGPL FFmpeg plugin (with libopus and libvpx). The x264 encoder is a separate
download; see [plugin packaging](../plugins/README.md). The default
distribution gate rejects a GPL FFmpeg build; local builds may use one.

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
      Celesta-mcp
    Helpers/
      node
      esbuild
    Frameworks/
      (non-system shared libraries Celesta's own executables need, if any)
    Resources/
      PlugIns/
        ffmpeg/
          celesta-plugin.json
          bin/macos-<arch>/celesta-codec-ffmpeg
          Frameworks/
            (FFmpeg and its non-system shared-library dependencies)
          licenses/
        avfoundation/
          celesta-plugin.json
          bin/macos-<arch>/celesta-codec-avfoundation
          licenses/
      react/
      examples/
      licenses/
      NODE-LICENSE
```

Node.js 26.12.1 is downloaded from nodejs.org and verified against the official
SHA256 checksums. Use `--node-version` to select a different version. Only the
Node executable and license are extracted; npm is not included. JavaScript
dependencies are copied from the locked installation without pnpm symlinks.
Native esbuild lives in Helpers so all executable code can be signed explicitly.

macOS uses dynamically linked FFmpeg libraries, which only the codec plugin
loads. `dylibbundler` copies their dependency closure into the plugin's own
`Resources/PlugIns/ffmpeg/Frameworks` and rewrites its load paths. Packaging
rejects remaining external or unresolved library dependencies. Existing static linking
on Windows is preserved.

The minimum macOS version in Info.plist is the highest minimum required by any
bundled Mach-O binary. Homebrew bottles may raise this version. Building on a
newer Mac does not establish compatibility
with older macOS releases; test on the oldest version you intend to support.

`--skip-build` reuses the current `target/<native Rust target>/release` binaries
and the built `packages/*/dist` runtime; the bundled media plugins (FFmpeg and
AVFoundation) are always built, which costs nothing when `plugins/target` is
current. Run the normal script once before using it. Downloads
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
signatures, then creates and signs the DMG and submits it once: Apple notarizes
the disk image together with the app and all other code inside it. The script
staples the ticket to the DMG, the outermost container, and checks that
Gatekeeper accepts the app. `--sign-identity` alone signs but does not notarize;
`--notary-profile` explicitly enables uploading to Apple.

## Verification

Before creating the DMG, the script checks native architectures, dependency load
paths, minimum OS versions, and signatures. It then runs the shared React hooks
and MOV/ProRes and WebM export smoke test with Homebrew and Node.js removed from PATH and without
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

The bundle includes available Rust license notices and a dependency inventory. The package-wide license folder contains Celesta's `LICENSE-MIT`
and `LICENSE-APACHE`; FFmpeg license texts and source notices stay in the
corresponding plugin's `licenses/` folder.

Celesta's own executables link no FFmpeg. Only `ffmpeg` (standard LGPL) is
staged under `Contents/Resources/PlugIns`, with its own `Frameworks/` and `licenses/`.
The plugin's `Frameworks/` holds the FFmpeg dylibs built from source and the
Homebrew libopus and libvpx (BSD licensed) they link; the app's
`licenses/homebrew.json` and `licenses/native/` carry those two formulas'
notices. The app's Rust/native inventory excludes the optional x264 crate.
The package smoke test exports ProRes/MOV and WebM and rejects an accidentally
bundled x264 plugin. The optional `ffmpeg-x264` ZIP is built separately by
[`package-media-plugins.py`](../../scripts/package-media-plugins.py), signed
and notarized in release builds, and attached to the same release; see
[plugin packaging](../plugins/README.md). Its licenses remain in its own
folder. The app stays MIT/Apache-2.0 with LGPL FFmpeg.

`Contents/Resources/PlugIns/avfoundation` holds the AVFoundation plugin
(`plugins/avfoundation`, built by the same script after FFmpeg). It is
MIT/Apache-2.0 licensed and links only system frameworks, so it has no
`Frameworks/` folder, needs no dylibbundler, and its `licenses/` holds
`LICENSE-MIT` and `LICENSE-APACHE` alone.

References: [Apple's notarization guide](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution),
[code signing in depth](https://developer.apple.com/library/archive/technotes/tn2206/),
[dylibbundler](https://github.com/auriamg/macdylibbundler), and
[FFmpeg licensing](https://ffmpeg.org/legal.html).
