#!/usr/bin/env python3
"""Publish first-party plugin archives to registry.celesta.studio.

    python3 scripts/publish-plugins.py --attestation <bundle> [--dry-run] <archives>

<archives> holds one folder per plugin, named after its id, with one
<target>.zip per target, as `package-media-plugins.py --registry` writes
them. Each plugin is one `POST <registry>/api/v1/publish`, a
multipart/form-data body with a Content-Length: `manifest`, the
celesta-plugin.json at the root of every archive (the same bytes in each, or
nothing is sent), `archive:<target>` for each ZIP, and `attestation:<target>`
for each, the Sigstore bundle of `actions/attest-build-provenance` from this
run, which must name every archive's SHA-256 as a subject. One bundle may
cover every archive of the release.

It runs in GitHub Actions, in a job with `id-token: write`: each request,
retries included, carries a new OIDC token for the registry's audience, since
the registry accepts a token once. A `409 version_exists` means the version
is already there (an earlier run, or a plugin whose version this release did
not change) and is skipped, so a failed run can be re-run. Any other answer
but 2xx fails the run once every plugin was tried; the registry's answer is
printed and written to the job summary. --dry-run checks the archives and
the bundle and sends nothing.
"""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import secrets
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile

REGISTRY = "https://registry.celesta.studio"
MANIFEST = "celesta-plugin.json"
# Answers that mean the request did not reach the Worker, so sending it again
# cannot meet a version an earlier attempt reserved.
RETRY_STATUSES = {502, 503, 504}
ATTEMPTS = 3


class PublishFailure(Exception):
    """A plugin that could not be published, with what the registry said."""

    def __init__(self, message, body=""):
        super().__init__(message)
        self.body = body


def read_plugin(folder):
    """The plugin in `folder`: its manifest bytes, id, version, and its
    archives as (target, path, bytes), by target. Fails unless every archive
    has the same celesta-plugin.json at its root, whose id is the folder's."""
    archives = []
    manifest = None
    for path in sorted(folder.glob("*.zip")):
        data = path.read_bytes()
        with zipfile.ZipFile(path) as archive:
            try:
                inner = archive.read(MANIFEST)
            except KeyError:
                raise PublishFailure(f"{path} has no {MANIFEST} at its root") from None
        if manifest is None:
            manifest = inner
        elif inner != manifest:
            raise PublishFailure(f"{path}'s {MANIFEST} differs from {archives[0][1]}'s; every target must carry the same")
        archives.append((path.stem, path, data))
    if not archives:
        raise PublishFailure(f"{folder} has no archives")
    parsed = json.loads(manifest)
    if parsed.get("id") != folder.name:
        raise PublishFailure(f"{folder}'s archives are for {parsed.get('id')!r}, not {folder.name!r}")
    return manifest, parsed["id"], parsed["version"], archives


def bundle_subjects(bundle):
    """The SHA-256 digests the bundle's in-toto statement names as subjects."""
    try:
        envelope = json.loads(bundle)["dsseEnvelope"]
        statement = json.loads(base64.b64decode(envelope["payload"]))
        return {subject["digest"]["sha256"] for subject in statement["subject"] if "sha256" in subject.get("digest", {})}
    except (ValueError, KeyError, TypeError) as error:
        raise PublishFailure(f"the attestation bundle is not a Sigstore bundle of an in-toto statement: {error}") from None


def multipart(parts):
    """`parts`, as (name, filename, content type, bytes), as a
    multipart/form-data body and its Content-Type."""
    while True:
        boundary = f"celesta-{secrets.token_hex(16)}"
        if not any(boundary.encode() in data for *_, data in parts):
            break
    body = bytearray()
    for name, filename, content_type, data in parts:
        body += (f"--{boundary}\r\n"
                 f'Content-Disposition: form-data; name="{name}"; filename="{filename}"\r\n'
                 f"Content-Type: {content_type}\r\n\r\n").encode()
        body += data + b"\r\n"
    body += f"--{boundary}--\r\n".encode()
    return bytes(body), f"multipart/form-data; boundary={boundary}"


def publish_parts(manifest, archives, bundle):
    """The publish request's parts: the manifest, then each target's archive
    and attestation."""
    parts = [("manifest", MANIFEST, "application/json", manifest)]
    for target, _, data in archives:
        parts.append((f"archive:{target}", f"{target}.zip", "application/zip", data))
        parts.append((f"attestation:{target}", f"{target}.sigstore.json", "application/json", bundle))
    return parts


def oidc_token(audience):
    """A new GitHub Actions OIDC token for `audience`, masked in the log."""
    url = os.environ.get("ACTIONS_ID_TOKEN_REQUEST_URL")
    request_token = os.environ.get("ACTIONS_ID_TOKEN_REQUEST_TOKEN")
    if not url or not request_token:
        raise PublishFailure("no OIDC token: run in GitHub Actions with `permissions: id-token: write`")
    separator = "&" if "?" in url else "?"
    request = urllib.request.Request(f"{url}{separator}audience={urllib.parse.quote(audience, safe='')}",
                                     headers={"Authorization": f"bearer {request_token}"})
    with urllib.request.urlopen(request, timeout=60) as response:
        token = json.load(response)["value"]
    print(f"::add-mask::{token}", flush=True)
    return token


