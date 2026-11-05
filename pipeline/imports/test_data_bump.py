"""Offline tests for `data:bump`, served by a local HTTP fixture Release."""

import hashlib
import json
import unittest

from pipeline.imports import data_bump as bump
from pipeline.imports import data_imports as imports
from pipeline.imports import display_assets as display
from pipeline.runtime import build_data as build
from pipeline.runtime import data_bundle as bundle
from pipeline.runtime import runtime_pack as pack
from pipeline.imports.test_data_imports import ReleaseServer
from pipeline.runtime.test_build_data import BuildDataTests

TAG = "game/9.1-1"
ASSET = "pobr-game-9.1.zip"


TARGET = {"game": "poe2", "game_patch": "9.1", "realm": "international"}


def receipt(commit="b" * 40, target=TARGET, drop=(), extra=None, schema="repoe-release-receipt/v1", **producer):
    found = {"repository": "ackness/repoe", "commit": commit, "dirty": False,
             "contract": {"version": build.contract_version(), "tag": "contract/1.0"}, **producer}
    for key in drop:
        del found[key]
    return json.dumps({"schema": schema, "target": target, "producer": found, **(extra or {})}).encode()


LOCALE_RECEIPT = "poe2-locale-release-receipt/v1"


def locale_receipt(spec, archive, target=TARGET, extra=None, package=True):
    """The translation repository's receipt: no contract, always a `package` pin."""
    pinned = {"asset": spec["asset"] if "asset" in spec else "pobr-locale-9.1.zip", **pack.pin(archive),
              "expanded_bytes": spec["expanded_bytes"], "manifest": spec["manifest"], "members": {}, "index": {}}
    return receipt(target=target, repository="ackness/poe2-zh-cn-stats", drop=("contract",), schema=LOCALE_RECEIPT,
                   extra={**({"package": pinned} if package else {}), **(extra or {})})


def support_display(target, catalog, drift=None):
    """A language owner's support display assets for `target` and trade catalog bytes."""
    provenance = {"kind": "generated-asset", "generation_data_version": target["game_patch"],
                  "source": "synthetic", "source_commit": "c" * 40, "source_file": "GemEffects.json",
                  "source_sha256": "d" * 64}
    asset = {"_meta": {**{k: v for k, v in provenance.items() if k not in ("kind", "generation_data_version")},
                       "data_version": target["game_patch"], "catalog_sha256": bundle.sha256(catalog)},
             "entries": {"synthetic": {"en": "English", "zh_cn": "合成"}}, "untranslated": []}
    manifest = {"schema": display.MANIFEST_SCHEMA, "locale": "zh-CN",
                "target": {"game": target["game"], "version": target["game_patch"], "realm": target["realm"]},
                "catalog_sha256": bundle.sha256(catalog), "provenance": provenance}
    if drift:
        drift(manifest, asset)
    asset_raw = bundle.encoded(asset)
    manifest["files"] = {display.ASSET: pack.pin(asset_raw)}
    return {display.MANIFEST: bundle.encoded(manifest), display.ASSET: asset_raw}


