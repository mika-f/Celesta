#!/usr/bin/env python3
"""Build optional media plugins as individually installable native ZIP archives."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import sys
import tempfile
import zipfile

# Reuse the app packager's license graph, Mach-O validation and signing helpers.
# Importing it does not run its macOS-only application build.
spec = importlib.util.spec_from_file_location("package_macos", Path(__file__).with_name("package-macos.py"))
packaging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packaging)
ROOT = packaging.ROOT
run = packaging.run
VARIANTS = {"x264": "x264", "opus": "opus", "vpx": "libvpx"}


def write_archive(plugin, destination):
    """Keep one installable plugin folder at the ZIP root, including Unix modes."""
    with zipfile.ZipFile(destination, "x", compression=zipfile.ZIP_DEFLATED, strict_timestamps=False) as archive:
        for path in sorted(plugin.rglob("*")):
            if path.is_symlink():
                raise RuntimeError(f"Plugin archive must not depend on symlinks: {path}")
            if path.is_file():
                archive.write(path, path.relative_to(plugin.parent).as_posix())


def windows_runtime():
    vswhere = Path(os.environ["ProgramFiles(x86)"]) / "Microsoft Visual Studio/Installer/vswhere.exe"
    installation = Path(run(vswhere, "-latest", "-products", "*", "-requires",
                            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath", capture=True))
    redists = sorted((installation / "VC/Redist/MSVC").glob("[0-9]*"), reverse=True)
    for redist in redists:
        for directory in (redist / "x64").glob("Microsoft.VC*.CRT"):
            if (directory / "vcruntime140.dll").is_file():
                return directory
    raise RuntimeError("MSVC x64 CRT redistributables were not found")


def windows_notices(plugin, variant):
    root = Path(os.environ[f"FFMPEG_{variant.upper()}_VCPKG_ROOT"])
    triplet = os.environ.get(f"FFMPEG_{variant.upper()}_VCPKGRS_TRIPLET",
                             os.environ.get("VCPKGRS_TRIPLET", "x64-windows-static-md"))
    shutil.copytree(root / "installed" / triplet / "share", plugin / "licenses/native")


def linux_dependencies(plugin, program, variant):
    # FFmpeg itself is static on Linux; only the selected external codec may
    # need a shared library. glibc and the platform runtime stay system-provided.
    prefix = {"x264": "libx264.so", "opus": "libopus.so", "vpx": "libvpx.so"}[variant]
    listing = run("ldd", program, capture=True)
    if "not found" in listing:
        raise RuntimeError(f"Unresolved plugin dependencies: {listing}")
    dependencies = re.findall(r"^\s*(\S+)\s+=>\s+(/\S+)\s+\(", listing, re.M)
    for name, source in dependencies:
        if name.startswith(prefix):
            directory = plugin / "lib"
            directory.mkdir(exist_ok=True)
            shutil.copy2(Path(source).resolve(), directory / name)
    if (plugin / "lib").exists():
        run("patchelf", "--set-rpath", "$ORIGIN/../../lib", program)
    resolved = run("ldd", program, capture=True)
    if "not found" in resolved:
        raise RuntimeError(f"Bundled plugin dependencies do not resolve: {resolved}")
    for name, source in re.findall(r"^\s*(\S+)\s+=>\s+(/\S+)\s+\(", resolved, re.M):
        if name.startswith(prefix) and not Path(source).resolve().is_relative_to(plugin.resolve()):
            raise RuntimeError(f"Codec dependency was not bundled: {source}")
    doc = {"x264": "libx264-*", "opus": "libopus[0-9]*", "vpx": "libvpx[0-9]*"}[variant]
    notices = list(Path("/usr/share/doc").glob(f"{doc}/copyright"))
    if not notices:
        raise RuntimeError(f"Native codec copyright notice not found: {doc}")
    for notice in notices:
        directory = plugin / "licenses/native" / notice.parent.name
        directory.mkdir(parents=True, exist_ok=True)
        shutil.copy2(notice, directory / "copyright")


def package_plugin(variant, output, target, system, arch, args):
    plugin_id = f"ffmpeg-{variant}"
    source = json.loads((ROOT / "plugins" / plugin_id / "plugin.json").read_text())
    version = source["version"]
    if args.version is not None and args.version != version:
        raise RuntimeError(f"Requested version {args.version} differs from {plugin_id} {version}")
    destination = output / f"Celesta-plugin-{plugin_id}-{version}-{system}-{arch}.zip"
    if destination.exists():
        raise RuntimeError(f"Output already exists: {destination}")
    with tempfile.TemporaryDirectory(prefix="media-plugin-", dir=output) as directory:
        stage = Path(directory)
        env = dict(os.environ)
        if system == "macos":
            env["RUSTFLAGS"] = env.get("RUSTFLAGS", "") + " -C link-arg=-Wl,-headerpad_max_install_names"
        run("node", ROOT / "scripts/plugins/build.mts", ROOT / "plugins" / plugin_id, "--out", stage, env=env)
        plugin = stage / plugin_id
        program = packaging.plugin_program(plugin)
        packaging.collect_licenses(plugin, target, names={source["cargoPackage"]}, include_app=False,
                                   native_formulas=(VARIANTS[variant],) if system == "macos" else ())
        if system == "macos":
            relative = os.path.relpath(plugin / "Frameworks", program.parent)
            run("dylibbundler", "-b", "-cd", "-ns", "-i", "/System/Library/", "-x", program,
                "-d", plugin / "Frameworks", "-p", f"@executable_path/{relative}/")
            run("xattr", "-cr", plugin)
            signing = ["--force", "--sign", args.sign_identity or "-"]
            if args.sign_identity:
                signing += ["--options", "runtime", "--timestamp"]
            for binary in packaging.macho_files(plugin):
                packaging.validate_dependencies(binary, plugin, run("otool", "-L", binary, capture=True))
                run("codesign", *signing, binary)
                run("codesign", "--verify", "--strict", binary)
        elif system == "windows":
            for dll in windows_runtime().glob("*.dll"):
                shutil.copy2(dll, program.parent / dll.name)
            windows_notices(plugin, variant)
        else:
            linux_dependencies(plugin, program, variant)
        # Run without codec search-path overrides so the staged package has to
        # resolve its own native libraries, rather than those on the build host.
        clean_env = {key: value for key, value in os.environ.items()
                     if not key.startswith("DYLD_") and key != "LD_LIBRARY_PATH"}
        printed = json.loads(run(program, "--manifest", capture=True, env=clean_env))
        if printed["id"] != plugin_id or printed["media"] != source["media"]:
            raise RuntimeError(f"Packaged manifest differs from {plugin_id}")
        (plugin / "INSTALL.txt").write_text(
            f"{plugin_id} {version} for Celesta\n\n"
            f"Copy the entire {plugin_id} folder into your Celesta user plugin folder:\n"
            "Windows: %APPDATA%\\Celesta\\plugins\n"
            "macOS: ~/Library/Application Support/Celesta/plugins\n"
            "Linux: $XDG_DATA_HOME/celesta/plugins (default ~/.local/share/celesta/plugins)\n\n"
            "Keep celesta-plugin.json, bin, native libraries and licenses together.\n"
            "Open Celesta, reload plugins and allow this plugin when prompted.\n"
            "For CLI use, pass --plugin-dir <parent folder> or --allow-plugin " + plugin_id + ".\n"
            "MP4 needs ffmpeg-x264; WebM needs ffmpeg-vpx and ffmpeg-opus.\n"
            "The standard ffmpeg plugin comes with Celesta. Use matching releases.\n", encoding="utf-8")
        archive = stage / destination.name
        write_archive(plugin, archive)
        if args.notary_profile:
            packaging.notarize(archive, args.notary_profile)
        os.link(archive, destination)
    print(f"Plugin ZIP: {destination}")
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plugins", nargs="+", choices=VARIANTS, default=list(VARIANTS))
    parser.add_argument("--version", help="Require the plugin manifests to match this release")
    parser.add_argument("--out", type=Path, default=ROOT / "target/packages")
    parser.add_argument("--sign-identity")
    parser.add_argument("--notary-profile")
    args = parser.parse_args()
    if args.notary_profile and not args.sign_identity:
        parser.error("--notary-profile requires --sign-identity")
    system = {"darwin": "macos", "win32": "windows", "linux": "linux"}.get(sys.platform)
    if system is None:
        parser.error("Unsupported platform")
    arch = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "x64", "AMD64": "x64"}.get(platform.machine())
    if arch is None:
        parser.error("Unsupported architecture")
    compiler = run("rustc", "-vV", capture=True)
    target = next(line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: "))
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    for variant in args.plugins:
        package_plugin(variant, args.out, target, system, arch, args)


if __name__ == "__main__":
    main()
