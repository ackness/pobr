"""Synthetic publication/frontier tests; no GitHub or game-data acquisition."""

import copy
import hashlib
import io
import os
from datetime import datetime, timedelta, timezone
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from pipeline.runtime import data_bundle as bundle
from pipeline.imports.data_imports import encoded, sha
from pipeline.latest import latest_data as latest
ROOT = Path(__file__).resolve().parents[2]


class LatestDataTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.source = self.root / "repository"
        for name in (latest.LOCK, latest.CHANNELS, "data/CURRENT"):
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((ROOT / name).read_bytes())
        self.lock = bundle.decode((self.source / latest.LOCK).read_bytes())
        self.channels = bundle.decode((self.source / latest.CHANNELS).read_bytes())
        self.payloads, self.frontier, self.descriptors, self.calls = {}, {}, {}, []
        self.head_calls = []
        # Owners publish more roles than PoBR consumes; these extras stay out of the lock.
        source = {"repository": "ackness/repoe", "revision": "9" * 40, "path": "handoff/display/source.zip",
                  "publication": "published", "bytes": 1, "sha256": "9" * 64,
                  "manifest": {"bytes": 1, "sha256": "6" * 64}}
        catalog = {"repository": "ackness/poe2-zh-cn-stats", "revision": "8" * 40,
                   "path": "handoff/catalog/manifest.json", "publication": "published",
                   "manifest": {"bytes": 1, "sha256": "8" * 64}}
        workbench = {"repository": "ackness/poe2-zh-cn-stats", "revision": "7" * 40,
                     "path": "src/translation-workbench.mjs", "publication": "published", "bytes": 1, "sha256": "7" * 64}
        extras = {"ackness/repoe": {"display_source": source},
                  "ackness/poe2-zh-cn-stats": {"display_catalog": catalog, "workbench": workbench}}
        for owner in latest.OWNERS:
            inputs = {**latest.declarations(self.lock, owner), **extras[owner]}
            publication = {"schema": latest.PUBLICATION_SCHEMA, "owner": owner,
                           "target": self.lock["target"], "sequence": 1, "previous": None,
                           "components": {role: {"sequence": 1, "input": value} for role, value in inputs.items()},
                           "scope": {"complete": True, "meaning": "Synthetic declaration only"}}
            if owner == "ackness/poe2-zh-cn-stats":
                validation = encoded({"format": "windows-fk-validation/v1", "manifest_sha256": catalog["manifest"]["sha256"],
                                      "independent_fk_recomputed": True, "boundaries": {"observed_patch": None,
                                      "observed_realm": None, "official_client_semantics_verified": False,
                                      "production_ready": False}})
                publication.update(scope={"complete": "declared-dependency-closure-only",
                                          "freshness_certified": False, "official_client_certified": False,
                                          "translation_quality_certified": False},
                                   bindings={"display_source": source},
                                   evidence={"catalog_validation": {"revision": catalog["revision"],
                                             "path": str(Path(catalog["path"]).parent / "validation.json"),
                                             **latest.pin(validation)}})
                self.payloads[latest.git_address({"repository": owner, **publication["evidence"]["catalog_validation"]},
                              publication["evidence"]["catalog_validation"]["path"])] = validation
            self.publish(owner, publication)
        self.lock = latest.bootstrap(self.source, self.fetch, self.head)
        (self.source / latest.LOCK).write_bytes(encoded(self.lock))
        self.owner_tags = {owner: {} for owner in latest.OWNERS}
        for owner, roles in latest.ROLES.items():
            for path in roles.values():
                spec = latest.lookup(self.lock, path)
                self.owner_tags[owner][spec["tag"]] = spec["revision"]
        self.before = self.snapshot()
        self.calls.clear()
        self.head_calls.clear()

    def tags(self, owner):
        return dict(self.owner_tags[owner])

    def snapshot(self):
        return {p.relative_to(self.source).as_posix(): p.read_bytes()
                for p in self.source.rglob("*") if p.is_file()}

    def publish(self, owner, document):
        raw = encoded(document)
        revision = hashlib.sha1(b"synthetic-publication:" + raw).hexdigest()
        path = "publication/releases/" + str(document["sequence"]) + ".json"
        spec = {"revision": revision, "path": path, **latest.pin(raw)}
        self.payloads[(f"https://github.com/{owner}.git", revision, path)] = raw
        index = {"schema": latest.INDEX_SCHEMA,
                 "channel": {key: document["target"][key] for key in ("game", "realm")},
                 "sequence": document["sequence"], "head": spec}
        index_raw = encoded(index)
        head = hashlib.sha1(b"synthetic-index:" + index_raw).hexdigest()
        self.payloads[(f"https://github.com/{owner}.git", head, "publication/index.json")] = index_raw
        self.frontier[owner] = head
        self.descriptors[owner] = (copy.deepcopy(document), spec)
        return spec

    def head(self, owner, branch):
        self.head_calls.append((owner, branch))
        return self.frontier[owner]

    def fetch(self, address):
        self.calls.append(address)
        return self.payloads[address]

    def next_publication(self, owner="ackness/poe2-zh-cn-stats", role="workbench", revision="f" * 40):
        document, previous = copy.deepcopy(self.descriptors[owner])
        document.update(sequence=document["sequence"] + 1, previous=previous)
        value = document["components"][role]
        value["sequence"] += 1
        value["input"]["revision"] = revision
        self.publish(owner, document)
        return document

    def test_current_complete_publications_are_frozen_once_without_mutation(self):
        status, _, _ = latest.discover(self.source, self.fetch, self.head)
        self.assertEqual(status["status"], "no-new-publication")
        self.assertEqual(len(self.head_calls), 2)
        self.assertTrue(status["metadata_auto_eligible"])
        self.assertFalse(status["auto_eligible"])
        self.assertEqual(self.snapshot(), self.before)
        self.assertEqual(latest.propose(self.source, self.root / "no-change", self.fetch, self.head)["status"],
                         "no-new-publication")
        self.assertFalse((self.root / "no-change").exists())

    def test_immutable_composition_verification_uses_recorded_heads_without_latest_discovery(self):
        self.next_publication()
        result = latest.verify_composition(self.source, self.fetch, self.tags)
        self.assertEqual(result["status"], "verified-immutable-declarations")
        self.assertEqual(result["tags"], "verified")
        self.assertEqual(result["freshness"], "not-checked")
        self.assertEqual(self.head_calls, [])
        self.assertEqual(self.snapshot(), self.before)
        moved = self.lock["test_fixtures"]["tag"]
        self.owner_tags["ackness/repoe"][moved] = "0" * 40
        with self.assertRaisesRegex(ValueError, "does not name its pinned revision"):
            latest.verify_composition(self.source, self.fetch, self.tags)
        del self.owner_tags["ackness/repoe"][moved]
        with self.assertRaisesRegex(ValueError, "does not name its pinned revision"):
            latest.verify_composition(self.source, self.fetch, self.tags)
        document = copy.deepcopy(self.lock)
        document["owner_publications"]["ackness/repoe"]["sequence"] = 999
        (self.source / latest.LOCK).write_bytes(encoded(document))
        with self.assertRaisesRegex(ValueError, "rollback"):
            latest.verify_composition(self.source, self.fetch, self.tags)

    def test_source_code_only_index_revision_does_not_make_a_data_update(self):
        owner = "ackness/repoe"
        before = self.frontier[owner]
        self.frontier[owner] = "d" * 40
        self.payloads[(f"https://github.com/{owner}.git", "d" * 40, "publication/index.json")] = self.payloads[
            (f"https://github.com/{owner}.git", before, "publication/index.json")]
        status, _, _ = latest.discover(self.source, self.fetch, self.head)
        self.assertEqual(status["status"], "no-new-publication")
        self.assertEqual(status["published_frontier"], self.lock["owner_publications"])

    def test_same_patch_module_change_creates_complete_candidate_and_keeps_current(self):
        self.next_publication()
        out = self.root / "candidate"
        result = latest.propose(self.source, out, self.fetch, self.head)
        self.assertEqual(result["status"], "update-available")
        receipt = latest.verify_candidate(out)
        lock, _ = latest.read_lock(out)
        self.assertEqual(lock["generation"], self.lock["generation"] + 1)
        self.assertEqual(lock["previous_id"], sha(self.before[latest.LOCK]))
        self.assertEqual(lock["owner_publications"]["ackness/poe2-zh-cn-stats"]["sequence"], 2)
        # An extra owner role advances the publication without touching consumed inputs.
        for key in ("packages", "support_display", "test_fixtures", "snapshot_manifest",
                    "runtime_identity", "release_id"):
            self.assertEqual(lock[key], self.lock[key], key)
        self.assertEqual(receipt["status"], "resolved-not-validated")
        self.assertEqual(receipt["adoption"], "not-adopted")
        self.assertEqual(self.snapshot(), self.before)

    def test_mutable_head_advance_during_fetch_does_not_mix_publications(self):
        self.next_publication()
        fetch = self.fetch
        def moving(address):
            raw = fetch(address)
            if address[2] == "publication/index.json":
                self.frontier["ackness/poe2-zh-cn-stats"] = "0" * 40
            return raw
        out = self.root / "frozen"
        latest.propose(self.source, out, moving, self.head)
        self.assertEqual(len(self.head_calls), 2)
        latest.verify_candidate(out)

    def test_missing_game_or_cross_owner_role_rejects_without_candidate(self):
        owner = "ackness/repoe"
        original = copy.deepcopy(self.descriptors[owner][0])
        for failure in ("missing", "wrong-owner"):
            document = copy.deepcopy(original)
            if failure == "missing":
                del document["components"]["game"]
            else:
                document["components"]["game"]["input"]["repository"] = "ackness/pobr"
            self.publish(owner, document)
            with self.subTest(failure=failure), self.assertRaises(ValueError):
                latest.propose(self.source, self.root / failure, self.fetch, self.head)
            self.assertEqual(self.snapshot(), self.before)

    def test_role_change_without_sequence_advance_rejects(self):
        document = self.next_publication(role="support_display")
        document["components"]["support_display"]["sequence"] -= 1
        self.publish(document["owner"], document)
        with self.assertRaisesRegex(ValueError, "role regressed"):
            latest.discover(self.source, self.fetch, self.head)

    def test_same_sequence_other_descriptor_is_a_fork(self):
        owner = "ackness/repoe"
        document, _ = copy.deepcopy(self.descriptors[owner])
        document["scope"]["meaning"] = "Different same-sequence publication"
        self.publish(owner, document)
        with self.assertRaisesRegex(ValueError, "history fork"):
            latest.discover(self.source, self.fetch, self.head)

    def test_changed_scope_requires_review_even_when_target_is_same(self):
        document = self.next_publication()
        document["scope"]["meaning"] = "New applicability decision"
        self.publish(document["owner"], document)
        status, _, _ = latest.discover(self.source, self.fetch, self.head)
        self.assertFalse(status["auto_eligible"])
        self.assertFalse(status["scope_preserved"])

    def zipped(self, members):
        handle = io.BytesIO()
        with zipfile.ZipFile(handle, "w") as archive:
            for name, raw in members.items():
                archive.writestr(name, raw)
        return handle.getvalue()

    def new_patch_publication(self, version="0.5.5.6"):
        """Publish a real consumable new-target candidate from both owners."""
        target = {**self.lock["target"], "game_patch": version}
        prefix = f"data/{version}/"
        content = {"base/skills.json": b'{"id":9223372036854775807}',
                   "overlay/trade_catalog.json": b'{"gems":[]}',
                   "i18n/zh-CN/skills.json": '{"raw":"技能"}'.encode(),
                   "i18n/zh-CN/display_texts.json": encoded(
                       {"schema": "pobr-native-display-texts/v1", "game_patch": version,
                        "realm": target["realm"], "locale": "zh-CN", "entries": {}, "scopes": {}})}
        snapshot = encoded({"schema_version": 3, "poe_version": version, "languages": ["zh-CN"],
                            "domains": {"base": ["skills"], "overlay": ["trade_catalog"]},
                            "files": {name: sha(raw) for name, raw in content.items()}})
        members = {prefix + "manifest.json": snapshot,
                   **{prefix + name: raw for name, raw in content.items() if not name.startswith("i18n/")}}
        manifest = encoded({"schema": latest.build_data.PART_SCHEMA, "kind": "game", "target": target,
                            "files": {name: latest.pin(raw) for name, raw in members.items()}})
        archive = self.zipped({**members, "manifest.json": manifest})
        game = {"repository": "ackness/repoe", "revision": "1" * 40, "path": "handoff/new/game.zip",
                "publication": "published", **latest.pin(archive), "manifest": latest.pin(manifest),
                "expanded_bytes": len(manifest) + sum(map(len, members.values()))}
        locale = encoded({"schema": "poe2-retained-locale-publication/v1", "target": target,
                          "files": {name: latest.pin(raw) for name, raw in content.items()
                                    if name.startswith("i18n/")}})
        language = {"repository": "ackness/poe2-zh-cn-stats", "revision": "2" * 40,
                    "path": "handoff/new/locale.json", "format": "locale-git-files",
                    "publication": "published", "manifest": latest.pin(locale)}
        catalog_sha = sha(members[prefix + "overlay/trade_catalog.json"])
        provenance = {"generation_data_version": "synthetic-9.9", "source": "synthetic",
                      "source_commit": "3" * 40, "source_file": "source.json", "source_sha256": "4" * 64}
        asset = encoded({"_meta": {"data_version": "synthetic-9.9", "catalog_sha256": catalog_sha,
                                   **{k: provenance[k] for k in
                                      ("source", "source_commit", "source_file", "source_sha256")}},
                         "entries": {}})
        display_manifest = encoded({"schema": "poe2-support-display-publication/v1",
                                    "target": {"game": target["game"], "version": version,
                                               "realm": target["realm"]},
                                    "locale": "zh-CN", "catalog_sha256": catalog_sha,
                                    "files": {"supportDescriptions.zh-CN.json": latest.pin(asset)},
                                    "provenance": provenance})
        support = {"repository": "ackness/poe2-zh-cn-stats", "revision": "5" * 40,
                   "path": "handoff/new/display/manifest.json", "publication": "published",
                   "manifest": latest.pin(display_manifest), "asset": latest.pin(asset),
                   "catalog_sha256": catalog_sha, "source_data_version": "synthetic-9.9"}
        fixture_files = {"api-sample.json": b'{"sample":1}'}
        fixture_manifest = encoded({"schema": "repoe-data-publication/v1", "target": target,
                                    "files": {name: latest.pin(raw) for name, raw in fixture_files.items()}})
        fixture_archive = self.zipped({**fixture_files, "manifest.json": fixture_manifest})
        fixtures = {"repository": "ackness/repoe", "revision": "6" * 40,
                    "path": "handoff/new/fixtures.zip", "publication": "published",
                    **latest.pin(fixture_archive), "manifest": latest.pin(fixture_manifest),
                    "expanded_bytes": len(fixture_manifest) + sum(map(len, fixture_files.values())),
                    "files": list(fixture_files)}
        for spec, raw in ((game, archive), (language, locale), (support, display_manifest),
                          (fixtures, fixture_archive)):
            self.payloads[latest.git_address(spec, spec["path"])] = raw
        for name, raw in content.items():
            if name.startswith("i18n/"):
                self.payloads[latest.git_address(language, f"handoff/new/{name}")] = raw
        self.payloads[latest.git_address(
            support, str(Path(support["path"]).parent / "supportDescriptions.zh-CN.json"))] = asset
        inputs = {"ackness/repoe": {"game": game, "test_fixtures": fixtures},
                  "ackness/poe2-zh-cn-stats": {"language": language, "support_display": support}}
        for owner in latest.OWNERS:
            document, previous = copy.deepcopy(self.descriptors[owner])
            document.update(sequence=document["sequence"] + 1, previous=previous, target=target)
            for role, value in inputs[owner].items():
                component = document["components"][role]
                document["components"][role] = {"sequence": component["sequence"] + 1, "input": value}
            self.publish(owner, document)
        self.owner_tags["ackness/repoe"].update({"handoff/new-game": game["revision"],
                                               "handoff/new-fixtures": fixtures["revision"]})
        self.owner_tags["ackness/poe2-zh-cn-stats"].update(
            {"handoff/new-language": language["revision"], "handoff/new-display": support["revision"]})
        # compact_release reads the adopted shared rules from the candidate root.
        for name in self.lock["shared_files"]:
            path = self.source / "data/overlay-common" / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((ROOT / "data/overlay-common" / name).read_bytes())
        return target, snapshot

    def test_new_patch_is_visible_but_cannot_relabel_current_requirements(self):
        document = self.next_publication()
        document["target"] = {**document["target"], "game_patch": "0.5.5.6"}
        self.publish(document["owner"], document)
        status, _, _ = latest.discover(self.source, self.fetch, self.head)
        self.assertFalse(status["same_target"])
        self.assertFalse(status["auto_eligible"])
        with self.assertRaisesRegex(ValueError, "--new-target"):
            latest.propose(self.source, self.root / "new-patch", self.fetch, self.head)
        self.assertFalse((self.root / "new-patch").exists())

    def test_new_target_candidate_is_explicit_and_applies_both_files(self):
        target, snapshot = self.new_patch_publication()
        shared = {"overlay-common/" + name: spec["sha256"]
                  for name, spec in self.lock["shared_files"].items()}
        real_expectations = latest.build_expectations
        # The adopted lock's real archives are not in the synthetic fetcher; its
        # local shared layer is identical, which is all the comparison needs.
        def expectations(lock, cache, fetcher):
            if lock["target"]["game_patch"] == target["game_patch"]:
                return real_expectations(lock, cache, fetcher)
            return {}, shared
        out = self.root / "new-patch"
        with self.assertRaisesRegex(ValueError, "--new-target"):
            latest.propose(self.source, out, self.fetch, self.head, tags=self.tags)
        self.assertFalse(out.exists())
        with patch.object(latest, "build_expectations", side_effect=expectations):
            result = latest.propose(self.source, out, self.fetch, self.head, tags=self.tags,
                                    allow_new_target=True)
        change = {"previous": self.lock["target"]["game_patch"], "proposed": target["game_patch"]}
        self.assertEqual(result["target_change"], change)
        self.assertFalse(result["auto_eligible"])
        receipt = latest.verify_candidate(out)
        self.assertEqual(receipt["target_change"], change)
        lock, _ = latest.read_lock(out)
        self.assertEqual(lock["target"], target)
        self.assertEqual(lock["generation"], self.lock["generation"] + 1)
        self.assertEqual(lock["snapshot_manifest"], latest.pin(snapshot))
        self.assertNotEqual(lock["runtime_identity"], self.lock["runtime_identity"])
        self.assertNotEqual(lock["release_id"], self.lock["release_id"])
        self.assertEqual((out / "data/CURRENT").read_bytes(), (target["game_patch"] + "\n").encode())
        applied = latest.apply_candidate(out, self.source, sha(self.before[latest.LOCK]))
        self.assertEqual(applied["status"], "metadata-staged-not-adopted")
        self.assertEqual((self.source / "data/CURRENT").read_bytes(), (target["game_patch"] + "\n").encode())
        self.assertEqual(latest.read_lock(self.source)[0]["target"], target)

    def test_new_target_requires_identical_owner_targets(self):
        self.next_publication()
        repoe, previous = copy.deepcopy(self.descriptors["ackness/repoe"])
        repoe.update(sequence=repoe["sequence"] + 1, previous=previous,
                     target={**repoe["target"], "game_patch": "0.5.5.6"})
        self.publish("ackness/repoe", repoe)
        with self.assertRaisesRegex(ValueError, "different targets"):
            latest.propose(self.source, self.root / "mixed", self.fetch, self.head,
                           tags=self.tags, allow_new_target=True)
        self.assertFalse((self.root / "mixed").exists())

    def test_unavailable_index_does_not_replay_old_data_as_latest(self):
        def unavailable(address):
            raise OSError("Synthetic remote unavailable")
        with self.assertRaisesRegex(OSError, "unavailable"):
            latest.discover(self.source, unavailable, self.head)
        self.assertEqual(self.snapshot(), self.before)

    def test_candidate_tampering_and_missing_owner_evidence_reject(self):
        self.next_publication()
        out = self.root / "candidate"
        latest.propose(self.source, out, self.fetch, self.head)
        name = out / latest.LOCK
        raw = name.read_bytes()
        name.write_bytes(raw + b" ")
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            latest.verify_candidate(out)
        name.write_bytes(raw)
        next((out / "evidence").iterdir()).unlink()
        with self.assertRaisesRegex(ValueError, "Not a regular"):
            latest.verify_candidate(out)

    def test_adopted_lock_drift_and_current_mismatch_reject(self):
        for change in ("schema", "tag", "release"):
            document = copy.deepcopy(self.lock)
            if change == "schema": document["schema"] = "pobr-build-data-lock/v2"
            elif change == "tag": del document["packages"]["game"]["tag"]
            else: document["release_id"] = "unknown"
            (self.source / latest.LOCK).write_bytes(encoded(document))
            with self.subTest(change=change), self.assertRaises(ValueError):
                latest.read_lock(self.source)
        (self.source / latest.LOCK).write_bytes(self.before[latest.LOCK])
        (self.source / "data/CURRENT").write_bytes(b"0.5.5.4.2\n")
        with self.assertRaisesRegex(ValueError, "CURRENT"):
            latest.read_lock(self.source)

    def test_local_only_role_cannot_enter_latest(self):
        owner = "ackness/repoe"
        document, _ = copy.deepcopy(self.descriptors[owner])
        document["components"]["game"]["input"]["publication"] = "local-only"
        self.publish(owner, document)
        with self.assertRaisesRegex(ValueError, "Unpublished latest"):
            latest.discover(self.source, self.fetch, self.head)

    def test_unsafe_fresh_candidate_path_and_bad_git_timeout_reject(self):
        self.next_publication()
        alias = self.root / "alias"
        alias.symlink_to(self.source, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "Unsafe data directory"):
            latest.propose(self.source, alias / "candidate", self.fetch, self.head)
        for value in (0, -1, True, float("inf")):
            with self.subTest(timeout=value), self.assertRaisesRegex(ValueError, "timeout"):
                latest.imports.GitInputs(self.root / "git", timeout=value)

    def test_official_new_version_blocks_older_owner_frontiers(self):
        official = {"version": "0.5.5.6", "scope": "Synthetic observer result"}
        status, _, _ = latest.discover(self.source, self.fetch, self.head, official=official)
        self.assertEqual(status["status"], "waiting-for-owner-publication")
        self.assertFalse(status["auto_eligible"])
        with self.assertRaisesRegex(ValueError, "lacks complete"):
            latest.propose(self.source, self.root / "too-old", self.fetch, self.head, official=official)
        self.assertFalse((self.root / "too-old").exists())

    def test_existing_fresh_observation_preserves_scope_and_rejects_stale_or_other_code(self):
        config = self.channels
        directory = self.root / "observation"
        directory.mkdir()
        response = b"Synthetic observer response, not a live TCP receipt"
        (directory / "response.bin").write_bytes(response)
        now = datetime.now(timezone.utc)
        record = {"schema": "repoe-client-version-observation/v1", "status": "endpoint-version-observation",
                  "endpoint": {"host": "patch.pathofexile2.com", "port": 13060}, "handshake_hex": "0107",
                  "producer": {"module": "RePoE.client_version", "code_sha256": {
                      name: spec["sha256"] for name, spec in config["official_observer"]["files"].items()}},
                  "response": {"path": "response.bin", **latest.pin(response)}, "version": "0.5.5.5",
                  "cdn": "https://patch-poe2.poecdn.com/0.5.5.5/", "scope": "Unknown raw client identity remains unknown",
                  "observed_at_utc": now.isoformat()}
        path = directory / "observation.json"
        path.write_bytes(encoded(record))
        observed = latest.observation(directory, config, now)
        self.assertEqual(observed["scope"], record["scope"])
        self.assertTrue(latest.discover(self.source, self.fetch, self.head, official=observed)[0]["auto_eligible"])
        with self.assertRaisesRegex(ValueError, "stale"):
            latest.observation(directory, config, now + timedelta(days=2))
        record["producer"]["code_sha256"]["RePoE/client_version.py"] = "0" * 64
        path.write_bytes(encoded(record))
        with self.assertRaisesRegex(ValueError, "producer differs"):
            latest.observation(directory, config, now)

    def test_language_binding_and_evidence_cannot_be_dropped_or_retargeted(self):
        owner = "ackness/poe2-zh-cn-stats"
        original = copy.deepcopy(self.descriptors[owner][0])
        for change in ("binding", "evidence", "validation-revision", "validation-path", "complete"):
            document = copy.deepcopy(original)
            if change == "binding": document.pop("bindings")
            elif change == "evidence": document.pop("evidence")
            elif change == "validation-revision": document["evidence"]["catalog_validation"]["revision"] = "0" * 40
            elif change == "validation-path": document["evidence"]["catalog_validation"]["path"] = "../validation.json"
            else: document["scope"]["complete"] = True
            self.publish(owner, document)
            with self.subTest(change=change), self.assertRaises(ValueError):
                latest.discover(self.source, self.fetch, self.head)
        document = copy.deepcopy(original)
        document["bindings"]["display_source"]["revision"] = "0" * 40
        self.publish(owner, document)
        with self.assertRaisesRegex(ValueError, "binding differs"):
            latest.bootstrap(self.source, self.fetch, self.head)

    def test_binding_only_language_publication_preserves_component_sequences(self):
        source = self.next_publication("ackness/repoe", "display_source")
        owner = "ackness/poe2-zh-cn-stats"
        language, previous = copy.deepcopy(self.descriptors[owner])
        before = copy.deepcopy(language["components"])
        language.update(sequence=2, previous=previous)
        language["bindings"]["display_source"] = source["components"]["display_source"]["input"]
        self.publish(owner, language)
        output = self.root / "binding-only"
        latest.propose(self.source, output, self.fetch, self.head)
        latest.verify_candidate(output)
        self.assertEqual(language["components"], before)
        lock, _ = latest.read_lock(output)
        self.assertEqual({owner: entry["sequence"] for owner, entry in lock["owner_publications"].items()},
                         {owner: 2 for owner in latest.OWNERS})
        self.assertEqual(lock["packages"], self.lock["packages"])

    def test_sequence_only_empty_publication_is_rejected(self):
        owner = "ackness/repoe"
        document, previous = copy.deepcopy(self.descriptors[owner])
        document.update(sequence=2, previous=previous)
        self.publish(owner, document)
        with self.assertRaisesRegex(ValueError, "Empty owner publication"):
            latest.discover(self.source, self.fetch, self.head)

    def test_metadata_reader_rejects_oversized_blob_before_reading_it(self):
        reader = latest.imports.GitInputs(self.root / "small-git", timeout=1)
        address = ("https://github.com/ackness/repoe.git", "a" * 40, "publication/index.json")
        reader.repositories[address[:2]] = self.root / "already-fetched-synthetic-objects"
        with patch.object(latest.imports.subprocess, "check_output", return_value=b"2097153\n") as command:
            with self.assertRaisesRegex(ValueError, "exceeds size limit"):
                latest.Publications(reader).get("ackness/repoe", address[1], address[2])
        self.assertEqual(command.call_count, 1)
        self.assertIn("-s", command.call_args.args[0])
        self.assertEqual(command.call_args.kwargs["timeout"], 1)

    def test_metadata_apply_uses_composition_cas_and_preserves_installed_runtime(self):
        self.next_publication()
        out = self.root / "candidate"
        latest.propose(self.source, out, self.fetch, self.head)
        marker = self.source / "data/0.5.5.5/untracked-runtime.json"
        marker.parent.mkdir(exist_ok=True)
        marker.write_bytes(b'"Installed data must stay unchanged"')
        result = latest.apply_candidate(out, self.source, sha(self.before[latest.LOCK]))
        self.assertEqual(result["status"], "metadata-staged-not-adopted")
        self.assertEqual(marker.read_bytes(), b'"Installed data must stay unchanged"')
        self.assertEqual((self.source / "data/CURRENT").read_bytes(), self.before["data/CURRENT"])
        self.assertEqual(latest.read_lock(self.source)[0]["generation"], self.lock["generation"] + 1)
        with self.assertRaisesRegex(ValueError, "Expected data lock changed"):
            latest.apply_candidate(out, self.source, sha(self.before[latest.LOCK]))

    def test_discovery_cannot_rebind_an_old_frontier_to_a_newer_baseline(self):
        self.next_publication()
        middle = self.root / "middle"
        latest.propose(self.source, middle, self.fetch, self.head)
        latest.apply_candidate(middle, self.source, sha(self.before[latest.LOCK]))
        document = self.next_publication()
        document["components"]["workbench"]["input"]["revision"] = "e" * 40
        self.publish(document["owner"], document)
        newest = self.root / "newest"
        latest.propose(self.source, newest, self.fetch, self.head)
        # Freeze the older A->B result while the adopted baseline advances A->B->C.
        for name, raw in self.before.items(): (self.source / name).write_bytes(raw)
        frozen = latest.discover(self.source, self.fetch, self.head)
        # Keep a valid B frontier (instead of the now published C).
        middle_comp = latest.read_lock(middle)[0]
        frozen[0]["published_frontier"] = middle_comp["owner_publications"]
        frozen[2]["ackness/poe2-zh-cn-stats"] = latest.Publications(self.fetch).descriptor(
            "ackness/poe2-zh-cn-stats", middle_comp["owner_publications"]["ackness/poe2-zh-cn-stats"]["descriptor"])
        latest.apply_candidate(middle, self.source, sha(self.before[latest.LOCK]))
        latest.apply_candidate(newest, self.source, sha(latest.regular(middle / latest.LOCK)))
        current = self.snapshot()
        with patch.object(latest, "discover", return_value=frozen), self.assertRaisesRegex(ValueError, "changed during discovery"):
            latest.propose(self.source, self.root / "stale", self.fetch, self.head)
        self.assertFalse((self.root / "stale").exists())
        self.assertEqual(self.snapshot(), current)
        self.assertEqual(latest.read_lock(self.source)[0]["owner_publications"]["ackness/poe2-zh-cn-stats"]["sequence"], 3)

    def test_resealed_lock_owner_sequence_must_match_descriptor_before_apply(self):
        self.next_publication()
        out = self.root / "candidate"
        latest.propose(self.source, out, self.fetch, self.head)
        path = out / latest.LOCK
        document = bundle.decode(path.read_bytes())
        document["owner_publications"]["ackness/repoe"]["sequence"] = 999
        raw = encoded(document)
        path.write_bytes(raw)
        receipt_path = out / "resolution.json"
        receipt = bundle.decode(receipt_path.read_bytes())
        receipt["lock_id"] = sha(raw)
        receipt["files"][latest.LOCK] = latest.pin(raw)
        receipt_path.write_bytes(encoded(receipt))
        with self.assertRaisesRegex(ValueError, "owner sequence differs"):
            latest.verify_candidate(out)
        with self.assertRaisesRegex(ValueError, "owner sequence differs"):
            latest.apply_candidate(out, self.source, sha(self.before[latest.LOCK]))
        self.assertEqual(self.snapshot(), self.before)

    def test_metadata_apply_rolls_back_mid_write_and_refuses_drift(self):
        self.next_publication()
        out = self.root / "candidate"
        latest.propose(self.source, out, self.fetch, self.head)
        def fail_after_write(source, target):
            os.replace(source, target)
            raise OSError("Synthetic disk write failure")
        with self.assertRaisesRegex(OSError, "disk write failure"):
            latest.apply_candidate(out, self.source, sha(self.before[latest.LOCK]), replace=fail_after_write)
        for name, raw in self.before.items():
            self.assertEqual((self.source / name).read_bytes(), raw, name)
        drifted = copy.deepcopy(self.lock)
        drifted["shared_files"] = dict(sorted(drifted["shared_files"].items(), reverse=True))
        (self.source / latest.LOCK).write_bytes(encoded(drifted) + b"\n")
        with self.assertRaisesRegex(ValueError, "Expected data lock changed"):
            latest.apply_candidate(out, self.source, sha(self.before[latest.LOCK]))

    def test_recheck_rejects_superseded_candidate_and_allows_code_only_head_advance(self):
        self.next_publication()
        out = self.root / "candidate"
        latest.propose(self.source, out, self.fetch, self.head)
        official = {"version": "0.5.5.5", "scope": "Synthetic fixture"}
        self.assertEqual(latest.recheck(out, self.source, self.fetch, self.head, official=official)["status"],
                         "fresh-candidate-not-adopted")
        owner = "ackness/poe2-zh-cn-stats"
        old = self.frontier[owner]
        self.frontier[owner] = "b" * 40
        self.payloads[(f"https://github.com/{owner}.git", "b" * 40, "publication/index.json")] = self.payloads[
            (f"https://github.com/{owner}.git", old, "publication/index.json")]
        latest.recheck(out, self.source, self.fetch, self.head, official=official)
        document = self.next_publication()
        # A genuinely new input, not another sequence with the same role.
        document["components"]["workbench"]["input"]["revision"] = "e" * 40
        self.publish(owner, document)
        with self.assertRaisesRegex(ValueError, "supersedes"):
            latest.recheck(out, self.source, self.fetch, self.head, official=official)

    def test_consumed_role_change_records_its_single_owner_tag(self):
        self.next_publication("ackness/repoe", "test_fixtures", "e" * 40)
        with self.assertRaisesRegex(ValueError, "exactly one handoff/\\* tag"):
            latest.propose(self.source, self.root / "untagged", self.fetch, self.head, tags=self.tags)
        self.assertFalse((self.root / "untagged").exists())
        self.owner_tags["ackness/repoe"].update({"handoff/fixtures-a": "e" * 40, "handoff/fixtures-b": "e" * 40})
        with self.assertRaisesRegex(ValueError, "exactly one handoff/\\* tag"):
            latest.propose(self.source, self.root / "ambiguous", self.fetch, self.head, tags=self.tags)
        del self.owner_tags["ackness/repoe"]["handoff/fixtures-b"]
        out = self.root / "tagged"
        latest.propose(self.source, out, self.fetch, self.head, tags=self.tags)
        latest.verify_candidate(out)
        lock, _ = latest.read_lock(out)
        self.assertEqual(lock["test_fixtures"]["revision"], "e" * 40)
        self.assertEqual(lock["test_fixtures"]["tag"], "handoff/fixtures-a")
        # Test fixtures are not runtime inputs: the release identity is unchanged.
        self.assertEqual(lock["release_id"], self.lock["release_id"])
        self.assertEqual(self.snapshot(), self.before)

    def test_runtime_input_change_recomputes_identity_and_release_id(self):
        self.next_publication("ackness/repoe", "game", "e" * 40)
        self.owner_tags["ackness/repoe"]["handoff/game-next"] = "e" * 40
        expected = {"snapshot_manifest": {"bytes": 1, "sha256": "1" * 64}, "runtime_identity": "2" * 64}
        with patch.object(latest, "build_expectations", return_value=(expected, {})), \
                patch.object(latest, "compact_release", return_value="3" * 64) as release:
            latest.propose(self.source, self.root / "runtime", self.fetch, self.head, tags=self.tags)
        lock, _ = latest.read_lock(self.root / "runtime")
        self.assertEqual(lock["packages"]["game"]["tag"], "handoff/game-next")
        self.assertEqual(lock["runtime_identity"], "2" * 64)
        self.assertEqual(lock["release_id"], "3" * 64)
        # The release is computed from the candidate lock, not the adopted one.
        self.assertEqual(release.call_args.args[0]["packages"]["game"]["revision"], "e" * 40)

    def test_owner_may_retire_an_unconsumed_role_but_not_a_consumed_one(self):
        owner = "ackness/poe2-zh-cn-stats"
        document, previous = copy.deepcopy(self.descriptors[owner])
        document.update(sequence=2, previous=previous)
        del document["components"]["workbench"]
        self.publish(owner, document)
        status, _, _ = latest.discover(self.source, self.fetch, self.head)
        self.assertEqual(status["status"], "update-available")
        document, previous = copy.deepcopy(self.descriptors[owner])
        document.update(sequence=3, previous=previous)
        del document["components"]["support_display"]
        self.publish(owner, document)
        with self.assertRaisesRegex(ValueError, "Incomplete owner publication roles"):
            latest.discover(self.source, self.fetch, self.head)
        self.assertEqual(self.snapshot(), self.before)

    def test_tag_listing_peels_annotated_tags_and_keeps_lightweight_tags(self):
        listing = ("a" * 40 + "\trefs/tags/handoff/annotated\n"
                   + "b" * 40 + "\trefs/tags/handoff/annotated^{}\n"
                   + "c" * 40 + "\trefs/tags/handoff/light\n").encode()
        with patch.object(latest.subprocess, "check_output", return_value=listing) as call:
            tags = latest.resolve_tags("ackness/repoe")
        self.assertEqual(tags, {"handoff/annotated": "b" * 40, "handoff/light": "c" * 40})
        self.assertIn("refs/tags/handoff/*", call.call_args.args[0])
        self.assertEqual(latest.tag_for("b" * 40, tags), "handoff/annotated")
        with self.assertRaisesRegex(ValueError, "exactly one handoff"):
            latest.tag_for("a" * 40, tags)
        with self.assertRaisesRegex(ValueError, "exactly one handoff"):
            latest.tag_for("c" * 40, {**tags, "handoff/second": "c" * 40})
        with self.assertRaisesRegex(ValueError, "Untrusted owner"):
            latest.resolve_tags("someone/else")

    def test_build_expectations_authenticate_metadata_and_require_language_alignment(self):
        version, wanted = "0.5.5.5", self.lock["target"]
        prefix = "data/" + version + "/"
        # The owner's game part carries the crafting catalog like any other game file.
        content = {"base/skills.json": b'{"id":9223372036854775807}',
                   "overlay/trade_crafting_sources.json": b'{"mods":[]}',
                   "i18n/zh-CN/display_texts.json": b'{"text":null}'}
        snapshot = encoded({"schema_version": 3, "poe_version": version, "languages": ["zh-CN"],
                            "domains": {"base": ["skills"], "overlay": ["trade_crafting_sources"]},
                            "files": {n: sha(v) for n, v in content.items()}})
        files = {prefix + n: v for n, v in content.items() if not n.startswith("i18n/")}
        files[prefix + "manifest.json"] = snapshot
        manifest = encoded({"schema": latest.build_data.PART_SCHEMA, "kind": "game", "target": wanted,
                            "files": {n: latest.pin(v) for n, v in files.items()}})
        handle = io.BytesIO()
        with zipfile.ZipFile(handle, "w") as archive:
            for name, raw in {**files, "manifest.json": manifest}.items(): archive.writestr(name, raw)
        game_raw = handle.getvalue()
        locale = encoded({"schema": "poe2-retained-locale-publication/v1", "target": wanted,
                          "files": {n: latest.pin(v) for n, v in content.items() if n.startswith("i18n/")}})
        game = {"repository": "ackness/repoe", "revision": "1" * 40, "path": "handoff/game.zip",
                **latest.pin(game_raw), "manifest": latest.pin(manifest)}
        language = {"repository": "ackness/poe2-zh-cn-stats", "revision": "2" * 40,
                    "path": "handoff/locale.json", "format": "locale-git-files", "manifest": latest.pin(locale)}
        build = {"target": wanted, "packages": {"game": game, "language": language}, "shared_files": {}}
        payloads = {latest.git_address(game, game["path"]): game_raw,
                    latest.git_address(language, language["path"]): locale}
        cache = self.root / "package-cache"
        cache.mkdir()
        actual, shared = latest.build_expectations(build, cache, payloads.__getitem__)
        self.assertEqual(actual["snapshot_manifest"], latest.pin(snapshot))
        self.assertEqual(actual["runtime_identity"], sha(bundle.encoded([
            "pobr-data-pack-v1", 3, version, {n: sha(v) for n, v in content.items()}])))
        self.assertEqual(shared, {})
        changed = bundle.decode(locale)
        changed["files"]["i18n/zh-CN/display_texts.json"]["sha256"] = "0" * 64
        locale = encoded(changed)
        language["manifest"] = latest.pin(locale)
        payloads[latest.git_address(language, language["path"])] = locale
        with self.assertRaisesRegex(ValueError, "consumer-data-divergence"):
            latest.build_expectations(build, cache, payloads.__getitem__)


if __name__ == "__main__":
    unittest.main()
