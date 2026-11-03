"""Small synthetic tests for split-owner, fail-closed build data preparation."""

import contextlib
import copy
import io
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

from pipeline.runtime import build_data as build
from pipeline.runtime import data_bundle as bundle
from pipeline.runtime import data_snapshot
from pipeline.runtime import runtime_pack as pack
from pipeline.imports import data_imports
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
        self.publication = {"index_revision": "c" * 40, "sequence": 1,
                            "descriptor": {"revision": "d" * 40, "path": "publication/releases/test/1.json",
                                           "bytes": 1, "sha256": "e" * 64}}
        self.lock = {"schema": build.DATA_LOCK_SCHEMA, "generation": 1, "previous_id": None,
                     "target": self.target, "locale": "zh-CN", "release_id": None,
                     "snapshot_manifest": pack.pin(self.contents["data/9.1/manifest.json"]),
                     "runtime_identity": identity, "packages": {},
                     "owner_publications": {o: dict(self.publication) for o in build.OWNERS.values()},
                     "support_display": {"tag": "handoff/test-display"},
                     "test_fixtures": {"tag": "handoff/test-fixtures"},
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
        spec = {**pack.pin(archive), "manifest": pack.pin(manifest), "tag": "handoff/test-data",
                "expanded_bytes": len(manifest) + sum(map(len, files.values())),
                "path": f"data/9.1/{kind}.zip", "publication": "published",
                "repository": build.OWNERS[kind], "revision": "a" * 40}
        self.lock["packages"][kind] = spec
        self.archives[kind] = archive

    def save(self):
        self.lock_path.write_bytes(pack.raw_json(self.lock))

    def fetch(self, address):
        url, revision, name = address
        kind = next(k for k, s in self.lock["packages"].items() if s["path"] == name)
        self.assertEqual(url, f"https://github.com/{build.OWNERS[kind]}.git")
        self.assertEqual(revision, "a" * 40)
        return self.archives[kind]

    def prepare(self, **kwargs):
        return build.prepare(self.lock_path, self.out, self.cache, self.shared, fetcher=self.fetch, **kwargs)

    def installed(self, destination="installed/data"):
        data = self.root / destination
        data.mkdir(parents=True)
        (data / "CURRENT").write_bytes((self.version + "\n").encode())
        shutil.copytree(self.shared, data / "overlay-common")
        build.install(self.out, data)
        return data

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

    def test_install_preserves_excluded_local_patch_and_maintenance_only(self):
        self.prepare()
        data = self.root / "source/data"
        local = {"patch/base/skills.json": b'[{"local":"patch"}]',
                 "generated/parsed_mods.json": b'{"local":"derived cache"}'}
        for name, raw in local.items():
            path = data / self.version / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
        build.install(self.out, data)
        for name, raw in local.items():
            self.assertEqual((data / self.version / name).read_bytes(), raw)
        extra = data / self.version / "unlisted.json"
        extra.write_bytes(b"{}")
        with self.assertRaisesRegex(ValueError, "edited, corrupt or has unlisted"):
            build.install(self.out, data)
        self.assertEqual(extra.read_bytes(), b"{}")

    def test_declared_cache_named_runtime_file_remains_pinned(self):
        self.prepare()
        name = 'generated/parsed_mods.json'
        raw = b'{"declared":"runtime bytes"}'
        path = self.out / 'data' / self.version / name
        path.parent.mkdir(parents=True)
        path.write_bytes(raw)
        manifest = self.out / 'data' / self.version / 'manifest.json'
        document = json.loads(manifest.read_bytes())
        document['files'][name] = bundle.sha256(raw)
        manifest.write_bytes(pack.raw_json(document))
        files, identity = bundle.snapshot_inputs(self.out, self.version)
        (self.out / 'bundle.json').write_bytes(pack.raw_json(bundle.document(self.version, files, identity)))
        data = self.root / 'installed/data'
        data.mkdir(parents=True)
        (data / 'CURRENT').write_bytes((self.version + '\n').encode())
        shutil.copytree(self.shared, data / 'overlay-common')
        build.install(self.out, data)
        build.install(self.out, data)
        (data / self.version / name).write_bytes(b'{"corrupt":true}')
        with self.assertRaisesRegex(ValueError, 'edited, corrupt or has unlisted'):
            build.install(self.out, data)

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
            "repository": None, "revision": None, "tag": "handoff/test-locale",
            "path": "handoff/locale/manifest.json", "manifest": pack.pin(raw)}
        self.save()
        result = self.prepare(local_parts={"language": locale})
        self.assertFalse((self.out / "evidence").exists())
        (locale / "evidence/boundary.json").write_bytes(b"bad")
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            build.prepare(self.lock_path, self.root / "bad-evidence", self.cache, self.shared,
                          local_parts={"language": locale}, fetcher=self.fetch)
        self.assertEqual(result["runtime_identity"], self.lock["runtime_identity"])

    def test_intact_stale_same_version_install_is_replaced_keeping_local_layers(self):
        self.prepare()
        data = self.installed()
        local = {"patch/base/skills.json": b'[{"local":"patch"}]',
                 "generated/parsed_mods.json": b'{"local":"maintenance"}'}
        for name, raw in local.items():
            path = data / self.version / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
        target = data / self.version / "base/skills.json"
        target.write_bytes(b'[{"id":"stale"}]')
        data_snapshot.manifest(data / self.version)
        build.install(self.out, data, {"own.json"})
        self.assertEqual(target.read_bytes(), self.contents["data/9.1/base/skills.json"])
        for name, raw in local.items():
            self.assertEqual((data / self.version / name).read_bytes(), raw)
        self.assertEqual((data / "CURRENT").read_bytes(), (self.version + "\n").encode())

    def test_edited_or_unlisted_installed_files_reject_byte_identical(self):
        self.prepare()
        data = self.installed()
        target = data / self.version / "base/skills.json"
        target.write_bytes(b'[{"id":"edited"}]')
        with self.assertRaisesRegex(ValueError, "edited, corrupt or has unlisted"):
            build.install(self.out, data)
        self.assertEqual(target.read_bytes(), b'[{"id":"edited"}]')
        extra = data / self.version / "base/extra.json"
        extra.write_bytes(b"{}")
        with self.assertRaisesRegex(ValueError, "edited, corrupt or has unlisted"):
            build.install(self.out, data)
        self.assertEqual(extra.read_bytes(), b"{}")

    def external_shared(self):
        """Add an owner-published shared file to the game part and re-pin the lock."""
        external = b'{"retained":"external fact"}'
        game = {n: r for n, r in self.contents.items() if "/i18n/" not in n}
        self.set_part("game", {**game, "data/overlay-common/external.json": external})
        hashes = {name.removeprefix("data/9.1/"): bundle.sha256(raw)
                  for name, raw in self.contents.items() if not name.endswith("manifest.json")}
        hashes.update({"overlay-common/own.json": self.lock["shared_files"]["own.json"]["sha256"],
                       "overlay-common/external.json": bundle.sha256(external)})
        self.lock["runtime_identity"] = pack.identity("9.1", hashes)
        self.save()
        return external

    def test_differing_external_shared_refreshes_while_owned_or_unlisted_rejects(self):
        external = self.external_shared()
        self.prepare()
        data = self.installed()
        stale = data / "overlay-common/external.json"
        stale.write_bytes(b'{"stale":1}')
        build.install(self.out, data, {"own.json"})
        self.assertEqual(stale.read_bytes(), external)
        owned = data / "overlay-common/own.json"
        owned.write_bytes(b'{"rule":"edited"}')
        with self.assertRaisesRegex(ValueError, "Install shared rules differ"):
            build.install(self.out, data, {"own.json"})
        self.assertEqual(owned.read_bytes(), b'{"rule":"edited"}')
        owned.write_bytes((self.shared / "own.json").read_bytes())
        (data / "overlay-common/stray.json").write_bytes(b"{}")
        with self.assertRaisesRegex(ValueError, "unlisted shared rules"):
            build.install(self.out, data, {"own.json"})

    def test_rejected_install_leaves_shared_and_version_files_untouched(self):
        self.external_shared()
        self.prepare()
        data = self.installed()
        stale_shared = data / "overlay-common/external.json"
        stale_shared.write_bytes(b'{"stale":1}')
        edited = data / self.version / "base/skills.json"
        edited.write_bytes(b'[{"id":"edited"}]')
        with self.assertRaisesRegex(ValueError, "edited, corrupt or has unlisted"):
            build.install(self.out, data, {"own.json"})
        self.assertEqual(stale_shared.read_bytes(), b'{"stale":1}')
        self.assertEqual(edited.read_bytes(), b'[{"id":"edited"}]')

    def test_missing_listed_runtime_file_rejects_without_changes(self):
        self.prepare()
        data = self.installed()
        victim = data / self.version / "i18n/zh-TW/skills.json"
        victim.unlink()
        other = data / self.version / "base/skills.json"
        before = other.read_bytes()
        with self.assertRaisesRegex(ValueError, "edited, corrupt or has unlisted"):
            build.install(self.out, data)
        self.assertFalse(victim.exists())
        self.assertEqual(other.read_bytes(), before)

    def test_replace_reports_the_pre_install_runtime_identity(self):
        external = self.external_shared()
        self.prepare()
        data = self.installed()
        stale_shared = data / "overlay-common/external.json"
        stale_shared.write_bytes(b'{"stale":1}')
        target = data / self.version / "base/skills.json"
        target.write_bytes(b'[{"id":"stale"}]')
        data_snapshot.manifest(data / self.version)
        expected_old = bundle.data_inputs(data, self.version)[1]
        stream = io.StringIO()
        with contextlib.redirect_stderr(stream):
            build.install(self.out, data, {"own.json"})
        self.assertIn(f"replaced installed {self.version} runtime {expected_old[:12]} → ",
                      stream.getvalue())
        self.assertEqual(stale_shared.read_bytes(), external)
        self.assertEqual(target.read_bytes(), self.contents["data/9.1/base/skills.json"])

    def test_download_cache_honors_the_environment_override(self):
        with patch.dict(os.environ, {"POBR_DATA_CACHE": "~/pobr-shared-cache"}):
            self.assertEqual(data_imports.download_cache(), Path("~/pobr-shared-cache").expanduser())
        for value in (None, ""):
            with self.subTest(value=value), patch.dict(os.environ) as env:
                if value is None:
                    env.pop("POBR_DATA_CACHE", None)
                else:
                    env["POBR_DATA_CACHE"] = value
                self.assertEqual(data_imports.download_cache(),
                                 data_imports.ROOT / ".cache/data-import-downloads")

    def data_lock(self):
        """The owner's game part carries the crafting catalog its snapshot declares."""
        self.add_catalog()

    def add_catalog(self, in_game_part=True):
        """The owner's game part carries the crafting catalog its snapshot declares."""
        raw = b'{"mods":[{"source":"alloy","id":"AlloyHybrid"}]}\n'
        name = "data/9.1/overlay/trade_crafting_sources.json"
        snapshot = bundle.decode(self.contents["data/9.1/manifest.json"])
        snapshot["domains"]["overlay"] = ["trade_crafting_sources"]
        snapshot["files"]["overlay/trade_crafting_sources.json"] = bundle.sha256(raw)
        self.contents["data/9.1/manifest.json"] = pack.raw_json(snapshot)
        hashes = snapshot["files"] | {"overlay-common/own.json": self.lock["shared_files"]["own.json"]["sha256"]}
        self.lock.update(runtime_identity=pack.identity("9.1", hashes),
                         snapshot_manifest=pack.pin(self.contents["data/9.1/manifest.json"]))
        game = {n: r for n, r in self.contents.items() if "/i18n/" not in n}
        if in_game_part:
            game[name] = raw
        self.set_part("game", game)
        self.save()
        return raw

    def test_data_lock_prepares_the_same_identity_and_requires_owner_tags(self):
        self.data_lock()
        result = self.prepare()
        self.assertEqual(result["runtime_identity"], self.lock["runtime_identity"])
        raw = (self.out / "data/9.1/overlay/trade_crafting_sources.json").read_bytes()
        self.assertEqual(bundle.sha256(raw), bundle.decode(self.contents["data/9.1/manifest.json"])
                         ["files"]["overlay/trade_crafting_sources.json"])
        self.assertFalse((self.cache / "build-input-receipts").exists())
        for path, value in ((("packages", "game", "tag"), None), (("support_display", "tag"), "bad tag"),
                            (("generation",), 2), (("release_id",), "not-a-digest")):
            with self.subTest(path=path, value=value):
                changed = copy.deepcopy(self.lock)
                parent = changed
                for key in path[:-1]:
                    parent = parent[key]
                if value is None:
                    del parent[path[-1]]
                else:
                    parent[path[-1]] = value
                with self.assertRaises(ValueError):
                    build.validate(changed)
        broken = copy.deepcopy(self.lock)
        del broken["owner_publications"]["ackness/repoe"]
        with self.assertRaisesRegex(ValueError, "Both owner publications"):
            build.validate(broken)

    def test_crafting_facts_are_no_input_and_a_part_without_the_catalog_fails(self):
        self.data_lock()
        changed = copy.deepcopy(self.lock)
        changed["crafting"] = {"repository": "ackness/repoe"}
        with self.assertRaisesRegex(ValueError, "not a PoBR input"):
            build.validate(changed)
        with self.assertRaisesRegex(ValueError, "Unknown local data part"):
            self.prepare(local_parts={"crafting": self.root / "crafting.zip"})
        self.add_catalog(in_game_part=False)
        self.lock["packages"]["game"]["tag"] = "handoff/test-data"
        self.save()
        with self.assertRaisesRegex(ValueError, "inventory differs from snapshot"):
            self.prepare()
        self.assertFalse(self.out.exists())

    def test_install_requires_current_and_ownership_guard_rejects_forced_data(self):
        self.prepare()
        import shutil
        installed = self.root / "install/data"
        installed.mkdir(parents=True)
        (installed / "CURRENT").write_bytes(b"9.2\n")
        shutil.copytree(self.shared, installed / "overlay-common")
        with self.assertRaisesRegex(ValueError, "CURRENT mismatch"):
            build.install(self.out, installed)
        self.assertFalse((installed / "9.1").exists())
        from pipeline.imports.check_data_ownership import check
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
