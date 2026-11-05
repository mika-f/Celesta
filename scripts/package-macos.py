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
import time
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
# Homebrew's unversioned ffmpeg formula is FFmpeg 9, which Celesta does not support yet.
FFMPEG_FORMULA = "ffmpeg@8"
# The plugins workspace's Cargo package of the bundled codec plugin.
CODEC_PACKAGE = "celesta-codec-ffmpeg"
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


def create_dmg(image, destination):
    # hdiutil intermittently fails with "Resource busy" on CI runners.
    for attempt in range(1, 6):
        try:
            run("hdiutil", "create", "-volname", "Celesta", "-srcfolder", image,
                "-fs", "HFS+", "-format", "UDZO", destination)
            return
        except subprocess.CalledProcessError:
            destination.unlink(missing_ok=True)
            if attempt == 5:
                raise
            print(f"hdiutil create failed (attempt {attempt}/5); retrying", file=sys.stderr)
            time.sleep(attempt * 5)


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


def plugin_program(plugin):
    """The one program a plugin folder's celesta-plugin.json names."""
    executables = json.loads((plugin / "celesta-plugin.json").read_text(encoding="utf-8")).get("executable", {})
    if len(executables) != 1:
        raise RuntimeError(f"{plugin}/celesta-plugin.json must name exactly one executable, not {executables}")
    return plugin / next(iter(executables.values()))


def validate_dependencies(binary, contents, listing):
    """Reject dependencies on the build machine, including unhandled @rpath links."""
    for line in listing.splitlines()[1:]:
        dependency = line.strip().split(" (compatibility version", 1)[0]
        if not dependency:
            continue
        if dependency.startswith(("/usr/lib/", "/System/Library/")):
            continue
        if dependency.startswith("@executable_path/"):
            if binary.suffix != ".dylib":
                executable_directory = binary.parent
            elif binary.parent.parent.parent.name == "PlugIns":
                # A plugin's libraries, in <plugin>/Frameworks, load for the
                # program its manifest names.
                executable_directory = plugin_program(binary.parent.parent).parent
            else:
                executable_directory = contents / "MacOS"
            target = executable_directory / dependency.removeprefix("@executable_path/")
        elif dependency.startswith("@loader_path/"):
            target = binary.parent / dependency.removeprefix("@loader_path/")
        else:
            raise RuntimeError(f"Unbundled dependency in {binary}: {dependency}")
        if not target.resolve().is_relative_to(contents.resolve()) or not target.is_file():
            raise RuntimeError(f"Missing or external dependency in {binary}: {dependency}")


def codec_packages(metadata):
    """The plugins workspace's packages the bundled codec plugin ships.

    That workspace also holds plugins the package does not bundle (VRM), so
    only the codec and what its normal and build dependencies reach in
    `cargo metadata`'s resolve graph are listed; a dependency it has only
    for its tests is not shipped.
    """
    packages = {package["id"]: package for package in metadata["packages"]}
    roots = [member for member in metadata["workspace_members"] if packages[member]["name"] == CODEC_PACKAGE]
    if len(roots) != 1:
        raise RuntimeError(f"The plugins workspace must have one {CODEC_PACKAGE} member, not {len(roots)}")
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    reached, pending = set(), roots
    while pending:
        package = pending.pop()
        if package not in reached:
            reached.add(package)
            pending.extend(dependency["pkg"] for dependency in nodes[package]["deps"]
                           if any(kind["kind"] != "dev" for kind in dependency["dep_kinds"]))
    return [packages[package] for package in sorted(reached)]


