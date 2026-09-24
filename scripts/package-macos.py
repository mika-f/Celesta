#!/usr/bin/env python3
"""Build a native-architecture app bundle and drag-to-Applications DMG on macOS."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
# Homebrew's unversioned ffmpeg formula is FFmpeg 9, which Celesta does not support yet.
FFMPEG_FORMULA = "ffmpeg@8"
MACHO_MAGIC = {
    b"\xfe\xed\xfa\xce", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf",
    b"\xcf\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca",
    b"\xca\xfe\xba\xbf", b"\xbf\xba\xfe\xca",
}


def run(*arguments, capture=False, env=None):
    result = subprocess.run(
        [str(argument) for argument in arguments], check=True, cwd=ROOT,
        text=True, stdout=subprocess.PIPE if capture else None, env=env,
    )
    return result.stdout.strip() if capture else None


def download(url, destination):
    if not destination.exists():
        partial = destination.with_suffix(destination.suffix + ".partial")
        with urllib.request.urlopen(url, timeout=60) as response, partial.open("wb") as output:
            shutil.copyfileobj(response, output)
        partial.replace(destination)


def macho_files(app):
    result = []
    for path in app.rglob("*"):
        if path.is_file() and not path.is_symlink():
            with path.open("rb") as file:
                if file.read(4) in MACHO_MAGIC:
                    result.append(path)
    return sorted(result)


def minimum_version(load_commands):
    versions = []
    for command in load_commands.split("Load command "):
        if "cmd LC_BUILD_VERSION\n" in command:
            match = re.search(r"\bminos (\d+(?:\.\d+)+)", command)
        elif "cmd LC_VERSION_MIN_MACOSX\n" in command:
            match = re.search(r"\bversion (\d+(?:\.\d+)+)", command)
        else:
            continue
        if match:
            versions.append(tuple(int(part) for part in match[1].split(".")))
    if not versions:
        raise RuntimeError("Mach-O file has no macOS minimum-version load command")
    return max(versions)


def validate_dependencies(binary, contents, listing):
    """Reject dependencies on the build machine, including unhandled @rpath links."""
    for line in listing.splitlines()[1:]:
        dependency = line.strip().split(" (compatibility version", 1)[0]
        if not dependency:
            continue
        if dependency.startswith(("/usr/lib/", "/System/Library/")):
            continue
        if dependency.startswith("@executable_path/"):
            executable_directory = contents / "MacOS" if binary.suffix == ".dylib" else binary.parent
            target = executable_directory / dependency.removeprefix("@executable_path/")
        elif dependency.startswith("@loader_path/"):
            target = binary.parent / dependency.removeprefix("@loader_path/")
        else:
            raise RuntimeError(f"Unbundled dependency in {binary}: {dependency}")
        if not target.resolve().is_relative_to(contents.resolve()) or not target.is_file():
            raise RuntimeError(f"Missing or external dependency in {binary}: {dependency}")


def collect_licenses(resources, target):
    licenses = resources / "licenses"
    licenses.mkdir()
    for path in ROOT.glob("LICENSE*"):
        shutil.copy2(path, licenses / path.name)
    shutil.copy2(ROOT / "packaging" / "GPL-SOURCE-OFFER.md", licenses / "GPL-SOURCE-OFFER.md")
    metadata = json.loads(run("cargo", "metadata", "--locked", "--offline", "--format-version", "1",
                              "--filter-platform", target, capture=True))
    inventory = []
    for package in metadata["packages"]:
        inventory.append({key: package.get(key) for key in ("name", "version", "license", "repository")})
        directory = Path(package["manifest_path"]).parent
        texts = [path for path in directory.iterdir()
                 if path.is_file() and re.match(r"LICENSE|COPYING|NOTICE", path.name, re.I)]
        if package.get("license_file"):
            texts.append(directory / package["license_file"])
        if texts:
            destination = licenses / "rust" / f'{package["name"]}-{package["version"]}'
            destination.mkdir(parents=True)
            for path in texts:
                shutil.copy2(path, destination / path.name)
    (licenses / "rust-packages.json").write_text(json.dumps(inventory, indent=2), encoding="utf-8")
    # Include the installed FFmpeg dependency closure's notices, including static dependencies.
    formulas = [FFMPEG_FORMULA, *run("brew", "deps", "--installed", FFMPEG_FORMULA, capture=True).splitlines()]
    (licenses / "homebrew.json").write_text(
        run("brew", "info", "--json=v2", *formulas, capture=True), encoding="utf-8")
    for formula in formulas:
        directory = Path(run("brew", "--prefix", formula, capture=True)).resolve()
        destination = licenses / "native" / formula.replace("/", "-")
        destination.mkdir(parents=True)
        for path in directory.iterdir():
            if path.is_file() and re.match(r"LICENSE|COPYING|NOTICE", path.name, re.I):
                shutil.copy2(path, destination / path.name)


def notarize(path, profile):
    result = json.loads(run("xcrun", "notarytool", "submit", path, "--keychain-profile", profile,
                            "--wait", "--output-format", "json", capture=True))
    if result.get("status") != "Accepted":
        raise RuntimeError(f"Notarization failed: {result}. Retrieve details with xcrun notarytool log.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", default="0.0.0")
    parser.add_argument("--node-version", default="24.20.0")
    parser.add_argument("--bundle-id", default="com.natsuneko.celesta")
    parser.add_argument("--sign-identity", help="Developer ID Application certificate identity")
    parser.add_argument("--notary-profile", help="Existing notarytool Keychain profile; uploads to Apple")
    parser.add_argument("--skip-build", action="store_true", help="Reuse current native release binaries and React dist")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("Run on macOS; the Apple SDK, signing tools, and hdiutil are required.")
    for version in (args.version, args.node_version):
        if not re.fullmatch(r"\d+\.\d+\.\d+", version):
            parser.error("Versions must have the form 1.2.3")
    if not re.fullmatch(r"[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+", args.bundle_id):
        parser.error("Invalid bundle identifier")
    if args.notary_profile and not args.sign_identity:
        parser.error("--notary-profile requires --sign-identity")
    architectures = {"arm64": ("arm64", "aarch64-apple-darwin"), "x86_64": ("x64", "x86_64-apple-darwin")}
    if platform.machine() not in architectures:
        parser.error("Only Apple Silicon and Intel Macs are supported")
    arch, target = architectures[platform.machine()]
    for tool in ("cargo", "rustc", "node", "pnpm", "brew", "dylibbundler", "otool", "lipo", "codesign", "hdiutil", "xcrun"):
        if not shutil.which(tool):
            parser.error(f"Missing required tool: {tool}")
    if f"host: {target}" not in run("rustc", "-vV", capture=True).splitlines():
        parser.error(f"Use the native {target} Rust toolchain (no Rosetta or cross-compilation)")
    if run("node", "-p", "process.arch", capture=True) != arch:
        parser.error("Use Node.js matching the build architecture, then reinstall dependencies")

    output = ROOT / "target/packages"
    output.mkdir(parents=True, exist_ok=True)
    dmg = output / f"Celesta-{args.version}-macos-{arch}.dmg"
    if dmg.exists():
        parser.error(f"Output already exists: {dmg}")
    stage = Path(tempfile.mkdtemp(prefix="macos-", dir=output))
    image = stage / "image"
    contents = image / "Celesta.app/Contents"
    app = contents.parent
    resources = contents / "Resources"
    helpers = contents / "Helpers"
    executables = contents / "MacOS"
    for directory in (resources, helpers, executables):
        directory.mkdir(parents=True)
    if not args.skip_build:
        run("pnpm", "--dir", "packages/react", "install", "--frozen-lockfile")
        run("pnpm", "--dir", "packages/react", "run", "codegen")
        run("pnpm", "--dir", "packages/react", "run", "build")
        build_env = dict(os.environ)
        # Reserve load-command space for relocating linked Homebrew libraries.
        build_env["CARGO_TARGET_DIR"] = str(ROOT / "target")
        build_env["CARGO_BUILD_TARGET"] = target
        build_env["RUSTFLAGS"] = build_env.get("RUSTFLAGS", "") + " -C link-arg=-Wl,-headerpad_max_install_names"
        run("cargo", "build", "--release", "--locked", "-p", "celesta-editor", "-p", "celesta-exporter", env=build_env)
    binaries = ROOT / "target" / target / "release"
    for source, name in (("celesta-editor", "Celesta"), ("celesta-exporter", "Celesta-export")):
        shutil.copy2(binaries / source, executables / name)

    downloads = ROOT / "target/package-downloads"
    downloads.mkdir(exist_ok=True)
    name = f"node-v{args.node_version}-darwin-{arch}"
    archive = downloads / f"{name}.tar.gz"
    checksums = downloads / f"node-v{args.node_version}-SHASUMS256.txt"
    base = f"https://nodejs.org/dist/v{args.node_version}"
    download(f"{base}/{archive.name}", archive)
    download(f"{base}/SHASUMS256.txt", checksums)
    expected = dict(line.split()[::-1] for line in checksums.read_text().splitlines())
    with archive.open("rb") as file:
        digest = hashlib.sha256()
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    if digest.hexdigest() != expected.get(archive.name):
        raise RuntimeError(f"Node.js SHA256 verification failed: {archive}")
    with tarfile.open(archive) as tar:
        for member, destination in (("bin/node", helpers / "node"), ("LICENSE", resources / "NODE-LICENSE")):
            # Extract only named regular files, never archive paths or symlinks.
            info = tar.getmember(f"{name}/{member}")
            if not info.isfile():
                raise RuntimeError(f"Unexpected Node.js archive entry: {member}")
            with tar.extractfile(info) as source, destination.open("wb") as file:
                shutil.copyfileobj(source, file)
    (helpers / "node").chmod(0o755)
    run(helpers / "node", ROOT / "scripts/stage-react-runtime.mjs", resources / "react")
    native_esbuild = resources / "react/node_modules/@esbuild" / f"darwin-{arch}" / "bin/esbuild"
    shutil.move(native_esbuild, helpers / "esbuild")
    examples = resources / "examples"
    examples.mkdir()
    for source in (ROOT / "examples/minimal.celesta.json", ROOT / "examples/editor-demo.celesta.json",
                   ROOT / "packages/react/examples/title.tsx"):
        shutil.copy2(source, examples / source.name)
    collect_licenses(resources, target)

    frameworks = contents / "Frameworks"
    # Main executables and Node are both one directory below Contents.
    run("dylibbundler", "-b", "-cd", "-ns", "-i", "/System/Library/", "-x", executables / "Celesta",
        "-x", executables / "Celesta-export", "-x", helpers / "node", "-x", helpers / "esbuild",
        "-d", frameworks, "-p", "@executable_path/../Frameworks/")
    native = macho_files(app)
    required = {executables / "Celesta", executables / "Celesta-export", helpers / "node", helpers / "esbuild"}
    if not required.issubset(native):
        raise RuntimeError("The app, exporter, Node.js, and esbuild must all be Mach-O executables")
    minimum = (0, 0)
    for binary in native:
        run("lipo", binary, "-verify_arch", platform.machine())
        validate_dependencies(binary, contents, run("otool", "-arch", platform.machine(), "-L", binary, capture=True))
        minimum = max(minimum, minimum_version(run("otool", "-arch", platform.machine(), "-l", binary, capture=True)))
    with (contents / "Info.plist").open("wb") as file:
        plistlib.dump({
            "CFBundleIdentifier": args.bundle_id, "CFBundleName": "Celesta",
            "CFBundleDisplayName": "Celesta", "CFBundleExecutable": "Celesta",
            "CFBundlePackageType": "APPL", "CFBundleVersion": args.version,
            "CFBundleShortVersionString": args.version, "NSHighResolutionCapable": True,
            "LSMinimumSystemVersion": ".".join(map(str, minimum)),
        }, file)

    run("xattr", "-cr", app)
    identity = args.sign_identity or "-"
    signing = ["--force", "--sign", identity]
    if args.sign_identity:
        signing += ["--options", "runtime", "--timestamp"]
    for binary in native:
        extra = ["--entitlements", ROOT / "packaging/macos/node-entitlements.plist"] if binary == helpers / "node" else []
        run("codesign", *signing, *extra, binary)
    run("codesign", *signing, app)
    run("codesign", "--verify", "--deep", "--strict", "--verbose=2", app)
    run(helpers / "node", ROOT / "scripts/test-package.mjs", app)
    if args.notary_profile:
        submission = stage / "notarization.zip"
        run("ditto", "-c", "-k", "--keepParent", app, submission)
        notarize(submission, args.notary_profile)
        run("xcrun", "stapler", "staple", app)
        run("xcrun", "stapler", "validate", app)
        run("spctl", "--assess", "--type", "execute", "--verbose=2", app)

    (image / "Applications").symlink_to("/Applications", target_is_directory=True)
    # Only the app and Applications shortcut go on the mounted volume.
    staged_dmg = stage / dmg.name
    run("hdiutil", "create", "-volname", "Celesta", "-srcfolder", image,
        "-fs", "HFS+", "-format", "UDZO", staged_dmg)
    if args.sign_identity:
        run("codesign", "--sign", identity, "--timestamp", staged_dmg)
    if args.notary_profile:
        notarize(staged_dmg, args.notary_profile)
        run("xcrun", "stapler", "staple", staged_dmg)
        run("xcrun", "stapler", "validate", staged_dmg)
    # Exclusive publication never replaces a previous DMG, even after a race.
    os.link(staged_dmg, dmg)
    print(f"DMG: {dmg}\nApp: {app}\nMinimum macOS: {'.'.join(map(str, minimum))}")
    if not args.notary_profile:
        print("Development build: not notarized for public distribution.")


if __name__ == "__main__":
    main()
