#!/usr/bin/env python3
"""Build the optional ffmpeg-x264 media plugin as an installable native ZIP archive.

The standard ffmpeg plugin (decoding, muxing, ProRes, VP9, AAC, Opus, PCM)
comes with Celesta; only the GPL libx264 build is a separate download.
"""

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
# Each optional variant, and the Homebrew formula its macOS build must bundle
# a library from; the notices copied come from what it actually bundles.
VARIANTS = {"x264": "x264"}


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
    installation = Path(run(vswhere, "-utf8", "-latest", "-products", "*", "-requires",
                            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath", capture=True))
    redists = sorted((installation / "VC/Redist/MSVC").glob("[0-9]*"), reverse=True)
    for redist in redists:
        for directory in (redist / "x64").glob("Microsoft.VC*.CRT"):
            if (directory / "vcruntime140.dll").is_file():
                return directory
    raise RuntimeError("MSVC x64 CRT redistributables were not found")


def windows_notices(plugin, variant):
    variable = f"FFMPEG_{variant.upper()}_VCPKG_ROOT"
    if not os.environ.get(variable):
        raise RuntimeError(f"Set {variable} to the native installation used to build this download")
    root = Path(os.environ[variable])
    triplet = os.environ.get(f"FFMPEG_{variant.upper()}_VCPKGRS_TRIPLET",
                             os.environ.get("VCPKGRS_TRIPLET", "x64-windows-static-md"))
    shutil.copytree(root / "installed" / triplet / "share", plugin / "licenses/native")


def debian_package(library, source):
    """Identify the exact distribution source, including downstream patches."""
    owners = run("dpkg-query", "--search", Path(source).resolve(), capture=True).splitlines()
    if len(owners) != 1 or ", " in owners[0].rsplit(": ", 1)[0]:
        raise RuntimeError(f"Ambiguous native package owner for {source}: {owners}")
    owner = owners[0].rsplit(": ", 1)[0]
    record = run("dpkg-query", "--show", "--showformat=${binary:Package}\t${Version}\t${Architecture}\t${source:Package}\t${source:Version}", owner, capture=True).split("\t")
    if len(record) != 5 or not all(record):
        raise RuntimeError(f"Incomplete native source provenance for {source}: {record}")
    package, version, architecture, source_package, source_version = record
    return dict(library=library, package=package, version=version, architecture=architecture,
                source_package=source_package, source_version=source_version)


def debian_copyright(package, source_package, doc=Path("/usr/share/doc")):
    """The copyright file of an installed package, and the package it belongs
    to: its own, or else that of an installed package built from the same
    source, as packaging/linux/record-codec-libraries.sh finds it. Some doc
    folders are symlinks to a sibling's, which Ubuntu's container images drop."""
    own = doc / package / "copyright"
    if own.is_file():
        return own, package
    listing = run("dpkg-query", "--show", "--showformat=${Package}\t${source:Package}\t${db:Status-Status}\n", capture=True)
    for line in listing.splitlines():
        sibling, source, status = line.split("\t")
        if source == source_package and status == "installed" and (doc / sibling / "copyright").is_file():
            return doc / sibling / "copyright", sibling
    raise RuntimeError(f"Native copyright notice not found for {package} (source package {source_package}) under {doc}")


def distribution_release(os_release):
    """`ID VERSION_ID` from os-release text, such as `ubuntu 26.04`: the same
    `distribution` the Linux image's record-codec-libraries.sh writes."""
    fields = {}
    for line in os_release.splitlines():
        key, separator, value = line.partition("=")
        if separator:
            fields[key.strip()] = value.strip().strip("\"'")
    if not fields.get("ID") or not fields.get("VERSION_ID"):
        raise RuntimeError("Incomplete native source provenance: os-release lacks ID or VERSION_ID")
    return f"{fields['ID']} {fields['VERSION_ID']}"


def linux_dependencies(plugin, program, variant):
    # FFmpeg itself is static on Linux; only the external codec library may
    # need a shared library. glibc and the platform runtime stay system-provided.
    prefix = {"x264": "libx264.so"}[variant]
    listing = run("ldd", program, capture=True)
    if "not found" in listing:
        raise RuntimeError(f"Unresolved plugin dependencies: {listing}")
    dependencies = re.findall(r"^\s*(\S+)\s+=>\s+(/\S+)\s+\(", listing, re.M)
    provenance = []
    native = plugin / "licenses/native"
    for name, source in dependencies:
        if name.startswith(prefix):
            directory = plugin / "lib"
            directory.mkdir(exist_ok=True)
            shutil.copy2(Path(source).resolve(), directory / name)
            # Record the exact binary and distribution source package, not
            # only its upstream version: Ubuntu/Debian may patch the source.
            entry = debian_package(name, source)
            # The notice of this entry's own package, not of whichever
            # package's doc folder happens to match a pattern.
            package = entry["package"].partition(":")[0]
            notice, entry["copyright_from"] = debian_copyright(package, entry["source_package"])
            (native / package).mkdir(parents=True, exist_ok=True)
            shutil.copy2(notice, native / package / "copyright")
            provenance.append(entry)
    if not provenance:
        raise RuntimeError(f"Expected a package-owned {prefix} dependency in {program}")
    (native / "debian-packages.json").write_text(json.dumps({
        "distribution": distribution_release(Path("/etc/os-release").read_text()), "packages": provenance,
        "source_instructions": "Retrieve the exact source_package=source_version from this distribution's source archive, including its Debian/Ubuntu patch set; request the matching materials through the bundled source notice.",
    }, indent=2) + "\n")
    if (plugin / "lib").exists():
        run("patchelf", "--set-rpath", "$ORIGIN/../../lib", program)
    resolved = run("ldd", program, capture=True)
    if "not found" in resolved:
        raise RuntimeError(f"Bundled plugin dependencies do not resolve: {resolved}")
    for name, source in re.findall(r"^\s*(\S+)\s+=>\s+(/\S+)\s+\(", resolved, re.M):
        if name.startswith(prefix) and not Path(source).resolve().is_relative_to(plugin.resolve()):
            raise RuntimeError(f"Codec dependency was not bundled: {source}")


def package_plugin(variant, output, target, system, arch, args):
    plugin_id = f"ffmpeg-{variant}"
    source = json.loads((ROOT / "plugins" / plugin_id / "plugin.json").read_text(encoding="utf-8"))
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
        run("node", ROOT / "scripts/plugins/build.mts", ROOT / "plugins" / plugin_id, "--distribution", "--out", stage, env=env)
        plugin = stage / plugin_id
        program = packaging.plugin_program(plugin)
        packaging.collect_licenses(plugin, target, names={source["cargoPackage"]}, include_app=False)
        if system == "macos":
            relative = os.path.relpath(plugin / "Frameworks", program.parent)
            libraries = packaging.bundle_libraries([program], plugin / "Frameworks", f"@executable_path/{relative}/")
            cellar = Path(run("brew", "--cellar", capture=True)).resolve()
            packaging.collect_formula_notices(plugin / "licenses", packaging.library_formulas(libraries, cellar),
                                              required=(VARIANTS[variant],))
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
        run(program, "--check-license", env=clean_env)
        run(program, "--check-build", env=clean_env)
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
            "Linux H.264/MP4 needs ffmpeg-x264; macOS/Windows also have system H.264 encoders.\n"
            "The standard ffmpeg plugin comes with Celesta and covers everything else,\n"
            "WebM (VP9 and Opus) included. Use matching releases.\n", encoding="utf-8")
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
