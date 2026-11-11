"""Portable checks for separately downloadable codec packages."""
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

    def test_ambiguous_native_package_ownership_fails(self):
        with patch.object(packaging, "run", return_value="one, two: /codec"):
            with self.assertRaisesRegex(RuntimeError, "Ambiguous native package owner"):
                packaging.debian_package("libx264.so.164", "/codec")

    def test_archive_is_installable_and_preserves_the_executable_mode(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            plugin = root / "ffmpeg-opus"
            program = plugin / "bin/macos-aarch64/celesta-codec-ffmpeg-opus"
            program.parent.mkdir(parents=True)
            program.write_text("#!/bin/sh\nexit 0\n")
            program.chmod(0o755)
            os.utime(program, (0, 0))
            (plugin / "celesta-plugin.json").write_text(json.dumps({"id": "ffmpeg-opus"}))
            (plugin / "licenses").mkdir()
            (plugin / "licenses/LICENSE-LGPL-2.1").write_text("license fixture")
            archive = root / "plugin.zip"
            packaging.write_archive(plugin, archive)
            with zipfile.ZipFile(archive) as zipped:
                self.assertEqual(sorted(zipped.namelist()), [
                    "ffmpeg-opus/bin/macos-aarch64/celesta-codec-ffmpeg-opus",
                    "ffmpeg-opus/celesta-plugin.json", "ffmpeg-opus/licenses/LICENSE-LGPL-2.1"])
                self.assertEqual(zipped.getinfo(program.relative_to(root).as_posix()).date_time[:3], (1980, 1, 1))
                self.assertEqual(zipped.getinfo(program.relative_to(root).as_posix()).external_attr >> 16 & 0o777, 0o755)
            with self.assertRaises(FileExistsError):
                packaging.write_archive(plugin, archive)

    def test_standalone_dylibs_resolve_from_the_plugin_program(self):
        with tempfile.TemporaryDirectory() as temporary:
            plugin = Path(temporary) / "ffmpeg-opus"
            program = plugin / "bin/macos-aarch64/codec"
            program.parent.mkdir(parents=True)
            program.touch()
            library = plugin / "Frameworks/libopus.dylib"
            library.parent.mkdir()
            library.touch()
            (plugin / "celesta-plugin.json").write_text(json.dumps({"executable": {"macos-aarch64": "bin/macos-aarch64/codec"}}))
            packaging.packaging.validate_dependencies(library, plugin,
                "libopus:\n\t@executable_path/../../Frameworks/libopus.dylib (compatibility version 1.0.0)\n")
            with self.assertRaises(RuntimeError):
                packaging.packaging.validate_dependencies(library, plugin,
                    "libopus:\n\t@executable_path/../../../outside.dylib (compatibility version 1.0.0)\n")

    def test_license_inventory_has_only_the_selected_plugins_dependencies(self):
        names = ["celesta-codec-ffmpeg", "celesta-codec-ffmpeg-opus", "celesta-codec-ffmpeg-x264", "opus-dev-only", "shared"]
        metadata = {"packages": [{"id": name, "name": name} for name in names],
            "workspace_members": names[:3], "resolve": {"nodes": [
                {"id": name, "deps": [{"pkg": dep, "dep_kinds": [{"kind": kind}]} for dep, kind in deps]}
                for name, deps in [
                    (names[0], [("shared", None)]), (names[1], [(names[0], None), ("opus-dev-only", "dev")]),
                    (names[2], [(names[0], None)]), ("opus-dev-only", []), ("shared", [])]]}}
        picked = packaging.packaging.bundled_packages(metadata, {"celesta-codec-ffmpeg-opus"})
        self.assertEqual(sorted(package["name"] for package in picked),
                         ["celesta-codec-ffmpeg", "celesta-codec-ffmpeg-opus", "shared"])


if __name__ == "__main__":
    unittest.main()
