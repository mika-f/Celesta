"""Portable checks for the media plugin archives: the ffmpeg-x264 download and the registry's."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location("package_media_plugins", Path(__file__).with_name("package-media-plugins.py"))
packaging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packaging)


class PluginArchives(unittest.TestCase):
    def test_native_inventory_records_downstream_binary_and_source_versions(self):
        with patch.object(packaging, "run", side_effect=[
            "libx264-164:amd64: /usr/lib/x86_64-linux-gnu/libx264.so.164",
            "libx264-164:amd64\t2:0.164-1build2\tamd64\tx264\t2:0.164-1",
        ]) as run:
            record = packaging.debian_package("libx264.so.164", "/usr/lib/x86_64-linux-gnu/libx264.so.164")
        self.assertEqual(record, dict(library="libx264.so.164", package="libx264-164:amd64",
            version="2:0.164-1build2", architecture="amd64", source_package="x264", source_version="2:0.164-1"))
        self.assertEqual(run.call_args.args[-1], "libx264-164:amd64")

    def test_a_native_inventory_without_an_exact_source_version_fails(self):
        with patch.object(packaging, "run", side_effect=["libx264-164:amd64: /codec", "libx264\tversion\tamd64\tx264\t"]):
            with self.assertRaisesRegex(RuntimeError, "Incomplete native source provenance"):
                packaging.debian_package("libx264.so.164", "/codec")

    def test_a_package_notice_is_its_own_or_a_same_source_siblings(self):
        installed = "libgcc-s1\tgcc-16\tinstalled\ngcc-15-base\tgcc-15\tinstalled\ngcc-16-base\tgcc-16\tinstalled\n"
        with tempfile.TemporaryDirectory() as temporary:
            doc = Path(temporary)
            for package in ("libx264-164", "libx264-165", "gcc-15-base", "gcc-16-base"):
                (doc / package).mkdir()
                (doc / package / "copyright").write_text(package)
            # A leftover libx264-164 folder is not the bundled libx264-165's notice.
            with patch.object(packaging, "run") as run:
                self.assertEqual(packaging.debian_copyright("libx264-165", "x264", doc),
                                 (doc / "libx264-165/copyright", "libx264-165"))
            run.assert_not_called()
            with patch.object(packaging, "run", return_value=installed):
                self.assertEqual(packaging.debian_copyright("libgcc-s1", "gcc-16", doc),
                                 (doc / "gcc-16-base/copyright", "gcc-16-base"))
            (doc / "libx264-165/copyright").unlink()
            with patch.object(packaging, "run", return_value="libx264-164\tx264\tconfig-files\n"):
                with self.assertRaisesRegex(RuntimeError, "Native copyright notice not found for libx264-165"):
                    packaging.debian_copyright("libx264-165", "x264", doc)

    def test_distribution_is_the_os_release_id_and_version(self):
        self.assertEqual(packaging.distribution_release(
            'PRETTY_NAME="Ubuntu 26.04 LTS"\nNAME="Ubuntu"\nVERSION_ID="26.04"\nID=ubuntu\nID_LIKE=debian\n'),
            "ubuntu 26.04")

    def test_a_distribution_without_id_or_version_fails(self):
        for text in ('VERSION_ID="26.04"\n', "ID=ubuntu\n", 'ID=\nVERSION_ID=""\n'):
            with self.assertRaisesRegex(RuntimeError, "Incomplete native source provenance"):
                packaging.distribution_release(text)

    def test_ambiguous_native_package_ownership_fails(self):
        with patch.object(packaging, "run", return_value="one, two: /codec"):
            with self.assertRaisesRegex(RuntimeError, "Ambiguous native package owner"):
                packaging.debian_package("libx264.so.164", "/codec")

    def test_archive_is_installable_and_preserves_the_executable_mode(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = root / "ffmpeg-x264"
            program = plugin / "bin/macos-aarch64/celesta-codec-ffmpeg-x264"
            program.parent.mkdir(parents=True)
            program.write_text("#!/bin/sh\nexit 0\n")
            program.chmod(0o755)
            os.utime(program, (0, 0))
            (plugin / "celesta-plugin.json").write_text(json.dumps({"id": "ffmpeg-x264"}))
            (plugin / "licenses").mkdir()
            (plugin / "licenses/LICENSE-GPL-2.0").write_text("license fixture")
            archive = root / "plugin.zip"
            packaging.write_archive(plugin, archive)
            with zipfile.ZipFile(archive) as zipped:
                self.assertEqual(sorted(zipped.namelist()), [
                    "ffmpeg-x264/bin/macos-aarch64/celesta-codec-ffmpeg-x264",
                    "ffmpeg-x264/celesta-plugin.json", "ffmpeg-x264/licenses/LICENSE-GPL-2.0"])
                self.assertEqual(zipped.getinfo(program.relative_to(root).as_posix()).date_time[:3], (1980, 1, 1))
                self.assertEqual(zipped.getinfo(program.relative_to(root).as_posix()).external_attr >> 16 & 0o777, 0o755)
            with self.assertRaises(FileExistsError):
                packaging.write_archive(plugin, archive)

    def test_standalone_dylibs_resolve_from_the_plugin_program(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin = Path(temporary) / "ffmpeg-x264"
            program = plugin / "bin/macos-aarch64/codec"
            program.parent.mkdir(parents=True)
            program.touch()
            library = plugin / "Frameworks/libx264.dylib"
            library.parent.mkdir()
            library.touch()
            (plugin / "celesta-plugin.json").write_text(json.dumps({"executable": {"macos-aarch64": "bin/macos-aarch64/codec"}}))
            packaging.packaging.validate_dependencies(library, plugin,
                "libx264:\n\t@executable_path/../../Frameworks/libx264.dylib (compatibility version 1.0.0)\n")
            with self.assertRaises(RuntimeError):
                packaging.packaging.validate_dependencies(library, plugin,
                    "libx264:\n\t@executable_path/../../../outside.dylib (compatibility version 1.0.0)\n")

    def test_license_inventory_has_only_the_selected_plugins_dependencies(self):
        names = ["celesta-codec-ffmpeg", "celesta-codec-ffmpeg-x264", "celesta-plugin-vrm", "x264-dev-only", "shared"]
        metadata = {"packages": [{"id": name, "name": name} for name in names],
            "workspace_members": names[:3], "resolve": {"nodes": [
                {"id": name, "deps": [{"pkg": dep, "dep_kinds": [{"kind": kind}]} for dep, kind in deps]}
                for name, deps in [
                    (names[0], [("shared", None)]), (names[1], [(names[0], None), ("x264-dev-only", "dev")]),
                    (names[2], []), ("x264-dev-only", []), ("shared", [])]]}}
        picked = packaging.packaging.bundled_packages(metadata, {"celesta-codec-ffmpeg-x264"})
        self.assertEqual(sorted(package["name"] for package in picked),
                         ["celesta-codec-ffmpeg", "celesta-codec-ffmpeg-x264", "shared"])

    def test_only_x264_is_a_separate_download(self):
        self.assertEqual(packaging.DOWNLOADS, ("ffmpeg-x264",))

    def test_every_first_party_plugin_is_packaged(self):
        source = Path(__file__).resolve().parent.parent / "plugins"
        manifests = {path.parent.name: json.loads(path.read_text(encoding="utf-8")) for path in source.glob("*/plugin.json")}
        # Those with a process per target, the others as one archive.
        self.assertEqual(sorted(packaging.PLUGINS), sorted(name for name, plugin in manifests.items() if "cargoPackage" in plugin))
        self.assertEqual(sorted(packaging.COMPONENTS), sorted(name for name, plugin in manifests.items() if "cargoPackage" not in plugin))
        self.assertEqual(sorted(packaging.COMPONENTS), ["aivis-speech", "code", "lottie", "score", "voicevox"])

    def test_a_component_plugin_is_one_archive_for_any_target(self):
        built = {"id": "code", "version": "0.10.0", "protocol": 1, "celesta": ">=0.10.0, <0.11.0",
                 "module": "js/index.js", "types": "js/index.d.ts"}

        def build(node, script, plugin, flag, stage):
            out = Path(stage) / "code"
            (out / "js").mkdir(parents=True)
            (out / "js/index.js").write_text("export {};\n")
            (out / "js/index.d.ts").write_text("export {};\n")
            (out / "celesta-plugin.json").write_text(json.dumps(built, indent=2) + "\n")

        with tempfile.TemporaryDirectory() as temporary, patch.object(packaging, "run", build):
            args = type("Args", (), {"version": None})()
            archive = packaging.package_component("code", Path(temporary), args)
            self.assertEqual(archive, Path(temporary) / "code/any.zip")
            with zipfile.ZipFile(archive) as zipped:
                self.assertEqual(sorted(zipped.namelist()), ["celesta-plugin.json", "js/index.d.ts", "js/index.js"])
            # Another run would overwrite it; the archives of one release are written once.
            with self.assertRaisesRegex(RuntimeError, "Output already exists"):
                packaging.package_component("code", Path(temporary), args)

    def test_a_component_plugin_refuses_another_version(self):
        args = type("Args", (), {"version": "9.9.9"})()
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(RuntimeError, "differs from code"):
                packaging.package_component("code", Path(temporary), args)

    def test_registry_archive_has_the_plugin_directory_at_its_root(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = root / "ffmpeg"
            program = plugin / "bin/linux-x86_64/celesta-codec-ffmpeg"
            program.parent.mkdir(parents=True)
            program.write_text("#!/bin/sh\nexit 0\n")
            program.chmod(0o755)
            (plugin / "celesta-plugin.json").write_text("{}")
            archive = root / "linux-x86_64.zip"
            packaging.write_archive(plugin, archive, contents_at_root=True)
            with zipfile.ZipFile(archive) as zipped:
                self.assertEqual(sorted(zipped.namelist()),
                                 ["bin/linux-x86_64/celesta-codec-ffmpeg", "celesta-plugin.json"])
                self.assertEqual(zipped.getinfo("bin/linux-x86_64/celesta-codec-ffmpeg").external_attr >> 16, 0o100755)

    def test_plugin_targets_follow_the_os_list(self):
        targets = ["linux-x86_64", "macos-aarch64", "windows-x86_64"]
        self.assertEqual(packaging.plugin_targets({}, targets), targets)
        self.assertEqual(packaging.plugin_targets({"os": ["macos"]}, targets), ["macos-aarch64"])
        self.assertTrue(packaging.builds_on({"os": ["windows"]}, "windows"))
        self.assertFalse(packaging.builds_on({"os": ["windows"]}, "linux"))

    def test_registry_manifest_names_every_targets_program(self):
        source = {"cargoPackage": "celesta-codec-ffmpeg"}
        built = '{\n  "id": "ffmpeg",\n  "executable": {\n    "macos-aarch64": "bin/macos-aarch64/celesta-codec-ffmpeg"\n  },\n  "media": {"priority": 1}\n}\n'
        targets = ["windows-x86_64", "linux-x86_64", "macos-aarch64"]
        text = packaging.registry_manifest(built, source, targets)
        self.assertEqual(json.loads(text), {"id": "ffmpeg", "executable": {
            "linux-x86_64": "bin/linux-x86_64/celesta-codec-ffmpeg",
            "macos-aarch64": "bin/macos-aarch64/celesta-codec-ffmpeg",
            "windows-x86_64": "bin/windows-x86_64/celesta-codec-ffmpeg.exe"}, "media": {"priority": 1}})
        self.assertTrue(text.startswith('{\n  "id": "ffmpeg",\n  "executable": {\n    "linux-x86_64"'))
        self.assertTrue(text.endswith("}\n"))
        # Built on Windows, the same manifest, byte for byte.
        windows = built.replace('"macos-aarch64": "bin/macos-aarch64/celesta-codec-ffmpeg"',
                                '"windows-x86_64": "bin/windows-x86_64/celesta-codec-ffmpeg.exe"')
        self.assertEqual(packaging.registry_manifest(windows, source, targets), text)

    def test_registry_manifest_refuses_an_unexpected_build(self):
        source = {"cargoPackage": "celesta-codec-ffmpeg"}
        for executable in ({}, {"freebsd-x86_64": "bin/freebsd-x86_64/celesta-codec-ffmpeg"},
                           {"linux-x86_64": "bin/celesta-codec-ffmpeg"},
                           {"linux-x86_64": "bin/linux-x86_64/celesta-codec-ffmpeg",
                            "macos-aarch64": "bin/macos-aarch64/celesta-codec-ffmpeg"}):
            with self.assertRaisesRegex(RuntimeError, "build.mts gave the executables"):
                packaging.registry_manifest(json.dumps({"executable": executable}), source, ["linux-x86_64", "macos-aarch64"])

    def test_linux_bundles_every_named_codec_library(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin = Path(temporary) / "ffmpeg"
            program = plugin / "bin/linux-x86_64/celesta-codec-ffmpeg"
            program.parent.mkdir(parents=True)
            program.touch()
            (plugin / "licenses").mkdir()
            listing = "\tlibopus.so.0 => /usr/lib/libopus.so.0 (0x1)\n\tlibz.so.1 => /usr/lib/libz.so.1 (0x2)\n"
            with patch.object(packaging, "run", return_value=listing):
                with self.assertRaisesRegex(RuntimeError, "Expected a package-owned libvpx.so dependency"):
                    with patch.object(packaging, "debian_package", side_effect=lambda name, source: dict(
                            library=name, package="libopus0:amd64", source_package="opus")), \
                         patch.object(packaging, "debian_copyright", return_value=(Path(__file__), "libopus0")), \
                         patch.object(packaging.shutil, "copy2"):
                        packaging.linux_dependencies(plugin, program, ("libopus.so", "libvpx.so"))


if __name__ == "__main__":
    unittest.main()
