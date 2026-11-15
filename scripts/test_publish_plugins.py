"""Checks for the registry publish client, with no network."""
import base64
from email.parser import BytesParser
from email.policy import HTTP
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import unittest.mock
import zipfile

spec = importlib.util.spec_from_file_location("publish_plugins", Path(__file__).with_name("publish-plugins.py"))
publishing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publishing)

MANIFEST = b'{\n  "id": "ffmpeg",\n  "version": "0.10.0-rc.1"\n}\n'


def archive(path, manifest=MANIFEST, extra=b""):
    with zipfile.ZipFile(path, "x") as zipped:
        zipped.writestr("celesta-plugin.json", manifest)
        zipped.writestr(f"bin/{path.stem}/program", b"program" + extra)


def bundle(*digests):
    statement = {"_type": "https://in-toto.io/Statement/v1", "subject": [
        {"name": f"subject-{index}", "digest": {"sha256": digest}} for index, digest in enumerate(digests)]}
    return json.dumps({"dsseEnvelope": {"payload": base64.b64encode(json.dumps(statement).encode()).decode()}}).encode()


class Plugins(unittest.TestCase):
    def test_a_plugin_is_its_archives_and_their_one_manifest(self):
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary) / "ffmpeg"
            folder.mkdir()
            archive(folder / "macos-aarch64.zip")
            archive(folder / "linux-x86_64.zip", extra=b"!")
            manifest, plugin_id, version, archives = publishing.read_plugin(folder)
        self.assertEqual((manifest, plugin_id, version), (MANIFEST, "ffmpeg", "0.10.0-rc.1"))
        self.assertEqual([target for target, *_ in archives], ["linux-x86_64", "macos-aarch64"])

    def test_archives_with_different_manifests_are_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary) / "ffmpeg"
            folder.mkdir()
            archive(folder / "linux-x86_64.zip")
            archive(folder / "macos-aarch64.zip", manifest=MANIFEST.replace(b"\n", b"\r\n"))
            with self.assertRaisesRegex(publishing.PublishFailure, "differs"):
                publishing.read_plugin(folder)

    def test_a_folder_must_be_named_after_its_plugin(self):
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary) / "ffmpeg-x264"
            folder.mkdir()
            archive(folder / "linux-x86_64.zip")
            with self.assertRaisesRegex(publishing.PublishFailure, "not 'ffmpeg-x264'"):
                publishing.read_plugin(folder)

    def test_bundle_subjects_are_its_statements_digests(self):
        self.assertEqual(publishing.bundle_subjects(bundle("a" * 64, "b" * 64)), {"a" * 64, "b" * 64})
        with self.assertRaises(publishing.PublishFailure):
            publishing.bundle_subjects(b"{}")