def send(url, token, body, content_type):
    """POST `body`; the status and the answer's text, or None for the status
    when no answer came."""
    request = urllib.request.Request(url, data=body, method="POST", headers={
        "Authorization": f"Bearer {token}", "Content-Type": content_type,
        "Content-Length": str(len(body)), "User-Agent": "celesta-plugin-release",
    })
    try:
        with urllib.request.urlopen(request, timeout=600) as response:
            return response.status, response.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode("utf-8", "replace")
    except (urllib.error.URLError, TimeoutError, ConnectionError) as error:
        return None, str(error)


def error_code(text):
    try:
        return json.loads(text).get("error")
    except (ValueError, AttributeError):
        return None


def publish(registry, audience, body, content_type, *, token=oidc_token, post=send, sleep=time.sleep):
    """Send one publish request, retrying when no answer came or a gateway
    failed, each time with a new token. "published" or "exists"; raises
    PublishFailure for anything else."""
    url = f"{registry.rstrip('/')}/api/v1/publish"
    for attempt in range(1, ATTEMPTS + 1):
        status, text = post(url, token(audience), body, content_type)
        if status is not None and 200 <= status < 300:
            return "published", text
        if status == 409 and error_code(text) == "version_exists":
            if attempt == 1:
                return "exists", text
            # An earlier attempt may have reserved the version and failed, or
            # published it; a re-run of the job tells them apart.
            raise PublishFailure("409 version_exists after a retry: the earlier attempt may have published it, "
                                 "or left it pending. Re-run the job: a published version is then skipped", text)
        if (status is None or status in RETRY_STATUSES) and attempt < ATTEMPTS:
            print(f"Attempt {attempt} got {status or 'no answer'}: {text}; retrying with a new token", flush=True)
            sleep(10 * attempt)
            continue
        raise PublishFailure(f"{status or 'no answer'}", text)
    raise AssertionError("unreachable")


def summary(rows):
    lines = ["## Registry publish", "", "| Plugin | Version | Targets | Result |", "| --- | --- | --- | --- |"]
    details = []
    for plugin_id, version, targets, result, body in rows:
        lines.append(f"| `{plugin_id}` | `{version}` | {', '.join(f'`{t}`' for t in targets)} | {result} |")
        if body:
            details += ["", f"<details><summary>{plugin_id}: the registry's answer</summary>", "",
                        "```json", body.strip(), "```", "", "</details>"]
    return "\n".join(lines + details) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("archives", type=Path, help="One folder per plugin id, each with <target>.zip files")
    parser.add_argument("--attestation", type=Path, required=True,
                        help="The Sigstore bundle (attest-build-provenance's bundle-path) covering every archive")
    parser.add_argument("--registry", default=REGISTRY)
    parser.add_argument("--audience", default=REGISTRY, help="The OIDC token's audience")
    parser.add_argument("--dry-run", action="store_true", help="Check the archives and the bundle; send nothing")
    args = parser.parse_args()

    bundle = args.attestation.read_bytes()
    subjects = bundle_subjects(bundle)
    folders = sorted(path for path in args.archives.iterdir() if path.is_dir())
    if not folders:
        parser.error(f"{args.archives} holds no plugin folders")
    # Check every plugin before sending anything.
    plugins = []
    for folder in folders:
        manifest, plugin_id, version, archives = read_plugin(folder)
        for target, path, data in archives:
            digest = hashlib.sha256(data).hexdigest()
            if digest not in subjects:
                raise PublishFailure(f"the attestation bundle does not name {path} (sha256 {digest})")
        plugins.append((manifest, plugin_id, version, archives))

    rows, failed = [], False
    for manifest, plugin_id, version, archives in plugins:
        targets = [target for target, *_ in archives]
        body, content_type = multipart(publish_parts(manifest, archives, bundle))
        print(f"{plugin_id} {version}: {', '.join(targets)} ({len(body)} bytes)", flush=True)
        if args.dry_run:
            rows.append((plugin_id, version, targets, "dry run, not sent", ""))
            continue
        try:
            result, text = publish(args.registry, args.audience, body, content_type)
        except PublishFailure as failure:
            print(f"::error title={plugin_id} {version} was not published::{failure}", flush=True)
            print(failure.body, flush=True)
            rows.append((plugin_id, version, targets, f"failed: {failure}", failure.body))
            failed = True
            continue
        if result == "exists":
            print(f"::notice title={plugin_id} {version}::already in the registry (409 version_exists), skipped", flush=True)
            rows.append((plugin_id, version, targets, "already published (409), skipped", ""))
        else:
            print(text, flush=True)
            rows.append((plugin_id, version, targets, "published", text))

    text = summary(rows)
    print(text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as file:
            file.write(text)
    return 1 if failed else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except PublishFailure as failure:
        print(f"::error::{failure}", file=sys.stderr)
        sys.exit(1)
