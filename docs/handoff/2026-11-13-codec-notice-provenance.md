# Codec packaging: notices tied to what is bundled (#819)

Follows up the review of [PR #810](https://github.com/mika-f/Celesta/pull/810)
([issue #819](https://github.com/mika-f/Celesta/issues/819)): four places where
a codec package's licence notices or recorded provenance could drift from the
libraries it actually ships.

## Specification

1. The Linux `ffmpeg-x264` ZIP copies the copyright file of the package each
   `debian-packages.json` entry names, not every `/usr/share/doc/libx264-*`
   folder a glob finds. A leftover `libx264-164` folder must not ship beside a
   `libx264-165` library.
2. Both Linux inventories (the ZIP's and the image's
   `record-codec-libraries.sh`) say whose copyright file each entry carries.
3. The Linux ZIP's base (glibc, bundled `libx264.so.N`, recorded
   `distribution`) changes only when the repository does.
4. The macOS notices cover whatever dylibbundler bundles, not a hand-kept
   formula list.

## Implementation

- `scripts/package-media-plugins.py`
  - `debian_copyright(package, source_package)` returns
    `/usr/share/doc/<package>/copyright`, or else an installed package from the
    same source package that has one (the same fallback as
    `record-codec-libraries.sh`), with the package it belongs to; it fails
    when there is neither.
  - `linux_dependencies` calls it for each provenance entry, copies the file to
    `licenses/native/<package>/copyright` (`<package>` is `binary:Package`
    without `:arch`), and sets the entry's `copyright_from`. The glob is gone.
  - On macOS it bundles through `packaging.bundle_libraries` and copies the
    notices of the formulas it traced; `VARIANTS[variant]` (`x264`) is now
    only a formula that must be among them.
- `packaging/linux/record-codec-libraries.sh` adds `copyright_from` to each
  entry: the doc folder name of the file `copyright_of` chose.
- `.github/workflows/package-build.yml`: `linux-plugins` runs on
  `ubuntu-24.04`, the image `ubuntu-latest` gave until now, so released ZIPs
  keep loading where they did. `ci.yml`'s docker job still loads the ZIP on
  `ubuntu:26.04`. Moving to 26.04 would raise the ZIP's glibc requirement to
  26.04's.
- `scripts/package-macos.py`
  - `linked_libraries(binaries)` follows `otool -arch <machine> -L` from the
    programs through every non-system dylib (skipping each dylib's own install
    name), keyed by the resolved file name, which is the name dylibbundler
    copies under (`libavcodec.62.dylib` becomes `libavcodec.62.28.100.dylib`).
    It fails on an `@rpath`/`@loader_path` link it cannot trace and on two
    files that would be bundled under one name.
  - `library_formulas(libraries, cellar)` maps each library under
    `brew --cellar` to its formula and fails on one that is neither a
    formula's nor an FFmpeg library (`libav*`, `libsw*`, `libpostproc`) built
    from source.
  - `bundle_libraries` runs dylibbundler and `check_bundled` fails unless
    `Frameworks/` holds exactly the traced files.
  - `collect_formula_notices(licenses, formulas, required)` writes
    `homebrew.json` and `licenses/native/<formula>/`; `collect_licenses` no
    longer takes `native_formulas`. The app collects the formulas of both the
    ffmpeg plugin's and the app's `Frameworks/` and requires
    `STANDARD_NATIVE_FORMULAS` (`opus`, `libvpx`) among them. The old
    `brew deps --installed` expansion is replaced by the trace, which reaches
    transitive libraries directly.

## Checks

- `python3 -B -m unittest scripts/test_package_macos.py scripts/test_package_media_plugins.py`
  (24 tests): the trace through symlinked kegs and dylibs, an untraceable
  `@rpath` link, formula classification and an uncovered library,
  `Frameworks/` holding extra or missing files, a missing required formula,
  and the own/sibling/missing copyright cases including the leftover
  `libx264-164` folder.
- On a Mac, `bundle_libraries` on a copy of Homebrew `ffmpeg@8`'s `ffprobe`
  with the real dylibbundler: the 18 traced libraries matched the 18 files it
  copied, and `library_formulas` named 11 formulas.
- `record-codec-libraries.sh`, run with stub `ldd` and `dpkg-query` and its
  paths pointed at a scratch doc tree, wrote valid JSON with
  `"copyright_from": "gcc-16-base"` for `libgcc-s1`. The real image build
  (`ci.yml`'s docker job) runs only on `workflow_dispatch`, and the macOS and
  Linux packaging jobs run in `package-build.yml`; neither ran for this change.
