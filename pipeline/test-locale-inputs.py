"""Acquisition and candidate-boundary tests with a synthetic language producer."""

import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import locale_inputs as locale
from data_imports import encoded, sha


class LocaleInputsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="locale inputs ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.output = self.root / "output"
        self.cache = self.root / "cache"
        self.target = {"game": "poe2", "game_patch": "1.2.3", "realm": "international"}
        body = encoded({"synthetic": True})
        self.input = {"bytes": len(body), "sha256": sha(body), "source": {
            "url": "https://raw.githubusercontent.com/example/dictionary/" + "b" * 40 + "/data.json"}}
        self.manifest = {"schema": "poe2-locale-generation-input/v1", "target": self.target,
                         "profile": "legacy", "identity": {"synthetic": True},
                         "inputs": {"dictionary/data.json": self.input},
                         "expected_files": {name: {"bytes": len(body), "sha256": sha(body)} for name in locale.OUTPUTS}}
        # This stub isolates PoBR acquisition/receipt checks. Real generation and
        # selection are tested in the independent producer repository.
        module = b'''import fs from 'node:fs';
import path from 'node:path';
const a=Object.fromEntries(process.argv.slice(2).reduce((r,x,i,all)=>i%2?r:[...r,[x,all[i+1]]],[]));
const m=JSON.parse(fs.readFileSync(a['--manifest']));
const input=fs.readFileSync(path.join(a['--input-dir'],'dictionary/data.json'));
if(m.test_mode==='fail') process.exit(19);
fs.mkdirSync(a['--out']);
for(const name of Object.keys(m.expected_files)){
 const file=path.join(a['--out'],name); fs.mkdirSync(path.dirname(file),{recursive:true}); fs.writeFileSync(file,input);
}
if(m.test_mode==='extra') fs.writeFileSync(path.join(a['--out'],'unexpected.json'),'{}');
if(m.test_mode==='corrupt') fs.writeFileSync(path.join(a['--out'],'i18n/zh-CN/words.json'),'{}');
if(m.test_mode==='input-mutation') fs.writeFileSync(path.join(a['--input-dir'],'dictionary/data.json'),'{}');
const receipt={schema:'poe2-locale-generation-receipt/v1',target:m.target,manifest_sha256:a['--manifest-sha256'],
 identity:m.identity,profile:m.profile,inputs:m.inputs,layers:{synthetic:true},files:m.expected_files};
if(m.test_mode==='receipt') receipt.manifest_sha256='0'.repeat(64);
fs.writeFileSync(path.join(a['--out'],'receipt.json'),JSON.stringify(receipt));
'''
        modules = {name: b"// synthetic closure\n" for name in locale.MODULES}
        modules["src/locale-producer.mjs"] = module
        self.blobs = {("https://github.com/ackness/poe2-zh-cn-stats.git", "a" * 40, name): raw
                      for name, raw in modules.items()}
        self.blobs[("https://github.com/example/dictionary.git", "b" * 40, "data.json")] = body
        self.lock = {"schema": "pobr-locale-generation-lock/v1", "target": self.target,
                     "repository": locale.OWNER, "revision": "a" * 40,
                     "producer": {"entrypoint": "src/locale-producer.mjs", "files": {
                         name: {"bytes": len(raw), "sha256": sha(raw)} for name, raw in modules.items()}}}
        self.lock_path = self.root / "lock.json"
        self.calls = []
        self.save()

    def save(self):
        raw = encoded(self.manifest)
        self.lock["input_manifest"] = {"path": "handoff/input.json", "bytes": len(raw), "sha256": sha(raw)}
        self.blobs[("https://github.com/ackness/poe2-zh-cn-stats.git", "a" * 40, "handoff/input.json")] = raw
        self.lock_path.write_bytes(encoded(self.lock))

    def fetch(self, address):
        self.calls.append(address)
        return self.blobs[address]

    def prepare(self, **kwargs):
        return locale.prepare(self.lock_path, self.output, self.cache, fetcher=self.fetch, **kwargs)

    def test_pinned_sources_and_exact_outputs_replay_offline(self):
        receipt = self.prepare()
        self.assertEqual(set(receipt["files"]), locale.OUTPUTS)
        self.assertIn(("https://github.com/example/dictionary.git", "b" * 40, "data.json"), self.calls)
        self.output = self.root / "replay"
        with patch.object(self, "fetch", side_effect=AssertionError("offline must not fetch")):
            self.assertEqual(self.prepare(offline=True), receipt)

    def test_producer_relative_inputs_use_the_pinned_commit(self):
        self.input["source"] = {"repository": "https://github.com/" + locale.OWNER,
                                "revision": "producer-commit", "path": "evidence/input.json"}
        raw = self.blobs[("https://github.com/example/dictionary.git", "b" * 40, "data.json")]
        self.blobs[("https://github.com/ackness/poe2-zh-cn-stats.git", "a" * 40, "evidence/input.json")] = raw
        self.save()
        self.prepare()
        self.assertIn(("https://github.com/ackness/poe2-zh-cn-stats.git", "a" * 40, "evidence/input.json"), self.calls)

    def test_foreign_target_or_missing_module_fails_before_acquisition(self):
        with self.assertRaisesRegex(ValueError, "another patch"):
            self.prepare(version="1.2.4")
        self.assertFalse(self.calls)
        self.lock["producer"]["files"].pop("src/strict-json.mjs")
        self.save()
        with self.assertRaisesRegex(ValueError, "closure"):
            self.prepare()
        self.assertFalse(self.calls)

    def test_mutable_url_path_escape_and_foreign_producer_are_rejected(self):
        sources = [{"url": "https://raw.githubusercontent.com/example/dictionary/HEAD/data.json"},
                   {"url": "https://raw.githubusercontent.com/example/dictionary/" + "b" * 40 + "/../data.json"},
                   {"repository": "https://github.com/example/dictionary", "revision": "producer-commit", "path": "data.json"}]
        for source in sources:
            with self.subTest(source=source):
                self.input["source"] = source
                self.save()
                with self.assertRaises(ValueError):
                    self.prepare()
                self.assertFalse(self.output.exists())

    def test_corrupt_cache_is_not_refetched(self):
        self.prepare()
        self.output = self.root / "second"
        (self.cache / self.input["sha256"]).write_bytes(b"corrupt")
        self.calls.clear()
        with self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            self.prepare()
        self.assertFalse(self.calls)
        self.assertFalse(self.output.exists())

    def test_symlink_cache_and_existing_output_are_rejected(self):
        self.cache.mkdir()
        victim = self.root / "victim"
        victim.write_bytes(b"untouched")
        (self.cache / self.input["sha256"]).symlink_to(victim)
        with self.assertRaisesRegex(ValueError, "Symlink"):
            self.prepare()
        self.assertEqual(victim.read_bytes(), b"untouched")
        self.output.mkdir()
        with self.assertRaisesRegex(ValueError, "Fresh"):
            self.prepare()

    def test_failed_or_forged_producer_never_exposes_output(self):
        for mode in ("fail", "extra", "corrupt", "input-mutation", "receipt"):
            with self.subTest(mode=mode):
                self.manifest["test_mode"] = mode
                self.save()
                with self.assertRaises((ValueError, locale.subprocess.CalledProcessError)):
                    self.prepare()
                self.assertFalse(self.output.exists())

    def test_lock_mutation_is_rejected_after_generation(self):
        original = self.fetch
        def fetch(address):
            raw = original(address)
            self.lock_path.write_bytes(b"{}")
            return raw
        with patch.object(self, "fetch", fetch):
            with self.assertRaisesRegex(ValueError, "lock changed"):
                self.prepare()
        self.assertFalse(self.output.exists())

    def test_candidate_install_requires_transaction_and_preserves_other_language_files(self):
        receipt = self.prepare()
        candidate = self.root / "data" / "1.2.3"
        candidate.mkdir(parents=True)
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaisesRegex(ValueError, "transaction"):
                locale.install_candidate(self.output, candidate, self.target)
        tw = candidate / "i18n/zh-TW/base_items.json"
        display = candidate / "i18n/zh-CN/display_texts.json"
        for path in (tw, display):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"untouched")
        with patch.dict(os.environ, {"POBR_CANDIDATE": "1", "POBR_DATA_ROOT": str(candidate.parent)}):
            locale.install_candidate(self.output, candidate, self.target)
        self.assertEqual(tw.read_bytes(), b"untouched")
        self.assertEqual(display.read_bytes(), b"untouched")
        self.assertEqual(json.loads((candidate / locale.RECEIPT).read_bytes()), receipt)


if __name__ == "__main__":
    unittest.main()
