"""Offline tests for the pinned Release and Git transports shared by every owner import."""

import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import io
from pathlib import Path
import tempfile
import threading
import unittest
import zipfile

from pipeline.imports import data_imports as imports


def pin(raw):
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


SOURCE = {"repository": "ackness/repoe", "revision": "a" * 40}
TAG = "game/9.1-1"
ASSET = "pobr-game-9.1.zip"


class ReleaseServer:
    """Serve `<root>/<owner>/<repo>/releases/download/<tag>/<asset>` and count requests."""

    def __init__(self, root):
        self.root, self.requests = Path(root), []
        server = self

        class Handler(SimpleHTTPRequestHandler):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, directory=str(server.root), **kwargs)

            def do_GET(self):
                server.requests.append(self.path)
                super().do_GET()

            def log_message(self, *args):
                pass

        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()
        self.base = f"http://127.0.0.1:{self.httpd.server_address[1]}"

    def close(self):
        self.httpd.shutdown()
        self.httpd.server_close()

    def publish(self, repository, tag, assets, sums=None):
        directory = self.root / repository / "releases/download" / tag
        directory.mkdir(parents=True, exist_ok=True)
        for name, raw in assets.items():
            (directory / name).write_bytes(raw)
        if sums is None:
            sums = {name: hashlib.sha256(raw).hexdigest() for name, raw in assets.items()}
        (directory / "SHA256SUMS").write_text("".join(f"{d}  {n}\n" for n, d in sums.items()))
        return directory


class TransportTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.cache = self.root / "cache"
        self.cache.mkdir()
        self.calls = []
        self.remote = {}

    def fetch(self, address):
        self.calls.append(address)
        return self.remote[address]

    def publish(self, name, raw):
        self.remote[imports.git_address(SOURCE, name)] = raw
        return pin(raw)

    def test_fetch_is_pinned_cached_and_replayed_offline(self):
        expected = self.publish("package.zip", b"payload")
        self.assertEqual(imports.acquire(SOURCE, "package.zip", expected, self.cache, False, self.fetch), b"payload")
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(imports.acquire(SOURCE, "package.zip", expected, self.cache, True, self.fetch), b"payload")
        self.assertEqual(len(self.calls), 1)

    def test_corrupt_cache_never_falls_back_to_network(self):
        expected = self.publish("package.zip", b"payload")
        (self.cache / expected["sha256"]).write_bytes(b"corrupt")
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            imports.acquire(SOURCE, "package.zip", expected, self.cache, False, self.fetch)
        self.assertEqual(self.calls, [])

    def test_wrong_remote_bytes_are_not_cached(self):
        expected = pin(b"payload")
        self.remote[imports.git_address(SOURCE, "package.zip")] = b"other"
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            imports.acquire(SOURCE, "package.zip", expected, self.cache, False, self.fetch)
        self.assertFalse((self.cache / expected["sha256"]).exists())

    def test_offline_cache_miss_moving_refs_and_bad_pins_reject(self):
        with self.assertRaisesRegex(ValueError, "Offline cache miss"):
            imports.acquire(SOURCE, "package.zip", pin(b"x"), self.cache, True, self.fetch)
        with self.assertRaisesRegex(ValueError, "immutable"):
            imports.git_address({"repository": "ackness/repoe", "revision": "master"}, "tables.json")
        with self.assertRaisesRegex(ValueError, "Invalid repository"):
            imports.git_address({"repository": "../outside", "revision": "a" * 40}, "tables.json")
        with self.assertRaisesRegex(ValueError, "Invalid input pin"):
            imports.acquire(SOURCE, "package.zip", {"bytes": 1, "sha256": "../../outside"},
                            self.cache, False, self.fetch)
        self.assertEqual(self.calls, [])

    def test_archive_paths_sizes_and_unlisted_members_reject(self):
        for member in ["../outside", "extra.json"]:
            raw = io.BytesIO()
            with zipfile.ZipFile(raw, "w") as archive:
                archive.writestr("manifest.json", '{"files":{}}')
                archive.writestr(member, "bad")
            spec = {"manifest": pin(b'{"files":{}}'), "expanded_bytes": 15}
            with self.assertRaises(ValueError):
                imports.unpack(raw.getvalue(), spec, self.root / "rejected")
        self.assertFalse((self.root / "outside").exists())
        body = b'{"a":1}'
        manifest = ('{"files":{"a.json":%s}}' % str(pin(body)).replace("'", '"')).encode()
        raw = io.BytesIO()
        with zipfile.ZipFile(raw, "w") as archive:
            archive.writestr("manifest.json", manifest)
            archive.writestr("a.json", body)
        spec = {"manifest": pin(manifest), "expanded_bytes": len(manifest) + len(body)}
        self.assertEqual(set(imports.unpack(raw.getvalue(), spec, self.root / "ok")["files"]), {"a.json"})
        self.assertEqual((self.root / "ok/a.json").read_bytes(), body)
        with self.assertRaisesRegex(ValueError, "Expanded archive size"):
            imports.unpack(raw.getvalue(), {**spec, "expanded_bytes": 1}, self.root / "size")


class ReleaseTransportTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.server = ReleaseServer(self.root / "srv")
        self.addCleanup(self.server.close)
        self.releases = imports.ReleaseInputs(base=self.server.base)
        self.cache = self.root / "cache"
        self.cache.mkdir()

    def test_release_asset_is_pinned_cached_and_replayed_offline(self):
        self.server.publish("ackness/repoe", TAG, {ASSET: b"payload"})
        spec = {"repository": "ackness/repoe", "tag": TAG, "asset": ASSET,
                "bytes": 7, "sha256": hashlib.sha256(b"payload").hexdigest()}
        self.assertEqual(imports.acquire(spec, None, spec, self.cache, False, self.releases), b"payload")
        self.assertEqual(imports.acquire(spec, None, spec, self.cache, True, self.releases), b"payload")
        self.assertEqual(len(self.server.requests), 1)
        self.assertIn("/ackness/repoe/releases/download/game/9.1-1/pobr-game-9.1.zip", self.server.requests[0])

    def test_size_sha_and_missing_asset_reject_without_caching(self):
        self.server.publish("ackness/repoe", TAG, {ASSET: b"payload"})
        good = {"repository": "ackness/repoe", "tag": TAG, "asset": ASSET}
        for name, pin in (("size", {"bytes": 6, "sha256": hashlib.sha256(b"payload").hexdigest()}),
                          ("sha", {"bytes": 7, "sha256": "0" * 64})):
            with self.subTest(name), self.assertRaisesRegex(ValueError, "bytes/SHA256"):
                imports.acquire({**good, **pin}, None, pin, self.cache, False, self.releases)
            self.assertFalse((self.cache / pin["sha256"]).exists())
        missing = {**good, "asset": "absent.zip", "bytes": 1, "sha256": "1" * 64}
        with self.assertRaisesRegex(ValueError, "Cannot download"):
            imports.acquire(missing, None, missing, self.cache, False, self.releases)
        with self.assertRaisesRegex(ValueError, "size limit"):
            self.releases(imports.release_address(good), max_bytes=3)

    def test_addresses_reject_unsafe_names_and_mixed_transports(self):
        for spec in ({"repository": "../x", "tag": TAG, "asset": ASSET},
                     {"repository": "ackness/repoe", "tag": "bad tag", "asset": ASSET},
                     {"repository": "ackness/repoe", "tag": TAG, "asset": "../x.zip"},
                     {"repository": "ackness/repoe", "tag": TAG, "asset": ASSET, "path": "x.zip"}):
            with self.subTest(spec=spec), self.assertRaises(ValueError):
                imports.source_address(spec)
        git = imports.source_address({"repository": "ackness/repoe", "revision": "a" * 40}, "x.zip")
        self.assertNotIsInstance(git, imports.ReleaseAsset)

    def test_sha256sums_parses_gnu_text_and_binary_modes(self):
        digest = "a" * 64
        self.assertEqual(imports.parse_sums(f"{digest}  one.zip\n{digest} *receipt.json\n".encode()),
                         {"one.zip": digest, "receipt.json": digest})
        for raw in (b"", f"{digest} one.zip\n".encode(), f"{digest}  ../one.zip\n".encode(),
                    f"{digest}  a.zip\n{digest}  a.zip\n".encode(), b"xyz  a.zip\n"):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                imports.parse_sums(raw)


if __name__ == "__main__":
    unittest.main()
