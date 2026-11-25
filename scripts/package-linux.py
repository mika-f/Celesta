#!/usr/bin/env python3
"""Assemble the Linux x86_64 CLI archive from a built checkout."""

import argparse
import hashlib
import importlib.util
from pathlib import Path
import platform
import re
import shutil
import tarfile
import tempfile

spec = importlib.util.spec_from_file_location(
    "package_media_plugins", Path(__file__).with_name("package-media-plugins.py")
)
media = importlib.util.module_from_spec(spec)
spec.loader.exec_module(media)
ROOT = media.ROOT
run = media.run


def stage_node(runtime, version):
    name = f"node-v{version}-linux-x64"
    downloads = ROOT / "target/package-downloads"
    downloads.mkdir(parents=True, exist_ok=True)
    archive = downloads / f"{name}.tar.xz"
    checksums = downloads / f"node-v{version}-SHASUMS256.txt"
    base = f"https://nodejs.org/dist/v{version}"
    media.packaging.download(f"{base}/{archive.name}", archive)
    media.packaging.download(f"{base}/SHASUMS256.txt", checksums)
    expected = dict(line.split()[::-1] for line in checksums.read_text().splitlines())
    digest = hashlib.sha256()
    with archive.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    if digest.hexdigest() != expected.get(archive.name):
        raise RuntimeError(f"Node.js SHA256 verification failed: {archive}")
    with tarfile.open(archive) as tar:
        for member, filename in (("bin/node", "node"), ("LICENSE", "NODE-LICENSE")):
            info = tar.getmember(f"{name}/{member}")
            if not info.isfile():
                raise RuntimeError(f"Unexpected Node.js archive entry: {member}")
            with tar.extractfile(info) as source, (runtime / filename).open("wb") as file:
                shutil.copyfileobj(source, file)
    (runtime / "node").chmod(0o755)
    run(runtime / "node", ROOT / "scripts/stage-react-runtime.mjs", runtime / "react")


def stage_ffmpeg(install):
    plugins = install / "lib/celesta/plugins"
    run("node", ROOT / "scripts/plugins/build.mts", ROOT / "plugins/ffmpeg", "--distribution", "--out", plugins)
    plugin = plugins / "ffmpeg"
    program = media.packaging.plugin_program(plugin)
    media.linux_dependencies(plugin, program, ("libopus.so", "libvpx.so"))
    run(program, "--check-license")
    run(program, "--check-build")
    for path in (install, *install.rglob("*")):
        path.chmod(path.stat().st_mode & ~0o022)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", help="Require the workspace version to match this release")
    parser.add_argument("--node-version", default="26.13.0")
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        parser.error("Build this archive on Linux x86_64")
    if not re.fullmatch(r"\d+\.\d+\.\d+", args.node_version):
        parser.error("--node-version must have the form 1.2.3")
    manifests = (ROOT / "Cargo.toml").read_text().split("[workspace.package]", 1)[1].split("\n[", 1)[0]
    version = re.search(r'^version = "([^"]+)"$', manifests, re.M)[1]
    if args.version is not None and args.version != version:
        parser.error(f"Requested version {args.version} differs from workspace version {version}")

    output = ROOT / "target/packages"
    output.mkdir(parents=True, exist_ok=True)
    name = f"Celesta-{version}-linux-x64"
    with tempfile.TemporaryDirectory(prefix="celesta-linux-", dir=output) as directory:
        install = Path(directory) / name
        runtime = install / "bin/runtime"
        runtime.mkdir(parents=True)
        for binary in ("celesta", "celesta-exporter"):
            shutil.copy2(ROOT / "target/release" / binary, install / "bin")
        stage_node(runtime, args.node_version)
        stage_ffmpeg(install)
        media.packaging.collect_licenses(install, "x86_64-unknown-linux-gnu", names={"celesta-codec-ffmpeg"})
        archive = output / f"{name}.tar.gz"
        run("tar", "-czf", archive, "-C", directory, name)
        print(f"Linux CLI archive: {archive}")


if __name__ == "__main__":
    main()
