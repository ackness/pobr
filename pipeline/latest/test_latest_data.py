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
        for name in (*latest.FILES.values(), "data/CURRENT"):
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((ROOT / name).read_bytes())
        self.documents = {key: bundle.decode((self.source / name).read_bytes())
                          for key, name in latest.FILES.items()}
        self.payloads, self.frontier, self.descriptors, self.calls = {}, {}, {}, []
        self.head_calls = []
        for owner in latest.OWNERS:
            publication = {"schema": latest.PUBLICATION_SCHEMA, "owner": owner,
                           "target": self.documents["build"]["target"], "sequence": 1, "previous": None,
                           "components": {role: {"sequence": 1, "input": value}
                                          for role, value in latest.declarations(self.documents, owner).items()},
                           "scope": {"complete": True, "meaning": "Synthetic declaration only"}}
            if owner == "ackness/poe2-zh-cn-stats":
                catalog = publication["components"]["display_catalog"]["input"]
                validation = encoded({"format": "windows-fk-validation/v1", "manifest_sha256": catalog["manifest"]["sha256"],
                                      "independent_fk_recomputed": True, "boundaries": {"observed_patch": None,
                                      "observed_realm": None, "official_client_semantics_verified": False,
                                      "production_ready": False}})
                publication.update(scope={"complete": "declared-dependency-closure-only",
                                          "freshness_certified": False, "official_client_certified": False,
                                          "translation_quality_certified": False},
                                   bindings={"display_source": self.documents["display"]["packages"]["repoe"]},
                                   evidence={"catalog_validation": {"revision": catalog["revision"],
                                             "path": str(Path(catalog["path"]).parent / "validation.json"),
                                             **latest.pin(validation)}})
                self.payloads[latest.git_address({"repository": owner, **publication["evidence"]["catalog_validation"]},
                              publication["evidence"]["catalog_validation"]["path"])] = validation
            self.publish(owner, publication)
        self.composition = latest.bootstrap(self.source, self.fetch, self.head)
        (self.source / "pipeline/latest/data-composition.json").write_bytes(encoded(self.composition))
        self.before = self.snapshot()
        self.calls.clear()
        self.head_calls.clear()

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

    def next_publication(self, owner="ackness/poe2-zh-cn-stats", role="workbench"):
        document, previous = copy.deepcopy(self.descriptors[owner])
        document.update(sequence=document["sequence"] + 1, previous=previous)
        value = document["components"][role]
        value["sequence"] += 1
        value["input"]["revision"] = "f" * 40
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
        result = latest.verify_composition(self.source, self.fetch)
        self.assertEqual(result["status"], "verified-immutable-declarations")
        self.assertEqual(result["freshness"], "not-checked")
        self.assertEqual(self.head_calls, [])
        self.assertEqual(self.snapshot(), self.before)
        document = copy.deepcopy(self.composition)
        document["owner_publications"]["ackness/repoe"]["sequence"] = 999
        (self.source / "pipeline/latest/data-composition.json").write_bytes(encoded(document))
        with self.assertRaisesRegex(ValueError, "rollback"):
            latest.verify_composition(self.source, self.fetch)

    def test_source_code_only_index_revision_does_not_make_a_data_update(self):
        owner = "ackness/repoe"
        before = self.frontier[owner]
        self.frontier[owner] = "d" * 40
        self.payloads[(f"https://github.com/{owner}.git", "d" * 40, "publication/index.json")] = self.payloads[
            (f"https://github.com/{owner}.git", before, "publication/index.json")]
        status, _, _ = latest.discover(self.source, self.fetch, self.head)
        self.assertEqual(status["status"], "no-new-publication")
        self.assertEqual(status["published_frontier"], self.composition["owner_publications"])

    def test_same_patch_module_change_creates_complete_candidate_and_keeps_current(self):
        self.next_publication()
        out = self.root / "candidate"
        result = latest.propose(self.source, out, self.fetch, self.head)
        self.assertEqual(result["status"], "update-available")
        receipt = latest.verify_candidate(out)
        composition, documents, _, _ = latest.read_composition(out)
        self.assertEqual(composition["generation"], 2)
        self.assertEqual(composition["previous_id"], sha(self.before["pipeline/latest/data-composition.json"]))
        self.assertEqual(documents["display"]["workbench"]["revision"], "f" * 40)
        self.assertEqual(composition["expected"], self.composition["expected"])
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

    def test_missing_crafting_or_cross_owner_role_rejects_without_candidate(self):
        owner = "ackness/repoe"
        original = copy.deepcopy(self.descriptors[owner][0])
        for failure in ("missing", "wrong-owner"):
            document = copy.deepcopy(original)
            if failure == "missing":
                del document["components"]["crafting"]
            else:
                document["components"]["crafting"]["input"]["repository"] = "ackness/pobr"
            self.publish(owner, document)
            with self.subTest(failure=failure), self.assertRaises(ValueError):
                latest.propose(self.source, self.root / failure, self.fetch, self.head)
            self.assertEqual(self.snapshot(), self.before)

    def test_role_change_without_sequence_advance_rejects(self):
        document = self.next_publication()
        document["components"]["workbench"]["sequence"] -= 1
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

    def test_new_patch_is_visible_but_cannot_relabel_current_requirements(self):
        document = self.next_publication()
        document["target"] = {**document["target"], "game_patch": "0.5.5.6"}
        self.publish(document["owner"], document)
        status, _, _ = latest.discover(self.source, self.fetch, self.head)
        self.assertFalse(status["same_target"])
        with self.assertRaisesRegex(ValueError, "New patch requires"):
            latest.propose(self.source, self.root / "new-patch", self.fetch, self.head)
        self.assertFalse((self.root / "new-patch").exists())

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
        name = out / "pipeline/imports/data-imports.lock.json"
        raw = name.read_bytes()
        name.write_bytes(raw + b" ")
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            latest.verify_candidate(out)
        name.write_bytes(raw)
        next((out / "evidence").iterdir()).unlink()
        with self.assertRaisesRegex(ValueError, "Not a regular"):
            latest.verify_candidate(out)

    def test_adopted_file_drift_and_current_mismatch_reject(self):
        name = self.source / "pipeline/imports/sources.lock.json"
        name.write_bytes(name.read_bytes() + b" ")
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            latest.read_composition(self.source)
        name.write_bytes(self.before["pipeline/imports/sources.lock.json"])
        (self.source / "data/CURRENT").write_bytes(b"0.5.5.4.2\n")
        with self.assertRaisesRegex(ValueError, "CURRENT"):
            latest.read_composition(self.source)

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
        config = self.documents["channels"]
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
        self.assertEqual(latest.read_composition(output)[1]["display"]["packages"]["repoe"]["revision"], "f" * 40)

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
        result = latest.apply_candidate(out, self.source, sha(self.before["pipeline/latest/data-composition.json"]))
        self.assertEqual(result["status"], "metadata-staged-not-adopted")
        self.assertEqual(marker.read_bytes(), b'"Installed data must stay unchanged"')
        self.assertEqual((self.source / "data/CURRENT").read_bytes(), self.before["data/CURRENT"])
        self.assertEqual(latest.read_composition(self.source)[0]["generation"], 2)
        with self.assertRaisesRegex(ValueError, "Expected composition changed"):
            latest.apply_candidate(out, self.source, sha(self.before["pipeline/latest/data-composition.json"]))

    def test_discovery_cannot_rebind_an_old_frontier_to_a_newer_baseline(self):
        self.next_publication()
        middle = self.root / "middle"
        latest.propose(self.source, middle, self.fetch, self.head)
        latest.apply_candidate(middle, self.source, sha(self.before["pipeline/latest/data-composition.json"]))
        document = self.next_publication()
        document["components"]["workbench"]["input"]["revision"] = "e" * 40
        self.publish(document["owner"], document)
        newest = self.root / "newest"
        latest.propose(self.source, newest, self.fetch, self.head)
        # Freeze the older A->B result while the adopted baseline advances A->B->C.
        for name, raw in self.before.items(): (self.source / name).write_bytes(raw)
        frozen = latest.discover(self.source, self.fetch, self.head)
        # Keep a valid B frontier (instead of the now published C).
        middle_comp = latest.read_composition(middle)[0]
        frozen[0]["published_frontier"] = middle_comp["owner_publications"]
        frozen[2]["ackness/poe2-zh-cn-stats"] = latest.Publications(self.fetch).descriptor(
            "ackness/poe2-zh-cn-stats", middle_comp["owner_publications"]["ackness/poe2-zh-cn-stats"]["descriptor"])
        latest.apply_candidate(middle, self.source, sha(self.before["pipeline/latest/data-composition.json"]))
        latest.apply_candidate(newest, self.source, sha(latest.regular(middle / "pipeline/latest/data-composition.json")))
        current = self.snapshot()
        with patch.object(latest, "discover", return_value=frozen), self.assertRaisesRegex(ValueError, "changed during discovery"):
            latest.propose(self.source, self.root / "stale", self.fetch, self.head)
        self.assertFalse((self.root / "stale").exists())
        self.assertEqual(self.snapshot(), current)
        self.assertEqual(latest.read_composition(self.source)[0]["owner_publications"]["ackness/poe2-zh-cn-stats"]["sequence"], 3)

    def test_resealed_composition_owner_sequence_must_match_descriptor_before_apply(self):
        self.next_publication()
        out = self.root / "candidate"
        latest.propose(self.source, out, self.fetch, self.head)
        path = out / "pipeline/latest/data-composition.json"
        document = bundle.decode(path.read_bytes())
        document["owner_publications"]["ackness/repoe"]["sequence"] = 999
        raw = encoded(document)
        path.write_bytes(raw)
        receipt_path = out / "resolution.json"
        receipt = bundle.decode(receipt_path.read_bytes())
        receipt["composition_id"] = sha(raw)
        receipt["files"]["pipeline/latest/data-composition.json"] = latest.pin(raw)
        receipt_path.write_bytes(encoded(receipt))
        with self.assertRaisesRegex(ValueError, "owner sequence differs"):
            latest.verify_candidate(out)
        with self.assertRaisesRegex(ValueError, "owner sequence differs"):
            latest.apply_candidate(out, self.source, sha(self.before["pipeline/latest/data-composition.json"]))
        self.assertEqual(self.snapshot(), self.before)

    def test_metadata_apply_rolls_back_mid_write_and_refuses_drift(self):
        self.next_publication()
        out = self.root / "candidate"
        latest.propose(self.source, out, self.fetch, self.head)
        count = 0
        def fail_second(source, target):
            nonlocal count
            count += 1
            if count == 2:
                raise OSError("Synthetic disk write failure")
            os.replace(source, target)
        with self.assertRaisesRegex(OSError, "disk write failure"):
            latest.apply_candidate(out, self.source, sha(self.before["pipeline/latest/data-composition.json"]), replace=fail_second)
        for name, raw in self.before.items():
            self.assertEqual((self.source / name).read_bytes(), raw, name)
        (self.source / "pipeline/imports/sources.lock.json").write_bytes(b"Changed after candidate resolution")
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            latest.apply_candidate(out, self.source, sha(self.before["pipeline/latest/data-composition.json"]))

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

    def test_build_expectations_authenticate_metadata_and_require_language_alignment(self):
        version, wanted = "0.5.5.5", self.documents["build"]["target"]
        prefix = "data/" + version + "/"
        content = {"base/skills.json": b'{"id":9223372036854775807}',
                   "i18n/zh-CN/display_texts.json": b'{"text":null}'}
        snapshot = encoded({"schema_version": 3, "poe_version": version, "languages": ["zh-CN"],
                            "domains": {"base": ["skills"]}, "files": {n: sha(v) for n, v in content.items()}})
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

    def test_ordinary_expectations_update_only_external_pins_and_request_checksums(self):
        documents = copy.deepcopy(self.documents)
        lock = documents["ordinary"]
        original_policies = {name: copy.deepcopy(lock["inputs"][name]) for name in lock["policies"]}
        files = {f"{unit}/{name}": {"bytes": spec["bytes"] + 1, "sha256": "f" * 64}
                 for unit, unit_spec in lock["inputs"].items() if unit not in lock["policies"]
                 for name, spec in unit_spec["files"].items()}
        manifest = encoded({"schema": "repoe-data-publication/v1", "target": lock["target"], "files": files})
        handle = io.BytesIO()
        with zipfile.ZipFile(handle, "w") as archive: archive.writestr("manifest.json", manifest)
        raw = handle.getvalue()
        spec = lock["packages"]["repoe"]
        spec.update(latest.pin(raw), manifest=latest.pin(manifest))
        cache = self.root / "ordinary-cache"
        cache.mkdir()
        latest.ordinary_expectations(documents, cache, lambda address: raw)
        self.assertEqual({name: lock["inputs"][name] for name in lock["policies"]}, original_policies)
        for unit, unit_spec in lock["inputs"].items():
            if unit not in lock["policies"]:
                self.assertTrue(all(value["sha256"] == "f" * 64 for value in unit_spec["files"].values()))
        for job in documents["ordinary_requirements"]["domains"]:
            for flag, checksum in latest.ordinary_inputs.PROFILES[job["domain"]][0].items():
                if checksum:
                    unit = job["options"][flag]["input"]
                    name = "manifest.json" if flag == "--supplement" else lock["inputs"][unit]["path"]
                    self.assertEqual(job["options"][checksum], lock["inputs"][unit]["files"][name]["sha256"])


if __name__ == "__main__":
    unittest.main()