class Request(unittest.TestCase):
    def test_body_has_the_manifest_then_each_targets_archive_and_attestation(self):
        archives = [("linux-x86_64", None, b"zip one"), ("macos-aarch64", None, b"zip\r\ntwo")]
        body, content_type = publishing.multipart(publishing.publish_parts(MANIFEST, archives, b"bundle"))
        message = BytesParser(policy=HTTP).parsebytes(f"Content-Type: {content_type}\r\n\r\n".encode() + body)
        parts = [(part.get_param("name", header="content-disposition"), part.get_filename(), part.get_payload(decode=True))
                 for part in message.iter_parts()]
        self.assertEqual(parts, [
            ("manifest", "celesta-plugin.json", MANIFEST),
            ("archive:linux-x86_64", "linux-x86_64.zip", b"zip one"),
            ("attestation:linux-x86_64", "linux-x86_64.sigstore.json", b"bundle"),
            ("archive:macos-aarch64", "macos-aarch64.zip", b"zip\r\ntwo"),
            ("attestation:macos-aarch64", "macos-aarch64.sigstore.json", b"bundle"),
        ])
        self.assertTrue(body.endswith(f"--{content_type.split('boundary=')[1]}--\r\n".encode()))

    def run_publish(self, answers):
        tokens, sent = iter(range(100)), []

        def post(url, token, body, content_type):
            sent.append((url, token))
            return answers.pop(0)

        try:
            return publishing.publish("https://registry.example/", "aud", b"body", "multipart/form-data",
                                      token=lambda audience: f"token-{next(tokens)}", post=post,
                                      sleep=lambda seconds: None), sent
        except publishing.PublishFailure as failure:
            return failure, sent

    def test_created_is_published(self):
        result, sent = self.run_publish([(201, '{"id":"ffmpeg"}')])
        self.assertEqual(result, ("published", '{"id":"ffmpeg"}'))
        self.assertEqual(sent, [("https://registry.example/api/v1/publish", "token-0")])

    def test_version_exists_is_skipped(self):
        result, _ = self.run_publish([(409, '{"error":"version_exists","message":"exists"}')])
        self.assertEqual(result[0], "exists")

    def test_other_refusals_fail_with_the_answer(self):
        for status, text in ((409, '{"error":"other"}'), (422, '{"error":"invalid_archive","message":"x"}'),
                             (500, '{"error":"internal"}'), (403, '{"error":"forbidden"}')):
            result, sent = self.run_publish([(status, text)])
            self.assertIsInstance(result, publishing.PublishFailure)
            self.assertEqual(result.body, text)
            self.assertEqual(len(sent), 1)

    def test_each_retry_has_a_new_token(self):
        result, sent = self.run_publish([(None, "reset"), (503, "unavailable"), (201, "{}")])
        self.assertEqual(result[0], "published")
        self.assertEqual([token for _, token in sent], ["token-0", "token-1", "token-2"])

    def test_retries_stop(self):
        result, sent = self.run_publish([(502, "a"), (502, "b"), (502, "c")])
        self.assertIsInstance(result, publishing.PublishFailure)
        self.assertEqual((len(sent), result.body), (3, "c"))

    def test_version_exists_after_a_retry_fails(self):
        result, _ = self.run_publish([(None, "timed out"), (409, '{"error":"version_exists"}')])
        self.assertIsInstance(result, publishing.PublishFailure)

    def test_summary_shows_a_failures_answer(self):
        text = publishing.summary([("ffmpeg", "0.10.0", ["linux-x86_64"], "failed: 422", '{"error":"invalid_archive"}'),
                                   ("avfoundation", "0.10.0", ["macos-aarch64"], "already published (409), skipped", "")])
        self.assertIn("| `ffmpeg` | `0.10.0` | `linux-x86_64` | failed: 422 |", text)
        self.assertIn('{"error":"invalid_archive"}', text)
        self.assertEqual(text.count("<details>"), 1)


class DryRun(unittest.TestCase):
    def test_an_archive_the_bundle_does_not_name_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "dist/ffmpeg").mkdir(parents=True)
            archive(root / "dist/ffmpeg/linux-x86_64.zip")
            digest = hashlib.sha256((root / "dist/ffmpeg/linux-x86_64.zip").read_bytes()).hexdigest()
            (root / "good.json").write_bytes(bundle(digest))
            (root / "bad.json").write_bytes(bundle("0" * 64))
            argv = ["publish-plugins.py", str(root / "dist"), "--dry-run", "--attestation"]
            with unittest.mock.patch("sys.argv", argv + [str(root / "good.json")]), \
                 unittest.mock.patch.dict("os.environ", {"GITHUB_STEP_SUMMARY": str(root / "summary.md")}):
                self.assertEqual(publishing.main(), 0)
            self.assertIn("dry run", (root / "summary.md").read_text())
            with unittest.mock.patch("sys.argv", argv + [str(root / "bad.json")]):
                with self.assertRaisesRegex(publishing.PublishFailure, "does not name"):
                    publishing.main()


if __name__ == "__main__":
    unittest.main()