class DataBumpTests(BuildDataTests):
    """Bump the synthetic lock's game entry to a Release carrying the same or new bytes."""

    def setUp(self):
        super().setUp()
        self.server = ReleaseServer(self.root / "srv")
        self.addCleanup(self.server.close)

    def owner_inputs(self):
        git = self.fetch
        releases = imports.ReleaseInputs(base=self.server.base)
        return lambda address, **options: (releases(address, **options)
                                           if isinstance(address, imports.ReleaseAsset) else git(address))

    def publish_game(self, archive=None, **receipt_fields):
        archive = self.archives["game"] if archive is None else archive
        return self.server.publish("ackness/repoe", TAG, {ASSET: archive, "receipt.json": receipt(**receipt_fields)})

    def run_bump(self, pairs=(("ackness/repoe", TAG),)):
        return bump.bump(self.lock_path, list(pairs), self.cache, self.shared, fetcher=self.owner_inputs())

    def test_bump_rewrites_only_the_owner_entry_and_keeps_identity(self):
        self.publish_game()
        before = bundle.decode(self.lock_path.read_bytes())
        result = self.run_bump()
        after = bundle.decode(self.lock_path.read_bytes())
        game = after["packages"]["game"]
        self.assertEqual(game, {"repository": "ackness/repoe", "tag": TAG, "asset": ASSET,
                                **pack.pin(self.archives["game"]),
                                "manifest": before["packages"]["game"]["manifest"],
                                "expanded_bytes": before["packages"]["game"]["expanded_bytes"],
                                "receipt": pack.pin(receipt()), "producer_commit": "b" * 40,
                                "contract_version": build.contract_version()})
        self.assertEqual({k: v for k, v in after.items() if k != "packages"},
                         {k: v for k, v in before.items() if k != "packages"})
        self.assertEqual(after["packages"]["language"], before["packages"]["language"])
        self.assertFalse(result["identity_changed"])
        # The bumped lock prepares through the Release transport.
        self.assertEqual(build.prepare(self.lock_path, self.out, self.cache, self.shared,
                                       fetcher=self.owner_inputs())["runtime_identity"], before["runtime_identity"])

    def test_cache_is_reused_on_a_second_bump(self):
        self.publish_game()
        original = self.lock_path.read_bytes()
        self.run_bump()
        first = list(self.server.requests)
        self.lock_path.write_bytes(original)
        self.run_bump()
        second = self.server.requests[len(first):]
        self.assertEqual(len(first), 3)
        self.assertEqual([p.rsplit("/", 1)[1] for p in second], ["SHA256SUMS"])

    def test_new_game_bytes_change_snapshot_and_runtime_identity(self):
        self.contents["data/9.1/base/skills.json"] = b'[{"id":"raw","value":1}]'
        snapshot = bundle.decode(self.contents["data/9.1/manifest.json"])
        snapshot["files"]["base/skills.json"] = bundle.sha256(self.contents["data/9.1/base/skills.json"])
        self.contents["data/9.1/manifest.json"] = pack.raw_json(snapshot)
        # Only the Release carries the new archive; the lock file still pins the old one.
        self.set_part("game", {n: r for n, r in self.contents.items() if "/i18n/" not in n})
        self.publish_game(self.archives["game"])
        result = self.run_bump()
        after = bundle.decode(self.lock_path.read_bytes())
        self.assertTrue(result["identity_changed"])
        self.assertEqual(after["snapshot_manifest"], pack.pin(self.contents["data/9.1/manifest.json"]))
        self.assertNotEqual(after["runtime_identity"], result["before"]["runtime_identity"])

    def test_checksum_missing_receipt_dirty_and_missing_asset_leave_the_lock(self):
        original = self.lock_path.read_bytes()
        cases = {
            "checksum": lambda: self.server.publish("ackness/repoe", TAG,
                {ASSET: self.archives["game"], "receipt.json": receipt()},
                sums={ASSET: "0" * 64, "receipt.json": hashlib.sha256(receipt()).hexdigest()}),
            "receipt-unlisted": lambda: self.server.publish("ackness/repoe", TAG, {ASSET: self.archives["game"]}),
            "dirty": lambda: self.publish_game(dirty=True),
            "commit": lambda: self.publish_game(commit="HEAD"),
            "asset-missing": lambda: (self.publish_game() / ASSET).unlink(),
            "two-packages": lambda: self.server.publish("ackness/repoe", TAG,
                {ASSET: self.archives["game"], "other.zip": b"x", "receipt.json": receipt()}),
        }
        messages = {"checksum": "does not match SHA256SUMS", "receipt-unlisted": "does not list receipt.json",
                    "dirty": "clean producer", "commit": "producer commit", "asset-missing": "Cannot download",
                    "two-packages": "exactly one .zip"}
        for name, publish in cases.items():
            with self.subTest(name):
                for stale in (self.server.root / "ackness/repoe/releases/download" / TAG).glob("*"):
                    stale.unlink()
                for cached in self.cache.glob("*"):
                    cached.unlink()
                publish()
                with self.assertRaisesRegex(ValueError, messages[name]):
                    self.run_bump()
                self.assertEqual(self.lock_path.read_bytes(), original)

    def test_receipt_must_describe_the_package(self):
        original = self.lock_path.read_bytes()
        game = {n: r for n, r in self.contents.items() if "/i18n/" not in n}
        good = receipt()
        cases = {
            "target": ({"target": {**TARGET, "realm": "china"}}, None, "target differs"),
            "contract": ({"contract": {"version": build.contract_version() + 1}}, None, "Snapshot declares contract"),
            "dirty-missing": ({"drop": ("dirty",)}, None, "clean producer"),
            "repository-missing": ({"drop": ("repository",)}, None, "producer repository"),
            "release-build-pin": ({}, {"receipt": {"path": "receipt.json", **pack.pin(b"other")}}, "another receipt"),
            "release-build-commit": ({}, {"receipt": {"path": "receipt.json", **pack.pin(good)},
                                          "producer": {"commit": "c" * 40}}, "another producer commit"),
        }
        for name, (fields, built, message) in cases.items():
            with self.subTest(name):
                self.set_part("game", game, **({"release_build": built} if built else {}))
                self.server.publish("ackness/repoe", TAG, {ASSET: self.archives["game"],
                                                           "receipt.json": receipt(**fields)})
                with self.assertRaisesRegex(ValueError, message):
                    self.run_bump()
                self.assertEqual(self.lock_path.read_bytes(), original)
        # A fresh root manifest that pins this very receipt is accepted.
        self.set_part("game", game, release_build={"receipt": {"path": "receipt.json", **pack.pin(good)},
                                                   "producer": {"repository": "ackness/repoe", "commit": "b" * 40}})
        self.server.publish("ackness/repoe", TAG, {ASSET: self.archives["game"], "receipt.json": good})
        self.run_bump()
        self.assertEqual(bundle.decode(self.lock_path.read_bytes())["packages"]["game"]["receipt"], pack.pin(good))

    def test_receipt_package_pin_binds_the_zip_and_manifest(self):
        """A fresh manifest names no receipt; the receipt's own `package` pin binds the two."""
        original = self.lock_path.read_bytes()
        game = {n: r for n, r in self.contents.items() if "/i18n/" not in n}
        self.set_part("game", game)
        zip_pin, manifest_pin = pack.pin(self.archives["game"]), self.lock["packages"]["game"]["manifest"]
        repoe = {"path": "game-build-part.zip", **zip_pin, "manifest": {"class": "fresh-manifest", **manifest_pin}}
        cases = {
            "zip": ({**repoe, **pack.pin(b"other zip")}, "another package zip"),
            "manifest": ({**repoe, "manifest": pack.pin(b"other manifest")}, "another package manifest"),
            "missing-manifest": ({k: v for k, v in repoe.items() if k != "manifest"}, "another package manifest"),
            "expanded": ({**repoe, "expanded_bytes": 1}, "another expanded package size"),
            "asset": ({**repoe, "asset": "pobr-game-9.0.zip"}, "another package asset"),
            "malformed": ("pobr-game-9.1.zip", "malformed"),
        }
        v2 = "repoe-release-receipt/v2"
        cases["v2-without-package"] = (None, "'repoe-release-receipt/v2'\\) lacks its package pin")
        cases["v2-mismatch"] = ({**repoe, **pack.pin(b"other zip")}, "another package zip")
        for name, (package, message) in cases.items():
            with self.subTest(name):
                fields = {"schema": v2} if name.startswith("v2") else {}
                if package is not None:
                    fields["extra"] = {"package": package}
                self.server.publish("ackness/repoe", TAG, {ASSET: self.archives["game"],
                                                           "receipt.json": receipt(**fields)})
                with self.assertRaisesRegex(ValueError, message):
                    self.run_bump()
                self.assertEqual(self.lock_path.read_bytes(), original)
        # RePoE's shape (the build-directory zip name, a classed manifest pin) is accepted.
        good = receipt(extra={"package": repoe}, schema=v2)
        self.server.publish("ackness/repoe", TAG, {ASSET: self.archives["game"], "receipt.json": good})
        self.run_bump()
        self.assertEqual(bundle.decode(self.lock_path.read_bytes())["packages"]["game"]["receipt"], pack.pin(good))
        # The language owner's shape (Release asset name, expanded size) too.
        language = {n: r for n, r in self.contents.items() if "/i18n/" in n}
        self.set_part("language", language)
        spec = self.lock["packages"]["language"]
        def publish_locale(raw):
            self.server.publish("ackness/poe2-zh-cn-stats", "locale/9.1-2", {
                "pobr-locale-9.1.zip": self.archives["language"], "receipt.json": raw})
        # Only RePoE's v1 receipt may omit the package pin: not the locale schema, a missing or an unknown one.
        for name, raw in {"locale": locale_receipt(spec, self.archives["language"], package=False),
                          "missing": receipt(repository="ackness/poe2-zh-cn-stats", schema=None),
                          "unknown": receipt(repository="ackness/poe2-zh-cn-stats", schema="other/v1")}.items():
            with self.subTest(name):
                publish_locale(raw)
                with self.assertRaisesRegex(ValueError, "lacks its package pin"):
                    self.run_bump([("ackness/poe2-zh-cn-stats", "locale/9.1-2")])
        publish_locale(locale_receipt(spec, self.archives["language"]))
        self.run_bump([("ackness/poe2-zh-cn-stats", "locale/9.1-2")])
        after = bundle.decode(self.lock_path.read_bytes())["packages"]["language"]
        self.assertEqual((after["tag"], after["contract_version"]), ("locale/9.1-2", None))

    def test_failed_prepare_restores_the_previous_lock_and_current(self):
        self.publish_game()
        data = self.root / "source/data"
        before = {path: path.read_bytes() for path in (self.lock_path, data / "CURRENT")}
        def install():
            self.assertNotEqual(self.lock_path.read_bytes(), before[self.lock_path])
            raise RuntimeError("prepare failed")
        with self.assertRaisesRegex(RuntimeError, "prepare failed"):
            bump.adopt(self.lock_path, [("ackness/repoe", TAG)], self.cache, data,
                       fetcher=self.owner_inputs(), install=install)
        self.assertEqual({path: path.read_bytes() for path in before}, before)
        self.assertEqual(list(self.lock_path.parent.glob(f".{self.lock_path.name}.*")), [])

    def publish_new_patch(self, version="9.2", catalog=b'{"gems":[]}', shown=True, drift=None, extra=None):
        target = {**TARGET, "game_patch": version}
        contents = {n.replace("data/9.1/", f"data/{version}/"): r for n, r in self.contents.items()}
        contents[f"data/{version}/overlay/trade_catalog.json"] = catalog
        base = bundle.decode(self.contents["data/9.1/manifest.json"])
        snapshot = {**base, "poe_version": version, "domains": {**base["domains"], "overlay": ["trade_catalog"]},
                    "files": {**base["files"], "overlay/trade_catalog.json": bundle.sha256(catalog)}}
        contents[f"data/{version}/manifest.json"] = pack.raw_json(snapshot)
        for kind, owner, tag, asset in (("game", "ackness/repoe", f"game/{version}-1", f"pobr-game-{version}.zip"),
                                        ("language", "ackness/poe2-zh-cn-stats", f"locale/{version}-1",
                                         f"pobr-locale-{version}.zip")):
            self.set_part(kind, {n: r for n, r in contents.items() if ("/i18n/" in n) == (kind == "language")},
                          target=target)
            assets = {asset: self.archives[kind]}
            if kind == "language" and shown:
                assets.update(support_display(target, catalog, drift))
            if kind == "language":
                assets["receipt.json"] = locale_receipt({**self.lock["packages"][kind], "asset": asset},
                                                        self.archives[kind], target,
                                                        extra=extra(assets) if extra else None)
            else:
                assets["receipt.json"] = receipt(target=target)
            self.server.publish(owner, tag, assets)
        return target

    NEW_PATCH = [("ackness/repoe", "game/9.2-1"), ("ackness/poe2-zh-cn-stats", "locale/9.2-1")]

    def test_new_patch_needs_every_owner_and_rewrites_current(self):
        target = self.publish_new_patch()
        data = self.root / "source/data"
        original = self.lock_path.read_bytes()
        before = bundle.decode(original)
        with self.assertRaisesRegex(ValueError, "needs every owner bumped"):
            bump.adopt(self.lock_path, [("ackness/repoe", "game/9.2-1")], self.cache, data,
                       fetcher=self.owner_inputs())
        self.assertEqual(self.lock_path.read_bytes(), original)
        self.assertEqual((data / "CURRENT").read_bytes(), b"9.1\n")
        result = bump.adopt(self.lock_path, self.NEW_PATCH, self.cache, data, fetcher=self.owner_inputs())
        after = bundle.decode(self.lock_path.read_bytes())
        self.assertEqual(after["target"], target)
        self.assertEqual({spec["tag"] for spec in after["packages"].values()}, {"game/9.2-1", "locale/9.2-1"})
        self.assertEqual((data / "CURRENT").read_bytes(), b"9.2\n")
        self.assertTrue(result["identity_changed"])
        self.assertEqual(build.prepare(self.lock_path, self.out, self.cache, self.shared,
                                       fetcher=self.owner_inputs())["runtime_identity"], after["runtime_identity"])
        # The support display moved to the language Release; it imports from there.
        assets = support_display(target, b'{"gems":[]}')
        self.assertEqual(after["support_display"], {
            "repository": "ackness/poe2-zh-cn-stats", "tag": "locale/9.2-1",
            "manifest": pack.pin(assets[display.MANIFEST]), "asset": pack.pin(assets[display.ASSET]),
            "catalog_sha256": bundle.sha256(b'{"gems":[]}'), "source_data_version": "9.2"})
        self.assertEqual(result["after"]["support_display"]["tag"], "locale/9.2-1")
        self.assertEqual(display.prepare(self.lock_path, self.out, self.cache, fetcher=self.owner_inputs()),
                         assets[display.ASSET])
        # The patch-independent fixtures stay, recording the target they were published for.
        self.assertEqual(after["test_fixtures"], {**before["test_fixtures"], "target": TARGET})

    def test_new_patch_without_a_matching_support_display_leaves_the_lock(self):
        original = self.lock_path.read_bytes()
        cases = {
            "unlisted": ({"shown": False}, "needs a new support display"),
            "half-listed": ({"extra": None, "drift": None, "shown": True}, "without"),
            "catalog": ({"drift": lambda m, a: (m.update(catalog_sha256="e" * 64),
                                                a["_meta"].update(catalog_sha256="e" * 64))},
                        "bound to trade catalog"),
            "target": ({"drift": lambda m, a: m["target"].update(version="9.1")}, "target/schema"),
            "provenance": ({"drift": lambda m, a: a["_meta"].update(data_version="8.0")}, "provenance"),
            "receipt": ({"extra": lambda assets: {"support_display": {
                "manifest": pack.pin(b"other"), "asset": pack.pin(assets[display.ASSET])}}},
                        "pins another support display"),
            "receipt-without-assets": ({"shown": False, "extra": lambda assets: {"support_display": {
                "manifest": pack.pin(b"m"), "asset": pack.pin(b"a")}}}, "lists neither"),
        }
        for name, (options, message) in cases.items():
            with self.subTest(name):
                for cached in self.cache.glob("*"):
                    cached.unlink()
                self.publish_new_patch(**options)
                if name == "half-listed":
                    sums = self.server.root / "ackness/poe2-zh-cn-stats/releases/download/locale/9.2-1/SHA256SUMS"
                    sums.write_text("".join(line + "\n" for line in sums.read_text().splitlines()
                                            if not line.endswith(display.ASSET)))
                with self.assertRaisesRegex(ValueError, message):
                    self.run_bump(self.NEW_PATCH)
                self.assertEqual(self.lock_path.read_bytes(), original)

    def test_a_receipt_pinning_the_display_is_accepted(self):
        self.publish_new_patch(extra=lambda assets: {"support_display": {
            "manifest": {"asset": display.MANIFEST, **pack.pin(assets[display.MANIFEST])},
            "asset": {"asset": display.ASSET, **pack.pin(assets[display.ASSET])}}})
        self.run_bump(self.NEW_PATCH)
        self.assertEqual(bundle.decode(self.lock_path.read_bytes())["support_display"]["tag"], "locale/9.2-1")

    def test_a_kept_git_support_display_must_match_a_changed_catalog(self):
        original = self.lock_path.read_bytes()
        self.contents["data/9.1/overlay/trade_catalog.json"] = b'{"gems":[{"skill_id":"new"}]}'
        snapshot = bundle.decode(self.contents["data/9.1/manifest.json"])
        snapshot["files"]["overlay/trade_catalog.json"] = bundle.sha256(self.contents["data/9.1/overlay/trade_catalog.json"])
        self.contents["data/9.1/manifest.json"] = pack.raw_json(snapshot)
        self.set_part("game", {n: r for n, r in self.contents.items() if "/i18n/" not in n})
        self.publish_game(self.archives["game"])
        with self.assertRaisesRegex(ValueError, "bound to trade catalog"):
            self.run_bump()
        self.assertEqual(self.lock_path.read_bytes(), original)

    def test_a_kept_support_display_must_match_the_new_catalog(self):
        """A game re-release that changes the trade catalog needs a new support display."""
        self.publish_new_patch()
        self.run_bump(self.NEW_PATCH)
        adopted = self.lock_path.read_bytes()
        self.lock = bundle.decode(adopted)
        # Same target, new catalog, language untouched: the kept display no longer binds.
        self.publish_new_patch(catalog=b'{"gems":[{"skill_id":"new"}]}', shown=False)
        with self.assertRaisesRegex(ValueError, "bound to trade catalog .* publish a support display"):
            self.run_bump([("ackness/repoe", "game/9.2-1")])
        self.assertEqual(self.lock_path.read_bytes(), adopted)
        # The same catalog keeps it.
        self.publish_new_patch(shown=False)
        self.run_bump([("ackness/repoe", "game/9.2-1")])
        self.assertEqual(bundle.decode(self.lock_path.read_bytes())["support_display"],
                         bundle.decode(adopted)["support_display"])

    def test_wrong_part_kind_and_unknown_owner_reject(self):
        self.server.publish("ackness/repoe", TAG, {ASSET: self.archives["language"], "receipt.json": receipt()})
        with self.assertRaisesRegex(ValueError, "not a game part"):
            self.run_bump()
        with self.assertRaisesRegex(ValueError, "Unknown data owner"):
            bump.owner_name("ackness/pobr")
        self.assertEqual(bump.owner_name("repoe"), "ackness/repoe")

    def test_a_snapshot_without_a_contract_is_rejected(self):
        snapshot = bundle.decode(self.contents["data/9.1/manifest.json"])
        del snapshot["contract_version"]
        self.contents["data/9.1/manifest.json"] = pack.raw_json(snapshot)
        self.lock["snapshot_manifest"] = pack.pin(self.contents["data/9.1/manifest.json"])
        self.set_part("game", {n: r for n, r in self.contents.items() if "/i18n/" not in n})
        self.save()
        self.publish_game()
        original = self.lock_path.read_bytes()
        with self.assertRaisesRegex(ValueError, "declares no contract_version.*RELEASING.md"):
            self.run_bump()
        self.assertEqual(self.lock_path.read_bytes(), original)


# BuildDataTests' own cases already run in test_build_data.
for _name in [n for n in vars(BuildDataTests) if n.startswith("test_")]:
    if _name not in vars(DataBumpTests):
        setattr(DataBumpTests, _name, None)
del BuildDataTests


if __name__ == "__main__":
    unittest.main()
