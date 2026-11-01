"""Small synthetic tests for split-owner, fail-closed build data preparation."""

import copy
import json
from pathlib import Path
import tempfile
import unittest

import build_data as build
import data_bundle as bundle
import runtime_pack as pack


class BuildDataTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pobr split data ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.version = "9.1"
        self.target = {"game": "poe2", "game_patch": self.version, "realm": "international"}
        self.shared = self.root / "source/data/overlay-common"
        self.shared.mkdir(parents=True)
        (self.shared / "own.json").write_bytes(b'{"rule":"pobr"}')
        self.contents = {
            "data/9.1/base/skills.json": b'[{"id":"raw","value":9223372036854775807}]',
            "data/9.1/i18n/zh-CN/skills.json": '{"raw":"技能"}'.encode(),
            "data/9.1/i18n/zh-TW/skills.json": '{"raw":"技能TW"}'.encode(),
        }
        snapshot = {"schema_version": 3, "poe_version": self.version,
                    "domains": {"base": ["skills"]}, "languages": ["zh-CN", "zh-TW"],
                    "files": {k.removeprefix("data/9.1/"): bundle.sha256(v) for k, v in self.contents.items()}}
        self.contents["data/9.1/manifest.json"] = pack.raw_json(snapshot)
        for name, raw in {**self.contents, "data/CURRENT": b"9.1\n"}.items():
            path = self.root / "source" / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
        _, identity = bundle.snapshot_inputs(self.root / "source", self.version)
        self.lock = {"schema": build.LEGACY_LOCK_SCHEMA, "target": self.target,
                     "snapshot_manifest": pack.pin(self.contents["data/9.1/manifest.json"]),
                     "runtime_identity": identity, "packages": {},
                     "shared_files": {"own.json": pack.pin((self.shared / "own.json").read_bytes())}}
        self.archives = {}
        for kind in build.OWNERS:
            files = {n: r for n, r in self.contents.items()
                     if ("/i18n/" in n) == (kind == "language")}
            self.set_part(kind, files)
        self.lock_path = self.root / "build-data.lock.json"
        self.save()
        self.out, self.cache = self.root / "prepared", self.root / "cache"

    def set_part(self, kind, files, **changes):
        manifest = pack.raw_json({"schema": build.PART_SCHEMA, "kind": kind, "target": self.target,
                                  "files": {n: pack.pin(r) for n, r in files.items()}, **changes})
        archive = pack.archive_bytes({**files, "manifest.json": manifest})
        spec = {**pack.pin(archive), "manifest": pack.pin(manifest),
                "expanded_bytes": len(manifest) + sum(map(len, files.values())),
                "path": f"data/9.1/{kind}.zip", "publication": "published",
                "repository": build.OWNERS[kind], "revision": "a" * 40}
        self.lock["packages"][kind] = spec
        self.archives[kind] = archive

    def save(self):
        self.lock_path.write_bytes(pack.raw_json(self.lock))

    def fetch(self, address):
        url, revision, name = address
        if name == "crafting.zip":
            self.assertEqual(url, "https://github.com/ackness/repoe.git")
            self.assertEqual(revision, "b" * 40)
            return self.crafting_archive
        kind = next(k for k, s in self.lock["packages"].items() if s["path"] == name)
        self.assertEqual(url, f"https://github.com/{build.OWNERS[kind]}.git")
        self.assertEqual(revision, "a" * 40)
        return self.archives[kind]

    def prepare(self, **kwargs):
        return build.prepare(self.lock_path, self.out, self.cache, self.shared, fetcher=self.fetch, **kwargs)

    def test_exact_bundle_cache_replay_and_install(self):
        first = self.prepare()
        self.assertEqual(first, bundle.verify(self.out))
        self.assertEqual((self.out / "data/9.1/base/skills.json").read_bytes(),
                         self.contents["data/9.1/base/skills.json"])
        other = self.root / "replay"
        self.assertEqual(first, build.prepare(self.lock_path, other, self.cache, self.shared, offline=True,
                                              fetcher=lambda _: self.fail("offline fetched")))
        install = self.root / "installed/data"
        install.mkdir(parents=True)
        (install / "CURRENT").write_bytes(b"9.1\n")
        import shutil
        shutil.copytree(self.shared, install / "overlay-common")
        build.install(self.out, install)
        build.install(self.out, install)
        (install / "9.1/base/skills.json").write_bytes(b"[]")
        with self.assertRaises(ValueError):
            build.install(self.out, install)
        self.assertEqual((install / "9.1/base/skills.json").read_bytes(), b"[]")

    def test_missing_offline_and_corrupt_pin_never_use_existing_source_snapshot(self):
        with self.assertRaisesRegex(ValueError, "Offline cache miss"):
            self.prepare(offline=True)
        self.assertFalse(self.out.exists())
        self.archives["game"] += b"corruption"
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            self.prepare()
        self.assertFalse(self.out.exists())
        self.assertFalse(any(self.root.glob(".build-data-*")))

    def test_cache_corruption_rejects_without_network_repair(self):
        self.prepare()
        (self.cache / self.lock["packages"]["game"]["sha256"]).write_bytes(b"bad")
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            build.prepare(self.lock_path, self.root / "bad", self.cache, self.shared,
                          fetcher=lambda _: self.fail("must not fetch over corrupt cache"))

    def test_pending_requires_explicit_exact_local_parts(self):
        paths = {}
        for kind, spec in self.lock["packages"].items():
            spec.update(publication="pending", repository=None, revision=None)
            paths[kind] = self.root / f"{kind}.zip"
            paths[kind].write_bytes(self.archives[kind])
        self.save()
        with self.assertRaisesRegex(ValueError, "Unpublished"):
            self.prepare()
        self.assertFalse(self.out.exists())
        self.assertEqual(self.prepare(local_parts=paths)["runtime_identity"], self.lock["runtime_identity"])

    def test_cross_owner_mixed_target_unlisted_and_identity_mismatch_fail(self):
        original, archives = copy.deepcopy(self.lock), self.archives.copy()
        for case in ("cross-owner", "target", "inventory", "runtime", "manifest", "pobr"):
            with self.subTest(case=case):
                self.lock, self.archives = copy.deepcopy(original), archives.copy()
                if case == "cross-owner":
                    self.set_part("game", self.contents)
                elif case == "target":
                    files = {n: r for n, r in self.contents.items() if "/i18n/" in n}
                    self.set_part("language", files, target={**self.target, "realm": "china"})
                elif case == "inventory":
                    self.set_part("language", {"data/9.1/i18n/zh-CN/skills.json": b"{}"})
                elif case == "runtime":
                    self.lock["runtime_identity"] = "0" * 64
                elif case == "manifest":
                    self.lock["snapshot_manifest"]["sha256"] = "0" * 64
                else:
                    self.lock["packages"]["game"]["repository"] = "ackness/pobr"
                self.save()
                with self.assertRaises(ValueError):
                    self.prepare()
                self.assertFalse(self.out.exists())

    def test_external_shared_subset_and_local_rule_overlap(self):
        own = (self.shared / "own.json").read_bytes()
        self.lock["shared_files"] = {"own.json": pack.pin(own)}
        external = b'{"retained":"external fact"}'
        game = {n: r for n, r in self.contents.items() if "/i18n/" not in n}
        self.set_part("game", {**game, "data/overlay-common/external.json": external})
        hashes = {name.removeprefix("data/9.1/"): bundle.sha256(raw)
                  for name, raw in self.contents.items() if not name.endswith("manifest.json")}
        hashes.update({"overlay-common/own.json": bundle.sha256(own),
                       "overlay-common/external.json": bundle.sha256(external)})
        self.lock["runtime_identity"] = pack.identity("9.1", hashes)
        self.save()
        self.prepare()
        self.assertEqual((self.out / "data/overlay-common/external.json").read_bytes(), external)
        self.lock["shared_files"]["external.json"] = pack.pin(external)
        self.save()
        with self.assertRaisesRegex(ValueError, "Duplicate external/local"):
            build.prepare(self.lock_path, self.root / "overlap", self.cache, self.shared, fetcher=self.fetch)

    def test_native_language_git_tree_verifies_evidence_and_projects_only_i18n(self):
        locale = self.root / "locale"
        members = {n.removeprefix("data/9.1/"): raw for n, raw in self.contents.items() if "/i18n/" in n}
        members["evidence/boundary.json"] = b'{"observed_patch":null}'
        raw = pack.raw_json({"schema": "poe2-retained-locale-publication/v1", "target": self.target,
                             "files": {n: pack.pin(r) for n, r in members.items()}})
        for name, value in {**members, "manifest.json": raw}.items():
            p = locale / name; p.parent.mkdir(parents=True, exist_ok=True); p.write_bytes(value)
        self.lock["packages"]["language"] = {"format": "locale-git-files", "publication": "pending",
            "repository": None, "revision": None, "path": "handoff/locale/manifest.json", "manifest": pack.pin(raw)}
        self.save()
        result = self.prepare(local_parts={"language": locale})
        self.assertFalse((self.out / "evidence").exists())
        (locale / "evidence/boundary.json").write_bytes(b"bad")
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            build.prepare(self.lock_path, self.root / "bad-evidence", self.cache, self.shared,
                          local_parts={"language": locale}, fetcher=self.fetch)
        self.assertEqual(result["runtime_identity"], self.lock["runtime_identity"])

    def test_runtime_pack_uses_the_independent_build_lock(self):
        requirements = {"target": self.target, "translation": {"locale": "zh-CN"},
                        "runtime_pack": {"schema": pack.SCHEMA}}
        rp = self.root / "requirements.json"; rp.write_bytes(pack.raw_json(requirements))
        imports = self.root / "imports"
        display = pack.raw_json({"schema": "pobr-native-display-texts/v1", "game_patch": self.version,
                                "realm": "international", "locale": "zh-CN", "entries": {}})
        for name, raw in {"runtime/display_texts.json": display,
                          "runtime/receipt.json": pack.raw_json({"files": {"display_texts.json": pack.pin(display)}}),
                          "import-receipt.json": pack.raw_json({"requirements_sha256": bundle.sha256(rp.read_bytes()),
                              "target": self.target, "locale": "zh-CN"})}.items():
            p = imports / name; p.parent.mkdir(parents=True, exist_ok=True); p.write_bytes(raw)
        result = pack.import_pack(rp, self.lock_path, imports, self.root / "pack", self.cache,
                                  fetcher=self.fetch, shared_root=self.shared)
        release, _, _ = pack.read_release(self.root / "pack/player", result["release_id"])
        self.assertEqual(release["target"], self.target)

    def enable_crafting(self, include_old=False):
        import crafting_inputs
        import runpy
        helpers = runpy.run_path(str(Path(__file__).with_name("test-trade-crafting.py")))
        spec, self.crafting_archive = helpers["source_package"](helpers["fixture"](), self.target)
        projected, _ = crafting_inputs.project(helpers["fixture"](),
            source_manifest_sha256=spec["manifest"]["sha256"],
            facts_manifest_sha256=spec["input_manifest"]["sha256"], target=self.target)
        from data_imports import encoded
        raw = encoded(projected)
        name = "data/9.1/overlay/trade_crafting_sources.json"
        snapshot = bundle.decode(self.contents["data/9.1/manifest.json"])
        snapshot["domains"]["overlay"] = ["trade_crafting_sources"]
        snapshot["files"]["overlay/trade_crafting_sources.json"] = bundle.sha256(raw)
        self.contents["data/9.1/manifest.json"] = pack.raw_json(snapshot)
        hashes = snapshot["files"] | {"overlay-common/own.json": self.lock["shared_files"]["own.json"]["sha256"]}
        self.lock.update(schema=build.LOCK_SCHEMA, crafting=spec, runtime_identity=pack.identity("9.1", hashes),
                         snapshot_manifest=pack.pin(self.contents["data/9.1/manifest.json"]))
        game = {n: r for n, r in self.contents.items() if "/i18n/" not in n}
        if include_old:
            game[name] = raw  # Even an identical processed copy must not provide the projected member.
        self.set_part("game", game)
        self.save()
        return raw

    def test_v2_crafting_is_required_and_never_reads_processed_fallback(self):
        raw = self.enable_crafting()
        result = self.prepare()
        self.assertEqual((self.out / "data/9.1/overlay/trade_crafting_sources.json").read_bytes(), raw)
        receipts = list((self.cache / "build-input-receipts").glob("*.json"))
        self.assertEqual(len(receipts), 1)
        proof = bundle.decode(receipts[0].read_bytes())
        self.assertEqual(proof["summary"], {"selected_rows": 2, "catalog_mods": 3})
        self.assertFalse((self.out / "crafting.json").exists())
        self.assertEqual(bundle.verify(self.out), result)
        self.assertEqual(build.prepare(self.lock_path, self.root / "again", self.cache, self.shared, offline=True), result)
        self.enable_crafting(include_old=True)
        with self.assertRaisesRegex(ValueError, "contains PoBR-projected"):
            build.prepare(self.lock_path, self.root / "old-fallback", self.cache, self.shared, fetcher=self.fetch)
        self.assertFalse((self.root / "old-fallback").exists())

    def test_v2_missing_corrupt_or_foreign_crafting_cannot_publish_bundle(self):
        for case in ("missing", "offline", "corrupt", "foreign"):
            with self.subTest(case=case):
                self.enable_crafting()
                if case == "missing": del self.lock["crafting"]
                elif case == "corrupt": self.crafting_archive += b"corrupt"
                elif case == "foreign":
                    # A valid archive/pin bound to a different source target still rejects.
                    import runpy
                    helpers = runpy.run_path(str(Path(__file__).with_name("test-trade-crafting.py")))
                    self.lock["crafting"], self.crafting_archive = helpers["source_package"](
                        helpers["fixture"](), self.target | {"realm": "china"})
                self.save()
                with self.assertRaises((ValueError, KeyError)):
                    self.prepare(offline=case == "offline")
                self.assertFalse(self.out.exists())

    def test_rollback_install_keeps_active_current_and_ownership_guard_rejects_forced_data(self):
        self.prepare()
        import shutil
        installed = self.root / "install/data"
        installed.mkdir(parents=True)
        (installed / "CURRENT").write_bytes(b"9.2\n")
        shutil.copytree(self.shared, installed / "overlay-common")
        with self.assertRaisesRegex(ValueError, "CURRENT mismatch"):
            build.install(self.out, installed)
        build.install(self.out, installed, keep_current=True)
        build.install(self.out, installed, keep_current=True)
        self.assertEqual((installed / "CURRENT").read_bytes(), b"9.2\n")
        from check_data_ownership import check
        declared = {"shared_files": {"own.json": pack.pin((self.shared / "own.json").read_bytes())}}
        check(["data/CURRENT", "data/overlay-common/own.json"], declared)
        for name in ("data/9.1/base/skills.json", "data/9.1/i18n/zh-CN/skills.json", "data/overlay-common/facts.json"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "cannot be tracked"):
                check([name], declared)

    def test_symlinks_shared_rule_drift_and_fresh_output(self):
        (self.shared / "own.json").write_bytes(b'{"rule":"changed"}')
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            self.prepare()
        (self.shared / "own.json").unlink()
        (self.shared / "own.json").symlink_to(self.lock_path)
        with self.assertRaisesRegex(ValueError, "Symlink"):
            self.prepare()
        self.out.mkdir()
        with self.assertRaisesRegex(ValueError, "Fresh data output"):
            self.prepare()


if __name__ == "__main__":
    unittest.main()
