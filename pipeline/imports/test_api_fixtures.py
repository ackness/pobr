"""Synthetic contracts for independently stored API test fixtures."""

from pathlib import Path
import tempfile
import unittest

from pipeline.runtime import data_bundle as bundle
from pipeline.imports import api_fixtures as fixtures
from pipeline.imports import data_imports
from pipeline.runtime import runtime_pack as pack


class ApiFixturesTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        self.lock = {"target": {"game": "poe2", "game_patch": "9.1", "realm": "international"}}
        self.lock_path, self.cache = self.root / "lock.json", self.root / "downloads"

    pin = staticmethod(pack.pin)

    def fixture_archive(self, target=None):
        raw = b'[{"id":"synthetic"}]'
        manifest = bundle.encoded({"schema": "repoe-data-publication/v1",
                                  "target": target or self.lock["target"],
                                  "files": {"catalog.json": self.pin(raw)}})
        archive = pack.archive_bytes({"manifest.json": manifest, "catalog.json": raw})
        self.lock["test_fixtures"] = {"repository": "ackness/repoe", "revision": "a" * 40,
            "path": "fixtures.zip", **self.pin(archive),
            "manifest": self.pin(manifest), "expanded_bytes": len(raw) + len(manifest), "files": ["catalog.json"]}
        self.lock_path.write_bytes(bundle.encoded(self.lock))
        return archive, raw

    def test_external_api_fixtures_install_and_offline_replay(self):
        archive, raw = self.fixture_archive()
        def fetch(address):
            self.assertEqual(address, ("https://github.com/ackness/repoe.git", "a" * 40, "fixtures.zip"))
            return archive
        files = fixtures.prepare(self.lock_path, self.cache, fetcher=fetch)
        self.assertEqual(files, {"catalog.json": raw})
        self.assertEqual(fixtures.prepare(self.lock_path, self.cache, offline=True,
                         fetcher=lambda _: self.fail("offline fetched")), files)
        out = self.root / "fixtures"
        fixtures.install(files, out)
        self.assertEqual((out / "catalog.json").read_bytes(), raw)
        (out / "unlisted.json").write_bytes(b"[]")
        with self.assertRaisesRegex(ValueError, "Unlisted"):
            fixtures.install(files, out)

    def test_api_fixtures_keep_the_target_they_were_published_for(self):
        """A new game patch keeps the patch-independent fixtures; the entry records their target."""
        published = dict(self.lock["target"])
        archive, raw = self.fixture_archive()
        self.lock["target"] = {**published, "game_patch": "9.2"}
        self.lock_path.write_bytes(bundle.encoded(self.lock))
        with self.assertRaisesRegex(ValueError, "target/inventory"):
            fixtures.prepare(self.lock_path, self.cache, fetcher=lambda _: archive)
        self.lock["test_fixtures"]["target"] = published
        self.lock_path.write_bytes(bundle.encoded(self.lock))
        self.assertEqual(fixtures.prepare(self.lock_path, self.cache, fetcher=lambda _: archive),
                         {"catalog.json": raw})
        self.lock["test_fixtures"]["target"] = {"game_patch": "9.1"}
        self.lock_path.write_bytes(bundle.encoded(self.lock))
        with self.assertRaisesRegex(ValueError, "Invalid test fixture target"):
            fixtures.prepare(self.lock_path, self.cache, fetcher=lambda _: archive)

    def test_api_fixtures_from_a_release(self):
        archive, raw = self.fixture_archive()
        spec = self.lock["test_fixtures"]
        del spec["revision"], spec["path"]
        spec.update(tag="fixtures/1", asset="pobr-test-fixtures.zip")
        self.lock_path.write_bytes(bundle.encoded(self.lock))
        def fetch(address, **options):
            self.assertEqual(address, data_imports.ReleaseAsset("ackness/repoe", "fixtures/1",
                                                                "pobr-test-fixtures.zip"))
            return archive
        self.assertEqual(fixtures.prepare(self.lock_path, self.cache, fetcher=fetch),
                         {"catalog.json": raw})

    def test_api_fixture_wrong_target_and_directory_escape_reject(self):
        archive, _ = self.fixture_archive({**self.lock["target"], "realm": "china"})
        with self.assertRaisesRegex(ValueError, "target/inventory"):
            fixtures.prepare(self.lock_path, self.cache, fetcher=lambda _: archive)
        self.lock["test_fixtures"]["files"] = ["../escape.json"]
        self.lock_path.write_bytes(bundle.encoded(self.lock))
        with self.assertRaises(ValueError):
            fixtures.prepare(self.lock_path, self.cache, fetcher=lambda _: self.fail("unsafe fetched"))

    def test_absent_fixtures_are_skipped(self):
        self.lock_path.write_bytes(bundle.encoded(self.lock))
        self.assertIsNone(fixtures.prepare(self.lock_path, self.cache,
                                          fetcher=lambda _: self.fail("absent fixtures fetched")))
        output = self.root / "fixtures"
        fixtures.install(None, output)
        self.assertFalse(output.exists())

    def test_missing_fixture_metadata_has_a_named_error(self):
        self.fixture_archive()
        original = self.lock["test_fixtures"].copy()
        for field, message in (("manifest", "Missing test_fixtures manifest pin"),
                               ("files", "Invalid test fixture inventory"),
                               ("expanded_bytes", "Invalid test fixture expanded size")):
            with self.subTest(field=field):
                self.lock["test_fixtures"] = {k: v for k, v in original.items() if k != field}
                self.lock_path.write_bytes(bundle.encoded(self.lock))
                with self.assertRaisesRegex(ValueError, message):
                    fixtures.prepare(self.lock_path, self.cache,
                                     fetcher=lambda _: self.fail("invalid fixtures fetched"))


if __name__ == "__main__":
    unittest.main()
