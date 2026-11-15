#!/usr/bin/env python3
"""Build first-party media plugins as native ZIP archives.

By default, the optional ffmpeg-x264 plugin as an installable download for
the GitHub release, its plugin folder at the ZIP root. The standard ffmpeg
plugin (decoding, muxing, ProRes, VP9, AAC, Opus, PCM) comes with Celesta;
only the GPL libx264 build is a separate download.

With --registry, every first-party media plugin built on this system, as the
archives registry.celesta.studio takes: the plugin directory's contents at
the ZIP root, one ZIP per target at <out>/<id>/<target>.zip, each with the
same celesta-plugin.json, whose `executable` names the program of every
target in --targets the plugin is built for (.github/workflows/plugin-release.yml).
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
# The first-party media plugins, and what each needs beyond the folder
# build.mts lays out: the Homebrew formulas its macOS build must bundle a
# library from (the notices copied come from what it actually bundles), the
# prefix of the variables naming the vcpkg installation whose notices its
# Windows build copies (None: it links system libraries alone), the
# distribution libraries its Linux build bundles, and whether its program has
# FFmpeg's --check-license, --check-build and --manifest. AVFoundation and Media
# Foundation link system frameworks only.
PLUGINS = {
    "ffmpeg": dict(formulas=packaging.STANDARD_NATIVE_FORMULAS, vcpkg="", linux=("libopus.so", "libvpx.so"), ffmpeg=True),
    "ffmpeg-x264": dict(formulas=("x264",), vcpkg="FFMPEG_X264_", linux=("libx264.so",), ffmpeg=True),
    "avfoundation": dict(formulas=(), vcpkg=None, linux=(), ffmpeg=False),
    "mediafoundation": dict(formulas=(), vcpkg=None, linux=(), ffmpeg=False),
}
# The plugins that are a separate download on the GitHub release; the others
# come with Celesta.
DOWNLOADS = ("ffmpeg-x264",)


def write_archive(plugin, destination, *, contents_at_root=False):
    """Keep one installable plugin folder at the ZIP root, or with
    `contents_at_root` the folder's contents (the registry's layout),
    including Unix modes."""
    base = plugin if contents_at_root else plugin.parent
    with zipfile.ZipFile(destination, "x", compression=zipfile.ZIP_DEFLATED, strict_timestamps=False) as archive:
        for path in sorted(plugin.rglob("*")):
            if path.is_symlink():
                raise RuntimeError(f"Plugin archive must not depend on symlinks: {path}")
            if path.is_file():
                archive.write(path, path.relative_to(base).as_posix())


def builds_on(source, system):
    """Whether the plugin is built for `system` (`macos`, `windows`, `linux`):
    for every system, or for those its plugin.json `os` names."""
    return source.get("os") is None or system in source["os"]


def plugin_targets(source, targets):
    """The targets in `targets` (`<os>-<arch>`, as manifests name them) the
    plugin is built for."""
    return [target for target in targets if builds_on(source, target.split("-", 1)[0])]


def registry_manifest(text, source, targets):
    """The celesta-plugin.json every registry archive of the plugin carries:
    `text`, the one build.mts wrote for this system's target, with
    `executable` naming the program of each of `targets`, by the path
    build.mts gives it (bin/<target>/<program>). The registry wants one
    manifest for all of a version's targets, byte for byte the archives'."""
    manifest = json.loads(text)

    def program(target):
        return f"bin/{target}/{source['cargoPackage']}{'.exe' if target.startswith('windows-') else ''}"

    built = manifest.get("executable", {})
    own = next(iter(built), None)
    if len(built) != 1 or own not in targets or built[own] != program(own):
        raise RuntimeError(f"build.mts gave the executables {built}, not one of {sorted(targets)} at bin/<target>/")
    manifest["executable"] = {target: program(target) for target in sorted(targets)}
    return json.dumps(manifest, indent=2, ensure_ascii=False) + "\n"


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


def windows_notices(plugin, prefix):
    """Copy the notices of the vcpkg installation the plugin was built with:
    the one <prefix>VCPKG_ROOT names, as setup-ffmpeg exports it (VCPKG_ROOT
    for the standard build, FFMPEG_X264_VCPKG_ROOT for x264)."""
    variable = f"{prefix}VCPKG_ROOT"
    if not os.environ.get(variable):
        raise RuntimeError(f"Set {variable} to the native installation used to build this plugin")
    root = Path(os.environ[variable])
    triplet = os.environ.get(f"{prefix}VCPKGRS_TRIPLET",
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


def linux_dependencies(plugin, program, prefixes):
    # FFmpeg itself is static on Linux; only the external codec libraries
    # (`prefixes`, such as libx264.so) are shared, and each must be linked and
    # is bundled. glibc, zlib and the platform runtime stay system-provided.
    listing = run("ldd", program, capture=True)
    if "not found" in listing:
        raise RuntimeError(f"Unresolved plugin dependencies: {listing}")
    dependencies = re.findall(r"^\s*(\S+)\s+=>\s+(/\S+)\s+\(", listing, re.M)
    provenance = []
    native = plugin / "licenses/native"
    for name, source in dependencies:
        if name.startswith(prefixes):
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
    for prefix in prefixes:
        if not any(entry["library"].startswith(prefix) for entry in provenance):
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
        if name.startswith(prefixes) and not Path(source).resolve().is_relative_to(plugin.resolve()):
            raise RuntimeError(f"Codec dependency was not bundled: {source}")


def package_plugin(plugin_id, output, target, system, arch, args):
    """Build, check and (with a signing identity) sign one plugin, and write
    its ZIP: the download's, or with args.registry the registry's."""
    spec = PLUGINS[plugin_id]
    source = json.loads((ROOT / "plugins" / plugin_id / "plugin.json").read_text(encoding="utf-8"))
    version = source["version"]
    if args.version is not None and args.version != version:
        raise RuntimeError(f"Requested version {args.version} differs from {plugin_id} {version}")
    if not builds_on(source, system):
        raise RuntimeError(f"{plugin_id} is not built on {system}")
    destination = None
    if not args.registry:
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
            # AVFoundation links system frameworks alone, so it gets no Frameworks/.
            libraries = {}
            if packaging.linked_libraries([program]):
                relative = os.path.relpath(plugin / "Frameworks", program.parent)
                libraries = packaging.bundle_libraries([program], plugin / "Frameworks", f"@executable_path/{relative}/")
            cellar = Path(run("brew", "--cellar", capture=True)).resolve()
            packaging.collect_formula_notices(plugin / "licenses", packaging.library_formulas(libraries, cellar),
                                              required=spec["formulas"])
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
            if spec["vcpkg"] is not None:
                windows_notices(plugin, spec["vcpkg"])
        elif spec["linux"]:
            linux_dependencies(plugin, program, spec["linux"])
        # Run without codec search-path overrides so the staged package has to
        # resolve its own native libraries, rather than those on the build host.
        clean_env = {key: value for key, value in os.environ.items()
                     if not key.startswith("DYLD_") and key != "LD_LIBRARY_PATH"}
        # Only the FFmpeg programs have these flags; AVFoundation's and Media
        # Foundation's serve the plugin protocol alone.
        if spec["ffmpeg"]:
            run(program, "--check-license", env=clean_env)
            run(program, "--check-build", env=clean_env)
            printed = json.loads(run(program, "--manifest", capture=True, env=clean_env))
            if printed["id"] != plugin_id or printed["media"] != source["media"]:
                raise RuntimeError(f"Packaged manifest differs from {plugin_id}")
        if args.registry:
            # Named after the target build.mts gave this system's program.
            manifest = plugin / "celesta-plugin.json"
            text = manifest.read_text(encoding="utf-8")
            target_name = next(iter(json.loads(text).get("executable", {})), None)
            # Bytes, so Windows writes the same `\n` line ends as the others.
            manifest.write_bytes(registry_manifest(text, source, plugin_targets(source, args.targets)).encode("utf-8"))
            destination = output / plugin_id / f"{target_name}.zip"
            destination.parent.mkdir(parents=True, exist_ok=True)
            if destination.exists():
                raise RuntimeError(f"Output already exists: {destination}")
        else:
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
        write_archive(plugin, archive, contents_at_root=args.registry)
        if args.notary_profile:
            packaging.notarize(archive, args.notary_profile)
        os.link(archive, destination)
    print(f"Plugin ZIP: {destination}")
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plugins", nargs="+", choices=PLUGINS,
                        help=f"The plugins to package (default: {', '.join(DOWNLOADS)}; with --registry, "
                             "every first-party media plugin built on this system)")
    parser.add_argument("--version", help="Require the plugin manifests to match this release")
    parser.add_argument("--registry", action="store_true",
                        help="Write the registry's archives, <out>/<id>/<target>.zip, instead of the downloads")
    parser.add_argument("--targets", nargs="+", default=[],
                        help="With --registry, every <os>-<arch> the plugins are published for in this release")
    parser.add_argument("--out", type=Path)
    parser.add_argument("--sign-identity")
    parser.add_argument("--notary-profile")
    args = parser.parse_args()
    if args.notary_profile and not args.sign_identity:
        parser.error("--notary-profile requires --sign-identity")
    if args.registry != bool(args.targets):
        parser.error("--registry and --targets go together")
    system = {"darwin": "macos", "win32": "windows", "linux": "linux"}.get(sys.platform)
    if system is None:
        parser.error("Unsupported platform")
    arch = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "x64", "AMD64": "x64"}.get(platform.machine())
    if arch is None:
        parser.error("Unsupported architecture")
    if args.plugins is None:
        args.plugins = list(DOWNLOADS)
        if args.registry:
            args.plugins = [plugin_id for plugin_id in PLUGINS if builds_on(
                json.loads((ROOT / "plugins" / plugin_id / "plugin.json").read_text(encoding="utf-8")), system)]
    compiler = run("rustc", "-vV", capture=True)
    target = next(line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: "))
    args.out = (args.out or ROOT / ("target/registry" if args.registry else "target/packages")).resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    for plugin_id in args.plugins:
        package_plugin(plugin_id, args.out, target, system, arch, args)


if __name__ == "__main__":
    main()
