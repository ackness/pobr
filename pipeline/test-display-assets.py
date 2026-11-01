"""Synthetic contracts for independently stored support display data."""

import copy
from pathlib import Path
import tempfile
import unittest

import data_bundle as bundle
import display_assets as display
import check_data_ownership as ownership
import runtime_pack as pack


class DisplayAssetsTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name).resolve()
        source = self.root / "source/data/9.1"
        (source / "overlay").mkdir(parents=True)
        catalog = b'{"gems":[{"skill_id":"synthetic","description":"English"}]}'
        (source / "overlay/trade_catalog.json").write_bytes(catalog)
        snapshot = {"schema_version": 3, "poe_version": "9.1", "languages": [],
                    "domains": {"base": [], "overlay": ["trade_catalog"]},
                    "files": {"overlay/trade_catalog.json": bundle.sha256(catalog)}}
        (source / "manifest.json").write_bytes(bundle.encoded(snapshot))
        self.prepared = self.root / "prepared"
        receipt = bundle.create(self.root / "source", self.prepared, "9.1")
        provenance = {"generation_data_version": "8.1", "source": "synthetic",
                      "source_commit": "a" * 40, "source_file": "GemEffects.json", "source_sha256": "b" * 64}
        self.asset = {"_meta": {**{k: v for k, v in provenance.items() if k != "generation_data_version"},
                                "data_version": "8.1", "catalog_sha256": bundle.sha256(catalog)},
                      "entries": {"synthetic": {"en": "English", "zh_cn": "合成"}}, "untranslated": []}
        self.manifest = {"schema": "poe2-support-display-publication/v1", "locale": "zh-CN",
                         "target": {"game": "poe2", "version": "9.1", "realm": "international"},
                         "catalog_sha256": bundle.sha256(catalog), "provenance": provenance}
        self.lock = {"target": {"game": "poe2", "game_patch": "9.1", "realm": "international"},
                     "runtime_identity": receipt["runtime_identity"],
                     "support_display": {"repository": "ackness/poe2-zh-cn-stats", "publication": "published",
                         "revision": "a" * 40, "path": "display/manifest.json",
                         "catalog_sha256": bundle.sha256(catalog), "source_data_version": "8.1"}}
        self.lock_path, self.cache = self.root / "lock.json", self.root / "downloads"
        self.output = self.root / "web/support.json"
        self.output.parent.mkdir()
        self.output.write_bytes(b"previous")
        self.save()

    @staticmethod
    def pin(raw):
        return {"bytes": len(raw), "sha256": bundle.sha256(raw)}

    def save(self):
        raw = bundle.encoded(self.asset)
        self.manifest["files"] = {display.ASSET: self.pin(raw)}
        manifest = bundle.encoded(self.manifest)
        self.lock["support_display"].update(asset=self.pin(raw), manifest=self.pin(manifest))
        self.files = {"display/manifest.json": manifest, "display/" + display.ASSET: raw}
        self.lock_path.write_bytes(bundle.encoded(self.lock))

    def fetch(self, address):
        self.assertEqual(address[:2], ("https://github.com/ackness/poe2-zh-cn-stats.git", "a" * 40))
        return self.files[address[2]]

    def prepare(self, **kwargs):
        return display.prepare(self.lock_path, self.prepared, self.cache, fetcher=self.fetch, **kwargs)

    def test_import_and_offline_replay_preserve_source_version_and_bytes(self):
        raw = self.prepare()
        self.assertEqual(raw, bundle.encoded(self.asset))
        self.assertEqual(bundle.decode(raw)["_meta"]["data_version"], "8.1")
        replay = display.prepare(self.lock_path, self.prepared, self.cache, offline=True,
                                 fetcher=lambda _: self.fail("offline fetched"))
        self.assertEqual(replay, raw)
        display.install(raw, self.output)
        self.assertEqual(self.output.read_bytes(), raw)

    def test_wrong_target_catalog_and_relabelled_provenance_reject(self):
        original = copy.deepcopy((self.manifest, self.asset))
        for mutation in (lambda: self.manifest["target"].update(realm="china"),
                         lambda: self.manifest.update(catalog_sha256="c" * 64),
                         lambda: self.asset["_meta"].update(data_version="9.1")):
            self.manifest, self.asset = copy.deepcopy(original)
            mutation()
            self.save()
            with self.assertRaises(ValueError):
                self.prepare()
            self.assertEqual(self.output.read_bytes(), b"previous")

    def test_corrupt_asset_and_wrong_owner_reject(self):
        self.files["display/" + display.ASSET] = b"corrupt"
        with self.assertRaises(ValueError):
            self.prepare()
        self.lock["support_display"]["repository"] = "ackness/pobr"
        self.lock_path.write_bytes(bundle.encoded(self.lock))
        with self.assertRaisesRegex(ValueError, "Independent"):
            self.prepare()
        self.assertEqual(self.output.read_bytes(), b"previous")

    def test_output_symlink_and_forced_source_tracking_reject(self):
        link = self.root / "link.json"
        link.symlink_to(self.output)
        with self.assertRaises(ValueError):
            display.install(b"new", link)
        self.assertEqual(self.output.read_bytes(), b"previous")
        with self.assertRaisesRegex(ValueError, "External data"):
            ownership.check(["web/src/lib/" + display.ASSET], {"shared_files": {}})
        with self.assertRaisesRegex(ValueError, "External data"):
            ownership.check(["web/src/fixtures/catalog.json"], {"shared_files": {}})

    def fixture_archive(self, target=None):
        raw = b'[{"id":"synthetic"}]'
        manifest = bundle.encoded({"schema": "repoe-data-publication/v1",
                                  "target": target or self.lock["target"],
                                  "files": {"catalog.json": self.pin(raw)}})
        archive = pack.archive_bytes({"manifest.json": manifest, "catalog.json": raw})
        self.lock["test_fixtures"] = {"repository": "ackness/repoe", "revision": "a" * 40,
            "publication": "published", "path": "fixtures.zip", **self.pin(archive),
            "manifest": self.pin(manifest), "expanded_bytes": len(raw) + len(manifest), "files": ["catalog.json"]}
        self.lock_path.write_bytes(bundle.encoded(self.lock))
        return archive, raw

    def test_external_api_fixtures_install_and_offline_replay(self):
        archive, raw = self.fixture_archive()
        def fetch(address):
            self.assertEqual(address, ("https://github.com/ackness/repoe.git", "a" * 40, "fixtures.zip"))
            return archive
        files = display.prepare_test_fixtures(self.lock_path, self.cache, fetcher=fetch)
        self.assertEqual(files, {"catalog.json": raw})
        self.assertEqual(display.prepare_test_fixtures(self.lock_path, self.cache, offline=True,
                         fetcher=lambda _: self.fail("offline fetched")), files)
        out = self.root / "fixtures"
        display.install_test_fixtures(files, out)
        self.assertEqual((out / "catalog.json").read_bytes(), raw)
        (out / "unlisted.json").write_bytes(b"[]")
        with self.assertRaisesRegex(ValueError, "Unlisted"):
            display.install_test_fixtures(files, out)

    def test_api_fixture_wrong_target_and_directory_escape_reject(self):
        archive, _ = self.fixture_archive({**self.lock["target"], "realm": "china"})
        with self.assertRaisesRegex(ValueError, "target/inventory"):
            display.prepare_test_fixtures(self.lock_path, self.cache, fetcher=lambda _: archive)
        self.lock["test_fixtures"]["files"] = ["../escape.json"]
        self.lock_path.write_bytes(bundle.encoded(self.lock))
        with self.assertRaises(ValueError):
            display.prepare_test_fixtures(self.lock_path, self.cache, fetcher=lambda _: self.fail("unsafe fetched"))


if __name__ == "__main__":
    unittest.main()
