"""Portable checks for macOS package validation; no Apple tools are mocked as passing."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("package_macos", Path(__file__).with_name("package-macos.py"))
packaging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packaging)


class MacPackageValidationTests(unittest.TestCase):
    def test_reads_modern_and_legacy_minimum_os_versions(self):
        self.assertEqual(packaging.minimum_version(
            "Load command 1\n      cmd LC_BUILD_VERSION\n platform 1\n    minos 14.5\n      sdk 15.0\n"
        ), (14, 5))
        self.assertEqual(packaging.minimum_version(
            "Load command 1\n      cmd LC_VERSION_MIN_MACOSX\n  version 10.15\n      sdk 14.0\n"
        ), (10, 15))

    def test_unknown_minimum_os_fails_instead_of_claiming_compatibility(self):
        with self.assertRaises(RuntimeError):
            packaging.minimum_version("Load command 1\n      cmd LC_UUID\n")

    def test_only_system_and_present_bundled_dependencies_are_accepted(self):
        with tempfile.TemporaryDirectory() as directory:
            contents = Path(directory) / "Celesta.app/Contents"
            frameworks = contents / "Frameworks"
            frameworks.mkdir(parents=True)
            # POSIX resolves `MacOS/..` only when MacOS exists, as in a bundle.
            (contents / "MacOS").mkdir()
            (frameworks / "libavcodec.62.dylib").touch()
            binary = contents / "MacOS/Celesta"
            packaging.validate_dependencies(binary, contents, "Celesta:\n"
                "\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1.0.0)\n"
                "\t@executable_path/../Frameworks/libavcodec.62.dylib (compatibility version 1.0.0, current version 1.0.0)\n")
            for dependency in ("/opt/homebrew/lib/libavcodec.62.dylib", "@rpath/libavcodec.62.dylib",
                               "@executable_path/../Frameworks/missing.dylib", "@loader_path/../../../outside.dylib"):
                with self.subTest(dependency=dependency), self.assertRaises(RuntimeError):
                    packaging.validate_dependencies(binary, contents,
                        f"Celesta:\n\t{dependency} (compatibility version 1.0.0, current version 1.0.0)\n")

    def test_helpers_resolve_executable_relative_libraries_from_their_own_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            contents = Path(directory) / "Celesta.app/Contents"
            (contents / "Helpers").mkdir(parents=True)
            (contents / "Helpers/libhelper.dylib").touch()
            packaging.validate_dependencies(contents / "Helpers/node", contents,
                "node:\n\t@executable_path/libhelper.dylib (compatibility version 1.0.0, current version 1.0.0)\n")

    def test_plugin_libraries_resolve_from_the_plugin_program_folder(self):
        with tempfile.TemporaryDirectory() as directory:
            contents = Path(directory) / "Celesta.app/Contents"
            plugin = contents / "PlugIns/ffmpeg"
            (plugin / "Frameworks").mkdir(parents=True)
            (plugin / "bin/macos-aarch64").mkdir(parents=True)
            (plugin / "celesta-plugin.json").write_text(json.dumps(
                {"executable": {"macos-aarch64": "bin/macos-aarch64/celesta-codec-ffmpeg"}}))
            (plugin / "Frameworks/libavcodec.62.dylib").touch()
            (plugin / "Frameworks/libx264.dylib").touch()
            program = plugin / "bin/macos-aarch64/celesta-codec-ffmpeg"
            self.assertEqual(packaging.plugin_program(plugin), program)
            packaging.validate_dependencies(program, contents, "celesta-codec-ffmpeg:\n"
                "\t@executable_path/../../Frameworks/libavcodec.62.dylib (compatibility version 1.0.0, current version 1.0.0)\n")
            library = plugin / "Frameworks/libavcodec.62.dylib"
            packaging.validate_dependencies(library, contents, "libavcodec.62.dylib:\n"
                "\t@executable_path/../../Frameworks/libx264.dylib (compatibility version 1.0.0, current version 1.0.0)\n")
            with self.assertRaises(RuntimeError):
                packaging.validate_dependencies(library, contents, "libavcodec.62.dylib:\n"
                    "\t@executable_path/../Frameworks/libx264.dylib (compatibility version 1.0.0, current version 1.0.0)\n")

    def test_a_plugin_program_is_the_one_executable_its_manifest_names(self):
        with tempfile.TemporaryDirectory() as directory:
            plugin = Path(directory)
            for executable in ({}, {"macos-aarch64": "bin/a", "macos-x86_64": "bin/b"}):
                with self.subTest(executable=executable), self.assertRaises(RuntimeError):
                    (plugin / "celesta-plugin.json").write_text(json.dumps({"executable": executable}))
                    packaging.plugin_program(plugin)


if __name__ == "__main__":
    unittest.main()
