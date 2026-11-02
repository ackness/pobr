"""Offline orchestration tests; the executable fixture is not a Rust producer."""
import copy
from pathlib import Path
import sys
import tempfile
import zipfile
import unittest
from unittest.mock import patch
from pipeline.runtime.ordinary import ordinary_policy as policy
from pipeline.runtime import runtime_pack as runtime
from pipeline.runtime.ordinary import ordinary_inputs as o
class Contracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        # A sealed synthetic source envelope from the independent owner's tests.
        # The tiny verifier below tests routing only; real source semantics stay upstream.
        fixture = o.load(Path(__file__).parent / "fixtures/ordinary-source-synthetic.json")
        self.assertTrue(fixture["synthetic"])
        self.local = self.root / "portable"
        self.target = fixture["target"]
        archive = self.root / "out/ordinary-input.zip"
        archive.parent.mkdir()
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as package:
            for name, text in fixture["files"].items():
                package.writestr(name, text.encode())
        handoff = fixture["handoff"]
        handoff["archive"].update(o.pin(archive.read_bytes()))
        handoff["synthetic_fixture"] = True
        (archive.parent / "handoff.json").write_bytes(o.encoded(handoff))
        self.sdk = {
            "RePoE/__init__.py": b'"""Synthetic verifier, never for real source data."""\n',
            "RePoE/source_io.py": b'from hashlib import sha256\n',
            "RePoE/source_rebinding.py": b'''from .source_io import sha256
from pathlib import Path
import zipfile
def verify(archive, handoff, output):
    if handoff.get("synthetic_fixture") is not True:
        raise ValueError("Only explicitly synthetic source envelopes")
    raw = Path(archive).read_bytes()
    if {"bytes": len(raw), "sha256": sha256(raw).hexdigest()} != {k: handoff["archive"][k] for k in ("bytes", "sha256")}:
        raise ValueError("Synthetic envelope pin mismatch")
    with zipfile.ZipFile(archive) as package:
        for name in package.namelist():
            path = Path(output) / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(package.read(name))
''',
        }
        self.sdk_spec = {"schema": "pobr-source-verifier/v1", "repository": "ackness/repoe",
                         "revision": "a" * 40, "files": {name: o.pin(raw) for name, raw in self.sdk.items()}}
        def materialize(spec, output, cache, offline, fetcher):
            self.assertEqual(spec, self.sdk_spec)
            for name, raw in self.sdk.items():
                o.write(Path(output) / name, raw)
            return Path(output)
        self.addCleanup(patch.stopall)
        patch.object(o.source_verifier, "materialize", side_effect=materialize).start()
        self.request = {"schema": "pobr-ordinary-requirements/v1", "target": self.target,
                        "domains": [], "retained": "Other runtime domains are external."}
        self.lock = {"schema": "pobr-ordinary-input-lock/v1", "target": self.target,
                     "publication": "pending", "package": None, "inputs": {}, "source_verifier": self.sdk_spec}
        self.add_unit(o.ARCHIVE_INPUT, o.ARCHIVE_INPUT, {o.ARCHIVE_INPUT: (self.root / "out/ordinary-input.zip").read_bytes()})
        self.add_unit(o.HANDOFF_INPUT, o.HANDOFF_INPUT, {o.HANDOFF_INPUT: (self.root / "out/handoff.json").read_bytes()})
        for domain in ("minions", "spectres"):
            self.add_unit(domain + "/source", "legacy.json", {"legacy.json": b'{"unknown":[null,-5]}\n'})
            policy = {"target": {"facts_manifest_sha256": o.load(self.root / 'out/handoff.json')['facts_manifest']['sha256'],
                                 "game_patch": self.target['game_patch']}}
            self.add_unit(domain + "/policy", "policy.json", {"policy.json": o.encoded(policy)})
            raw = b'{"unknown":17}\n'
            manifest = {"format": "repoe-native-tables/v1", "production_ready": False,
                        "source_identity": self.target, "files": {"tables.json": o.pin(raw)}}
            self.add_unit(domain + "/supplement", ".", {"tables.json": raw, "manifest.json": o.encoded(manifest)})
            self.request["domains"].append({"domain": domain,
                "selected_facts": ["source-lock.json", "tables/A.json"], "options": {
                    "--codec-source": {"input": domain + "/source"},
                    "--codec-source-sha256": self.lock["inputs"][domain + "/source"]["files"]["legacy.json"]["sha256"],
                    "--codec-policy": {"input": domain + "/policy"},
                    "--codec-policy-sha256": self.lock["inputs"][domain + "/policy"]["files"]["policy.json"]["sha256"],
                    "--supplement": {"input": domain + "/supplement"},
                    "--supplement-manifest-sha256": self.lock["inputs"][domain + "/supplement"]["files"]["manifest.json"]["sha256"]}})

    def add_unit(self, key, entry, files):
        self.lock["inputs"][key] = {"path": entry, "files": {name: o.pin(raw) for name, raw in files.items()}}
        for name, raw in files.items():
            path = self.local / key / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)

    def executable(self, mode="ok"):
        path = self.root / "adapter"
        # A real process emitting the receipt contract. Not native conversion.
        script = '''
import json, hashlib, sys
from pathlib import Path
mode = MODE
args = dict(zip(sys.argv[1::2], sys.argv[2::2]))
root=Path(args['--ordinary-facts']); out=Path(args['--out']); domain=args['--ordinary-domain']
out.mkdir(parents=True)
def encoded(x): return (json.dumps(x,sort_keys=True,indent=2)+'\\n').encode()
def pin(raw): return {'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
def put(name,raw):
    p=out/name; p.parent.mkdir(parents=True,exist_ok=True); p.write_bytes(raw)
manifest=json.loads((root/'manifest.json').read_bytes())
if mode=='partial' and domain=='spectres':
    put('_domain_candidate.json',b'{}'); sys.exit(17)
put('source-manifest.json',(root/'manifest.json').read_bytes())
put('source-lock.json',(root/'source-lock.json').read_bytes())
put('codec-policy.json',Path(args['--codec-policy']).read_bytes())
put('legacy-actors.json',Path(args['--codec-source']).read_bytes())
put('overlay/'+domain+'.json',b'{"unknown":[null,-5]}\\n')
marker={'schema':'pobr-domain-candidate/v1','domain':'ordinary-'+domain,'version':args['--version'],
        'source_id':manifest['source_identity']['source'],'source_manifest_sha256':args['--facts-manifest-sha256']}
if mode=='domain': marker['domain']='ordinary-wrong'
put('_domain_candidate.json',encoded(marker))
selected={k:manifest['files'][k] for k in ['source-lock.json','tables/A.json']}
receipt={'format':'pobr-ordinary-facts-receipt/v1','status':'domain-candidate','complete_runtime':False,
         'adoption':'not-adopted','target':{'game':'poe2','realm':'international','version':args['--version']},
         'adapter':{'name':'executable-test-fixture'},
         'input':{'manifest_sha256':args['--facts-manifest-sha256'],'root':str(root.resolve()),
                  'source_identity':manifest['source_identity'],'selected_payloads':selected},
         'files':{str(p.relative_to(out)):pin(p.read_bytes()) for p in out.rglob('*') if p.is_file()}}
if mode=='receipt': receipt['input']['manifest_sha256']='0'*64
if mode=='realm': receipt['target']['realm']='china'
if mode=='selected': receipt['input']['selected_payloads'].pop('tables/A.json')
if mode=='policy':
    put('codec-policy.json',b'{"changed":true}')
    receipt['files']['codec-policy.json']=pin((out/'codec-policy.json').read_bytes())
if mode=='extra': put('unlisted.json',b'{}')
if mode=='corrupt': put('overlay/'+domain+'.json',b'{"changed":true}')
put('producer-receipt.json',encoded(receipt))
'''.replace("MODE", repr(mode))
        path.write_text("#!" + sys.executable + "\n" + script)
        path.chmod(0o755)
        return path

    def run_candidate(self, mode="ok", name="candidate"):
        return o.run(self.request, self.lock, self.root / name, self.local, self.executable(mode))

    def test_tracked_plan_has_no_local_paths_and_complete_csd_units(self):
        root = Path(__file__).resolve().parent
        request, lock = o.load(root / "ordinary-inputs.requirements.json"), o.load(root / "ordinary-inputs.lock.json")
        o.plan(request, lock)
        self.assertNotIn(b'/Users/', o.encoded(lock))
        for key in ("stat-descriptions/item-descriptions", "passive-tree/tree-descriptions"):
            self.assertEqual(lock['inputs'][key]['path'], 'descriptions.json')
            self.assertIn('manifest.json', lock['inputs'][key]['files'])

    def test_plan_rejects_wrong_target_paths_pins_and_argument_profiles(self):
        for change in ('duplicate','version','realm','argument','missing','pair','traversal','boolbytes','unexpected','selection'):
            r,l=copy.deepcopy(self.request),copy.deepcopy(self.lock)
            if change=='duplicate': r['domains'].append(r['domains'][0])
            elif change=='version': r['target']['game_patch']='0.5.5.4'
            elif change=='realm': r['target']['realm']=l['target']['realm']='global'
            elif change=='argument': r['domains'][0]['options']['--tree-reference']={'input':'minions/source'}
            elif change=='missing': del l['inputs']['minions/source']
            elif change=='pair': del r['domains'][0]['options']['--codec-source-sha256']
            elif change=='traversal': l['inputs']['../escape']=l['inputs'].pop('minions/source')
            elif change=='boolbytes': l['inputs']['minions/source']['files']['legacy.json']['bytes']=True
            elif change=='unexpected': l['inputs']['unused']=copy.deepcopy(l['inputs']['minions/source'])
            else: r['domains'][0]['selected_facts'].append('tables/A.json')
            with self.subTest(change=change), self.assertRaises((ValueError,KeyError)):
                o.plan(r,l)

    def test_json_duplicate_keys_and_nonfinite_values_rejected(self):
        for raw in (b'{"a":1,"a":2}', b'{"a":NaN}'):
            p=self.root/'bad.json'; p.write_bytes(raw)
            with self.assertRaises(ValueError): o.load(p)

    def test_real_process_outputs_verified_and_completed_last(self):
        result=self.run_candidate()
        self.assertEqual(result['status'],'completed-declared-domains')
        self.assertEqual(set(result['domains']),{'minions','spectres'})
        self.assertFalse(result['production_ready'])
        self.assertTrue((self.root/'candidate/completed.json').is_file())
        self.assertEqual(o.load(self.root/'candidate/logs/minions/execution.json')['exit_code'],0)
        with self.assertRaisesRegex(ValueError,'already exists'): self.run_candidate()

    def test_verify_only_never_claims_production(self):
        result=o.run(self.request,self.lock,self.root/'candidate',self.local)
        self.assertEqual(result['status'],'verified-inputs-only')
        self.assertFalse((self.root/'candidate/completed.json').exists())
        self.assertFalse((self.root/'candidate/logs').exists())

    def test_new_runs_require_pinned_code_while_legacy_output_plans_remain_readable(self):
        del self.lock["source_verifier"]
        o.plan(self.request, self.lock)
        with self.assertRaisesRegex(ValueError, "pinned RePoE verifier"):
            self.run_candidate()
        self.assertFalse((self.root / "candidate").exists())

    def test_verifier_pins_and_final_recheck_precede_both_completion_markers(self):
        original = o.preflight
        for production in (False, True):
            name = "code-changed-" + str(production)
            def mutate(*args):
                result = original(*args)
                (self.root / name / "verification-library/RePoE/source_io.py").write_text("# changed\n")
                return result
            with patch.object(o, "preflight", side_effect=mutate), self.assertRaises(ValueError):
                o.run(self.request, self.lock, self.root / name, self.local,
                      self.executable() if production else None)
            self.assertFalse((self.root / name / "verified-inputs.json").exists())
            self.assertFalse((self.root / name / "completed.json").exists())

    def test_explicit_local_verifier_uses_same_pins_without_acquisition(self):
        library = self.root / "local-verifier"
        for name, raw in self.sdk.items(): o.write(library / name, raw)
        with patch.object(o.source_verifier, "materialize", side_effect=AssertionError("local must not fetch")):
            result = o.run(self.request, self.lock, self.root / "local-verified", self.local,
                           verifier_library=library)
        self.assertEqual(result["source_verifier"]["library"], self.sdk_spec)
        (library / "RePoE/source_io.py").write_bytes(b"# changed\n")
        with self.assertRaises(ValueError):
            o.run(self.request, self.lock, self.root / "bad-code", self.local, verifier_library=library)
        self.assertFalse((self.root / "bad-code/verified-inputs.json").exists())

    def test_parent_traversal_cannot_alias_output_into_local_inputs(self):
        alias = self.root / "alias"
        alias.mkdir()
        before = sorted(path.relative_to(self.local) for path in self.local.rglob("*"))
        for action in ("verify", "pack"):
            output = alias / ".." / "portable" / action
            with self.subTest(action=action), self.assertRaisesRegex(ValueError, "Parent traversal"):
                if action == "verify":
                    o.run(self.request, self.lock, output, self.local)
                else:
                    o.pack(self.request, self.lock, self.local, output)
            self.assertFalse((self.local / action).exists())
        with self.assertRaisesRegex(ValueError, "Parent traversal"):
            o.run(self.request, self.lock, self.root / "input-alias", alias / ".." / "portable")
        self.assertFalse((self.root / "input-alias").exists())
        self.assertEqual(sorted(path.relative_to(self.local) for path in self.local.rglob("*")), before)

    def test_wrong_pin_missing_file_and_symlink_fail_before_execution(self):
        path=self.local/'spectres/source/legacy.json'; original=path.read_bytes()
        for mutation in ('pin','missing','symlink'):
            if mutation=='pin': path.write_bytes(original+b' ')
            elif mutation=='missing': path.unlink()
            else:
                path.unlink()
                path.symlink_to(self.local/'minions/source/legacy.json')
            with self.subTest(mutation=mutation), self.assertRaises((ValueError,OSError)):
                self.run_candidate(name=mutation)
            self.assertFalse((self.root/mutation/'logs').exists())
            if path.exists() or path.is_symlink(): path.unlink()
            path.write_bytes(original)

    def test_source_realm_and_version_are_exact_before_execution(self):
        for field,value in [('realm','china'),('game_patch','0.5.5.50')]:
            r,l=copy.deepcopy(self.request),copy.deepcopy(self.lock)
            r['target'][field]=value; l['target'][field]=value
            out=self.root/field
            with self.subTest(field=field), self.assertRaisesRegex(ValueError,'game/patch/realm'):
                o.run(r,l,out,self.local,self.executable())
            self.assertFalse((out/'logs').exists())

    def test_supplement_wrong_realm_rejected_before_execution(self):
        p=self.local/'spectres/supplement/manifest.json'; meta=o.load(p)
        meta['source_identity']['realm']='china'; raw=o.encoded(meta); p.write_bytes(raw)
        self.lock['inputs']['spectres/supplement']['files']['manifest.json']=o.pin(raw)
        self.request['domains'][1]['options']['--supplement-manifest-sha256']=o.sha(raw)
        with self.assertRaisesRegex(ValueError,'game/patch/realm'): self.run_candidate()
        self.assertFalse((self.root/'candidate/logs').exists())

    def test_success_exit_without_valid_output_contract_fails(self):
        for mode in ('receipt','realm','selected','domain','policy','extra','corrupt'):
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                self.run_candidate(mode,mode)
            self.assertFalse((self.root/mode/'completed.json').exists())

    def test_policy_rebinding_is_only_the_authenticated_facts_manifest(self):
        key='minions/policy'; p=self.local/key/'policy.json'; current=o.load(p)
        original=copy.deepcopy(current)
        original['target']['facts_manifest_sha256']=o.load(self.root/'out/handoff.json')['original']['manifest']['sha256']
        raw=o.encoded(original); (p.parent/'original-policy.json').write_bytes(raw)
        unit=self.lock['inputs'][key]; unit['files']['original-policy.json']=o.pin(raw)
        unit['policy_rebinding']={'original':'original-policy.json','pointer':o.POLICY_TARGETS['minions']}
        o.run(self.request,self.lock,self.root/'valid-rebound',self.local)
        current['unrelated']='changed'; raw=o.encoded(current); p.write_bytes(raw)
        unit['files']['policy.json']=o.pin(raw)
        self.request['domains'][0]['options']['--codec-policy-sha256']=o.sha(raw)
        with self.assertRaisesRegex(ValueError,'non-target content'):
            self.run_candidate(name='bad-rebound')
        self.assertFalse((self.root/'bad-rebound/logs').exists())

    def test_stale_policy_manifest_is_rejected_before_first_producer(self):
        key='spectres/policy'; p=self.local/key/'policy.json'; policy=o.load(p)
        policy['target']['facts_manifest_sha256']='0'*64; raw=o.encoded(policy); p.write_bytes(raw)
        self.lock['inputs'][key]['files']['policy.json']=o.pin(raw)
        self.request['domains'][1]['options']['--codec-policy-sha256']=o.sha(raw)
        with self.assertRaisesRegex(ValueError,'Policy target manifest'): self.run_candidate()
        self.assertFalse((self.root/'candidate/logs').exists())

    def test_failed_second_domain_retains_first_and_partial_but_no_completion(self):
        with self.assertRaisesRegex(ValueError,'Producer failed: spectres'): self.run_candidate('partial')
        self.assertTrue((self.root/'candidate/minions/producer-receipt.json').is_file())
        self.assertTrue((self.root/'candidate/spectres/_domain_candidate.json').is_file())
        self.assertEqual(o.load(self.root/'candidate/logs/spectres/execution.json')['exit_code'],17)
        self.assertFalse((self.root/'candidate/completed.json').exists())

    def test_deterministic_compressed_package_git_fetch_then_offline_replay(self):
        a=o.pack(self.request,self.lock,self.local,self.root/'pack-a')
        b=o.pack(self.request,self.lock,self.local,self.root/'pack-b')
        self.assertEqual(a,b)
        lock=copy.deepcopy(self.lock); lock.update(publication='published',package={**a,'repository':'fixture/repo','revision':'1'*40})
        calls=[]
        def fetcher(address):
            calls.append(address)
            return (self.root/'pack-a'/a['path']).read_bytes()
        cache=self.root/'cache'
        first=o.run(self.request,lock,self.root/'fetched',cache=cache,fetcher=fetcher)
        second=o.run(self.request,lock,self.root/'offline',cache=cache,offline=True,
                     fetcher=lambda _: self.fail('offline fetch'))
        self.assertEqual(first,second); self.assertEqual(len(calls),1)
        self.assertEqual(o.inventory(self.root/'fetched/inputs'),o.inventory(self.root/'offline/inputs'))
        (cache/a['sha256']).write_bytes(b'corrupt')
        with self.assertRaisesRegex(ValueError,'bytes/SHA256'):
            o.run(self.request,lock,self.root/'corrupt-cache',cache=cache,offline=True)

    def split_owner_fixture(self):
        lock = copy.deepcopy(self.lock)
        lock.update(schema="pobr-ordinary-input-lock/v2", packages={}, policies={})
        del lock["publication"]; del lock["package"]
        for job in self.request["domains"]:
            old = job["options"]["--codec-policy"]["input"]
            key = job["domain"] + "/codec-policy"
            job["options"]["--codec-policy"]["input"] = key
            lock["inputs"][key] = lock["inputs"].pop(old)
            raw = (self.local / old / "policy.json").read_bytes()
            source = self.root / policy.SOURCES[key]
            source.parent.mkdir(parents=True, exist_ok=True); source.write_bytes(raw)
            lock["policies"][key] = {"source": o.pin(raw)}
        files = {f"{key}/{name}": expected for key, unit in lock["inputs"].items()
                 for name, expected in unit["files"].items()}
        blobs = {name: (self.local / name).read_bytes() for name in files
                 if not any(name.startswith(key + "/") for key in lock["policies"])}
        groups = {"facts": {n: r for n, r in blobs.items() if n.startswith((o.ARCHIVE_INPUT, o.HANDOFF_INPUT))},
                  "compatibility": {n: r for n, r in blobs.items() if not n.startswith((o.ARCHIVE_INPUT, o.HANDOFF_INPUT))}}
        archives = {}
        for key, values in groups.items():
            manifest = runtime.raw_json({"schema": "repoe-data-publication/v1", "target": self.target,
                                         "files": {n: o.pin(r) for n, r in values.items()}})
            raw = runtime.archive_bytes({**values, "manifest.json": manifest})
            spec = {**o.pin(raw), "manifest": o.pin(manifest), "expanded_bytes": len(manifest) + sum(map(len, values.values())),
                    "repository": "ackness/repoe", "revision": "b" * 40, "path": key + ".zip", "publication": "published"}
            lock["packages"][key] = spec
            archives[spec["path"]] = raw
        return lock, archives

    def test_multiple_owner_packages_materialize_policies_and_replay_offline(self):
        lock, archives = self.split_owner_fixture()
        with patch.object(policy, "ROOT", self.root):
            first = o.run(self.request, lock, self.root / "split", cache=self.root / "downloads",
                          fetcher=lambda address: archives[address[2]])
            second = o.run(self.request, lock, self.root / "offline-split", cache=self.root / "downloads", offline=True,
                           fetcher=lambda _: self.fail("offline network"))
        self.assertEqual(first, second)
        for key in lock["policies"]:
            self.assertEqual((self.root / "split/inputs" / key / "policy.json").read_bytes(),
                             (self.root / policy.SOURCES[key]).read_bytes())

    def test_split_package_missing_input_and_policy_source_drift_fail_before_producer(self):
        lock, archives = self.split_owner_fixture()
        source = self.root / policy.SOURCES["minions/codec-policy"]
        source.write_bytes(source.read_bytes() + b" ")
        with patch.object(policy, "ROOT", self.root), self.assertRaisesRegex(ValueError, "bytes/SHA256"):
            o.run(self.request, lock, self.root / "bad-policy-source", adapter=self.executable(),
                  fetcher=lambda address: archives[address[2]])
        self.assertFalse((self.root / "bad-policy-source/logs").exists())
        lock["packages"].pop("compatibility")
        with self.assertRaisesRegex(ValueError, "Missing external ordinary"):
            o.run(self.request, lock, self.root / "missing-owner", fetcher=lambda address: archives[address[2]])

    def test_retained_binding_order_survives_template_materialization(self):
        template = {"schema": "test", "bindings": {"$retained": "compatibility_bindings"}}
        bindings = {"compatibility_bindings": {"z": {"stats": 1, "context": 2}, "a": {"context": 3}}}
        expanded = policy.expand_template(template, o.decode(o.encoded(bindings)))
        self.assertEqual(list(expanded["bindings"]), ["z", "a"])
        self.assertEqual(list(expanded["bindings"]["z"]), ["stats", "context"])

    def test_pending_git_and_offline_cache_miss_cannot_run(self):
        with self.assertRaisesRegex(ValueError,'Pending Git'): o.run(self.request,self.lock,self.root/'pending')
        package=o.pack(self.request,self.lock,self.local,self.root/'pack')
        self.lock.update(publication='published',package={**package,'repository':'fixture/repo','revision':'1'*40})
        with self.assertRaisesRegex(ValueError,'Offline cache miss'):
            o.run(self.request,self.lock,self.root/'missing-cache',offline=True)


if __name__ == '__main__':
    unittest.main()