def collect_licenses(resources, target):
    licenses = resources / "licenses"
    licenses.mkdir()
    for path in ROOT.glob("LICENSE*"):
        shutil.copy2(path, licenses / path.name)
    shutil.copy2(ROOT / "packaging" / "GPL-SOURCE-OFFER.md", licenses / "GPL-SOURCE-OFFER.md")
    flags = ("--locked", "--format-version", "1", "--filter-platform", target)
    # The codec plugin and its crates are in the plugins workspace. Packaging
    # builds only the codec there, so that workspace's other crates may not be
    # downloaded yet: no --offline for it, and only the codec's are listed.
    root = json.loads(run("cargo", "metadata", "--offline", *flags, capture=True))
    plugins = json.loads(run("cargo", "metadata", "--manifest-path", ROOT / "plugins/Cargo.toml", *flags, capture=True))
    packages = {}
    for package in (*root["packages"], *codec_packages(plugins)):
        packages.setdefault(package["id"], package)
    inventory = []
    for package in packages.values():
        inventory.append({key: package.get(key) for key in ("name", "version", "license", "repository")})
        directory = Path(package["manifest_path"]).parent
        texts = {path.name: path for path in directory.iterdir()
                 if path.is_file() and re.match(r"LICENSE|COPYING|NOTICE", path.name, re.I)}
        if package.get("license_file"):
            license_file = directory / package["license_file"]
            texts[license_file.name] = license_file
        # Vendored libraries keep their own notices one directory down
        # (signalsmith-stretch bundles two C++ libraries); the directory name
        # keeps same-named files apart.
        for path in directory.glob("*/*"):
            if (path.is_file() and path.suffix != ".rs"
                    and re.match(r"LICENSE|COPYING|NOTICE", path.name, re.I)):
                texts[f"{path.parent.name}-{path.name}"] = path
        if texts:
            destination = licenses / "rust" / f'{package["name"]}-{package["version"]}'
            destination.mkdir(parents=True)
            for name, path in texts.items():
                shutil.copy2(path, destination / name)
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
    parser.add_argument("--node-version", default="26.13.0")
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
    build_env = dict(os.environ)
    build_env["CARGO_TARGET_DIR"] = str(ROOT / "target")
    build_env["CARGO_BUILD_TARGET"] = target
    # Reserve load-command space for relocating linked Homebrew libraries.
    build_env["RUSTFLAGS"] = build_env.get("RUSTFLAGS", "") + " -C link-arg=-Wl,-headerpad_max_install_names"
    if not args.skip_build:
        run("pnpm", "install", "--frozen-lockfile")
        run("pnpm", "--dir", "packages/react", "run", "codegen")
        run("pnpm", "run", "build:runtime")
        run("cargo", "build", "--release", "--locked", "-p", "celesta-editor", "-p", "celesta-exporter",
            "-p", "celesta-mcp", env=build_env)
    binaries = ROOT / "target" / target / "release"
    for source, name in (("celesta-editor", "Celesta"), ("celesta-exporter", "Celesta-export"),
                         ("celesta-mcp", "Celesta-mcp")):
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
                   ROOT / "packages/cli/examples/title.tsx"):
        shutil.copy2(source, examples / source.name)
    collect_licenses(resources, target)

    # FFmpeg, and the GPL libx264 with it, is linked by the codec plugin
    # alone: Celesta's own executables link no FFmpeg, so the plugin folder
    # holds the program and every Homebrew library it loads. The shared build
    # script builds it in the plugins workspace (plugins/target, so not with
    # CARGO_TARGET_DIR; with --skip-build Cargo finds it current) and lays the
    # folder out; --target matches CARGO_BUILD_TARGET, where the script looks.
    codec = contents / "PlugIns" / "ffmpeg"
    codec_env = {name: value for name, value in build_env.items() if name != "CARGO_TARGET_DIR"}
    run(helpers / "node", ROOT / "scripts/plugins/build.mts", ROOT / "plugins/ffmpeg",
        "--out", contents / "PlugIns", "--target", target, env=codec_env)
    program = plugin_program(codec)
    frameworks_from_program = os.path.relpath(codec / "Frameworks", program.parent)
    run("dylibbundler", "-b", "-cd", "-ns", "-i", "/System/Library/", "-x", program,
        "-d", codec / "Frameworks", "-p", f"@executable_path/{frameworks_from_program}/")
    for binary in (executables / "Celesta", executables / "Celesta-export", executables / "Celesta-mcp"):
        if re.search(r"\blibav|\blibx264|\bffmpeg", run("otool", "-L", binary, capture=True), re.I):
            raise RuntimeError(f"{binary} links FFmpeg, which only the codec plugin may")
    frameworks = contents / "Frameworks"
    # Main executables and Node are both one directory below Contents.
    run("dylibbundler", "-b", "-cd", "-ns", "-i", "/System/Library/", "-x", executables / "Celesta",
        "-x", executables / "Celesta-export", "-x", executables / "Celesta-mcp", "-x", helpers / "node", "-x", helpers / "esbuild",
        "-d", frameworks, "-p", "@executable_path/../Frameworks/")
    native = macho_files(app)
    required = {executables / "Celesta", executables / "Celesta-export", executables / "Celesta-mcp",
                helpers / "node", helpers / "esbuild", program}
    if not required.issubset(native):
        raise RuntimeError("The app, exporter, MCP server, Node.js, esbuild, and the codec plugin must all be Mach-O executables")
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

    (image / "Applications").symlink_to("/Applications", target_is_directory=True)
    # Only the app and Applications shortcut go on the mounted volume.
    staged_dmg = stage / dmg.name
    create_dmg(image, staged_dmg)
    if args.sign_identity:
        run("codesign", "--sign", identity, "--timestamp", staged_dmg)
    if args.notary_profile:
        # One submission notarizes the DMG and all the code in it, the app
        # included. Only the outermost container needs the stapled ticket.
        notarize(staged_dmg, args.notary_profile)
        run("xcrun", "stapler", "staple", staged_dmg)
        run("xcrun", "stapler", "validate", staged_dmg)
        run("spctl", "--assess", "--type", "execute", "--verbose=2", app)
    # Exclusive publication never replaces a previous DMG, even after a race.
    os.link(staged_dmg, dmg)
    print(f"DMG: {dmg}\nApp: {app}\nMinimum macOS: {'.'.join(map(str, minimum))}")
    if not args.notary_profile:
        print("Development build: not notarized for public distribution.")


if __name__ == "__main__":
    main()
