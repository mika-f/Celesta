"""Portable checks for macOS package validation; no Apple tools are mocked as passing."""

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("package_macos", Path(__file__).with_name("package-macos.py"))
packaging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packaging)


class MacPackageValidationTests(unittest.TestCase):
    def test_captured_unicode_metadata_ignores_the_windows_ansi_code_page(self):
        payload = json.dumps({"description": "あ / codec"}, ensure_ascii=False).encode("utf-8")
        # Cargo emits UTF-8 even when Python's Windows default is cp1252.
        # Exercise the real pipe/decoder rather than mocking a decoded result.
        with patch("subprocess._text_encoding", return_value="cp1252"):
            captured = packaging.run(sys.executable, "-c",
                f"import sys; sys.stdout.buffer.write(bytes.fromhex('{payload.hex()}'))", capture=True)
        self.assertEqual(json.loads(captured), {"description": "あ / codec"})

    def test_formula_notices_are_copied_and_a_formula_without_one_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            prefix = root / "opus"
            prefix.mkdir()
            (prefix / "COPYING").write_text("BSD")
            (prefix / "README").write_text("not a notice")
            packaging.copy_formula_notices("opus", prefix, root / "licenses/native/opus")
            self.assertEqual(sorted(path.name for path in (root / "licenses/native/opus").iterdir()), ["COPYING"])
            bare = root / "bare"
            bare.mkdir()
            (bare / "README").write_text("no licence here")
            with self.assertRaisesRegex(RuntimeError, "no licence file"):
                packaging.copy_formula_notices("bare", bare, root / "licenses/native/bare")
            self.assertFalse((root / "licenses/native/bare").exists())

    def test_bundled_libraries_are_traced_through_each_other_to_their_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            keg = root / "Cellar/opus/1.5.2/lib"
            keg.mkdir(parents=True)
            (keg / "libopus.0.dylib").touch()
            (root / "opt").mkdir()
            (root / "opt/opus").symlink_to(root / "Cellar/opus/1.5.2")
            ffmpeg = root / "ffmpeg/lib"
            ffmpeg.mkdir(parents=True)
            (ffmpeg / "libavcodec.62.28.100.dylib").touch()
            (ffmpeg / "libavcodec.62.dylib").symlink_to("libavcodec.62.28.100.dylib")
            (ffmpeg / "libavutil.60.dylib").touch()
            program = root / "codec"
            entry = " (compatibility version 1.0.0, current version 1.0.0)"
            listings = {
                program: [ffmpeg / "libavcodec.62.dylib", "/usr/lib/libSystem.B.dylib"],
                # Each dylib lists its own install name first.
                ffmpeg / "libavcodec.62.28.100.dylib": [ffmpeg / "libavcodec.62.dylib", ffmpeg / "libavutil.60.dylib",
                                                        root / "opt/opus/lib/libopus.0.dylib",
                                                        "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation"],
                ffmpeg / "libavutil.60.dylib": [ffmpeg / "libavutil.60.dylib"],
                keg / "libopus.0.dylib": [root / "opt/opus/lib/libopus.0.dylib"],
            }

            def otool(*arguments, capture=False, env=None):
                self.assertEqual(arguments[0], "otool")
                return f"{arguments[-1]}:\n" + "".join(f"\t{line}{entry}\n" for line in listings[Path(arguments[-1])])

            with patch.object(packaging, "run", side_effect=otool):
                libraries = packaging.linked_libraries([program])
            self.assertEqual(libraries, {"libavcodec.62.28.100.dylib": ffmpeg / "libavcodec.62.28.100.dylib",
                                         "libavutil.60.dylib": ffmpeg / "libavutil.60.dylib",
                                         "libopus.0.dylib": keg / "libopus.0.dylib"})
            self.assertEqual(packaging.library_formulas(libraries, root / "Cellar"), {"opus"})

            listings[program] = ["@rpath/libavcodec.62.dylib"]
            with patch.object(packaging, "run", side_effect=otool), self.assertRaisesRegex(RuntimeError, "cannot be traced"):
                packaging.linked_libraries([program])

    def test_a_bundled_library_no_notice_covers_fails(self):
        cellar = Path("/opt/homebrew/Cellar")
        self.assertEqual(packaging.library_formulas({
            "libavformat.62.dylib": Path("/prefix/lib/libavformat.62.3.100.dylib"),
            "libvpx.12.dylib": cellar / "libvpx/1.16.0/lib/libvpx.12.dylib",
            "libx264.165.dylib": cellar / "x264/r3222/lib/libx264.165.dylib",
        }, cellar), {"libvpx", "x264"})
        with self.assertRaisesRegex(RuntimeError, "no notice covers it"):
            packaging.library_formulas({"libfoo.1.dylib": Path("/usr/local/lib/libfoo.1.dylib")}, cellar)

    def test_frameworks_must_hold_exactly_the_traced_libraries(self):
        with tempfile.TemporaryDirectory() as temporary:
            frameworks = Path(temporary) / "Frameworks"
            traced = {"libopus.0.dylib": Path("/opus")}
            with self.assertRaises(RuntimeError):
                packaging.check_bundled(frameworks, traced)
            packaging.check_bundled(frameworks, {})
            frameworks.mkdir()
            (frameworks / "libopus.0.dylib").touch()
            packaging.check_bundled(frameworks, traced)
            (frameworks / "libextra.dylib").touch()
            with self.assertRaisesRegex(RuntimeError, "libextra.dylib"):
                packaging.check_bundled(frameworks, traced)

    def test_a_required_formula_that_is_not_bundled_fails(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(packaging, "run") as run:
            with self.assertRaisesRegex(RuntimeError, "libvpx"):
                packaging.collect_formula_notices(Path(temporary), {"opus"}, required=("opus", "libvpx"))
            packaging.collect_formula_notices(Path(temporary), set())
            run.assert_not_called()
            self.assertEqual(list(Path(temporary).iterdir()), [])

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
            plugin = contents / "Resources/PlugIns/ffmpeg"
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

    def test_the_plugins_workspace_contributes_what_the_bundled_plugins_ship(self):
        def package(name):
            return {"id": f"registry+{name}#1.0.0", "name": name, "version": "1.0.0"}

        def node(name, *deps):
            return {"id": f"registry+{name}#1.0.0", "deps": [
                {"pkg": f"registry+{dependency}#1.0.0", "dep_kinds": [{"kind": kind, "target": None}
                                                                        for kind in kinds]}
                for dependency, *kinds in deps]}

        names = ["celesta-codec-ffmpeg-x264", "celesta-codec-ffmpeg", "ez-ffmpeg", "ffmpeg-sys-next", "bindgen", "tempfile",
                 "celesta-codec-avfoundation", "objc2-video-toolbox", "objc2", "criterion",
                 "celesta-plugin-vrm", "gltf", "rayon"]
        metadata = {
            "packages": [package(name) for name in names],
            "workspace_members": [package(name)["id"] for name in
                                  ("celesta-codec-ffmpeg", "celesta-codec-ffmpeg-x264", "celesta-codec-avfoundation", "celesta-plugin-vrm")],
            "resolve": {"nodes": [
                node("celesta-codec-ffmpeg-x264", ("celesta-codec-ffmpeg", None)),
                node("celesta-codec-ffmpeg", ("ez-ffmpeg", None), ("ffmpeg-sys-next", None),
                     ("tempfile", "dev"), ("rayon", None, "dev")),
                node("ez-ffmpeg", ("ffmpeg-sys-next", None)),
                node("ffmpeg-sys-next", ("bindgen", "build")),
                node("bindgen"), node("tempfile"), node("rayon"),
                node("celesta-codec-avfoundation", ("objc2-video-toolbox", None), ("criterion", "dev")),
                node("objc2-video-toolbox", ("objc2", None)),
                node("objc2"), node("criterion"),
                node("celesta-plugin-vrm", ("gltf", None), ("rayon", None)),
                node("gltf"),
            ]},
        }
        # Build dependencies stay listed; a dependency a bundled plugin has
        # only for its tests is not shipped, and neither is another plugin's.
        self.assertEqual(sorted(crate["name"] for crate in packaging.bundled_packages(metadata)),
                         ["bindgen", "celesta-codec-avfoundation", "celesta-codec-ffmpeg", "ez-ffmpeg",
                          "ffmpeg-sys-next", "objc2", "objc2-video-toolbox", "rayon"])

    def test_a_plugins_workspace_without_a_bundled_plugin_fails(self):
        def member(name):
            return {"id": f"registry+{name}#1.0.0", "name": name, "version": "1.0.0"}

        for present in ("celesta-codec-ffmpeg", "celesta-codec-avfoundation"):
            package = member(present)
            with self.assertRaises(RuntimeError, msg=f"only {present}"):
                packaging.bundled_packages({"packages": [package], "workspace_members": [package["id"]],
                                            "resolve": {"nodes": [{"id": package["id"], "deps": []}]}})
        with self.assertRaises(RuntimeError):
            packaging.bundled_packages({"packages": [], "workspace_members": [], "resolve": {"nodes": []}})

    def test_a_plugin_program_is_the_one_executable_its_manifest_names(self):
        with tempfile.TemporaryDirectory() as directory:
            plugin = Path(directory)
            for executable in ({}, {"macos-aarch64": "bin/a", "macos-x86_64": "bin/b"}):
                with self.subTest(executable=executable), self.assertRaises(RuntimeError):
                    (plugin / "celesta-plugin.json").write_text(json.dumps({"executable": executable}))
                    packaging.plugin_program(plugin)


if __name__ == "__main__":
    unittest.main()
