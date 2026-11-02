"""Exercise explicit locked selection and rollback with real small owner packages."""
import importlib.util
import contextlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import sys
import unittest
from unittest.mock import patch

from pipeline.runtime import data_bundle as bundle
from pipeline.runtime import data_snapshot as snapshot
from pipeline.runtime import locked_update as update
spec = importlib.util.spec_from_file_location('build_fixture', Path(__file__).with_name('test_build_data.py'))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
ROOT = Path(__file__).resolve().parents[2]


class LockedUpdateTests(unittest.TestCase):
    def setUp(self):
        self.f = fixture.BuildDataTests()
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.root = self.f.root
        self.receipt = self.f.prepare()
        self.data = self.root / 'source/data'
        self.before_validation_marker = b'8.1\n'
        shutil.copytree(self.data / '9.1', self.data / '8.1')
        snapshot.manifest(self.data / '8.1')
        (self.data / 'CURRENT').write_bytes(b'8.1\n')
        self.patch = self.data / '9.1/patch/base/skills.json'
        self.patch.parent.mkdir(parents=True)
        self.patch.write_bytes(b'[{"user":"keep target patch"}]\n')
        self.validation_calls = []

    def validate(self, stage, version):
        self.assertEqual((self.data / 'CURRENT').read_bytes(), self.before_validation_marker)
        self.assertEqual((stage / 'CURRENT').read_bytes(), (version + '\n').encode())
        self.validation_calls.append((stage, version))

    def tree(self):
        return update.fingerprint(self.data)

    def cli(self, name, *args):
        # Same entry as `mise run data:update`.
        return subprocess.run([sys.executable, '-m', 'pipeline.runtime.locked_update',
            '--lock', str(self.f.lock_path), '--out', str(self.root / name),
            '--cache', str(self.f.cache), '--data-root', str(self.data), '--offline', *args],
            cwd=ROOT, capture_output=True, text=True, timeout=15)

    def test_default_cli_prepares_candidate_without_mutating_current_or_inputs(self):
        before = self.tree()
        result = self.cli('candidate')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIsNone(json.loads(result.stdout)['selection'])
        self.assertEqual(bundle.verify(self.root / 'candidate'), self.receipt)
        self.assertEqual(self.tree(), before)
        self.assertFalse((self.root / 'vendor').exists())

    def test_cli_requires_exact_lock_patch_and_explicit_selection_pair(self):
        for args in (('--patch', '9.2'), ('--activate',), ('--expected-current', '8.1'),
                     ('--activate', '--expected-current', '7.1'), ('--skip-download',),
                     ('--expected-runtime', 'a' * 64),
                     ('--activate', '--expected-current', '8.1', '--expected-runtime', 'not-a-runtime'),
                     ('--vendor-sha', 'a' * 40)):
            before = self.tree()
            with self.subTest(args=args):
                result = self.cli('refused', *args)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.tree(), before)
                self.assertFalse((self.root / 'refused').exists())

    def test_cli_explicit_selection_retains_candidate_and_previous_version(self):
        old = update.fingerprint(self.data / '8.1')
        args = ['locked-update', '--lock', str(self.f.lock_path), '--out', str(self.root / 'candidate'),
                '--cache', str(self.f.cache), '--data-root', str(self.data), '--offline',
                '--activate', '--expected-current', '8.1']
        stdout = io.StringIO()
        with patch.object(sys, 'argv', args), patch.object(update, 'validate_candidate', self.validate), contextlib.redirect_stdout(stdout):
            update.main()
        self.assertEqual(json.loads(stdout.getvalue())['selection']['selected'], '9.1')
        self.assertEqual(len(self.validation_calls), 1)
        self.assertEqual((self.data / 'CURRENT').read_bytes(), b'9.1\n')
        self.assertEqual(update.fingerprint(self.data / '8.1'), old)
        self.assertEqual(bundle.verify(self.root / 'candidate'), self.receipt)
        self.assertEqual(self.patch.read_bytes(), b'[{"user":"keep target patch"}]\n')

    def test_same_version_refresh_preserves_patch_and_exact_new_runtime(self):
        (self.data / 'CURRENT').write_bytes(b'9.1\n')
        self.before_validation_marker = b'9.1\n'
        (self.data / '9.1/base/skills.json').write_bytes(b'[{"old":true}]\n')
        snapshot.manifest(self.data / '9.1')
        previous_runtime = bundle.data_inputs(self.data, '9.1')[1]
        result = update.activate(self.f.out, self.data, '9.1', self.validate,
                                 expected_runtime_identity=previous_runtime)
        self.assertEqual(result['previous_runtime_identity'], previous_runtime)
        self.assertEqual((self.data / '9.1/base/skills.json').read_bytes(),
                         self.f.contents['data/9.1/base/skills.json'])
        self.assertEqual(self.patch.read_bytes(), b'[{"user":"keep target patch"}]\n')
        self.assertEqual(bundle.snapshot_inputs(self.data.parent, '9.1')[1], self.receipt['runtime_identity'])
        self.assertEqual(bundle.verify(self.f.out), self.receipt)

    def test_same_patch_changed_runtime_refuses_late_candidate_without_validation(self):
        (self.data / 'CURRENT').write_bytes(b'9.1\n')
        expected = bundle.data_inputs(self.data, '9.1')[1]
        (self.data / '9.1/base/skills.json').write_bytes(b'[{"newer":true}]\n')
        snapshot.manifest(self.data / '9.1')
        before = self.tree()
        with self.assertRaisesRegex(ValueError, 'Expected runtime changed'):
            update.activate(self.f.out, self.data, '9.1', self.validate,
                            expected_runtime_identity=expected)
        self.assertEqual(self.tree(), before)
        self.assertFalse(self.validation_calls)

    def test_cli_runtime_mismatch_fails_before_preparing_candidate(self):
        before = self.tree()
        result = self.cli('stale-runtime', '--activate', '--expected-current', '8.1',
                          '--expected-runtime', 'a' * 64)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Expected runtime changed', result.stderr)
        self.assertEqual(self.tree(), before)
        self.assertFalse((self.root / 'stale-runtime').exists())

    def test_custom_data_directory_can_select_and_roll_back(self):
        rollback = self.root / 'rollback'
        old = bundle.create(self.data.parent, rollback)
        directory = self.data.with_name('runtime-store')
        self.data.rename(directory)
        self.data = directory
        self.patch = self.data / '9.1/patch/base/skills.json'
        update.activate(self.f.out, self.data, '8.1', self.validate)
        self.assertEqual((self.data / 'CURRENT').read_bytes(), b'9.1\n')
        self.before_validation_marker = b'9.1\n'
        update.activate(rollback, self.data, '9.1', self.validate)
        self.assertEqual((self.data / 'CURRENT').read_bytes(), b'8.1\n')
        self.assertEqual(bundle.data_inputs(self.data, '8.1')[1], old['runtime_identity'])
        self.assertEqual(len(self.validation_calls), 2)
        self.assertEqual(self.patch.read_bytes(), b'[{"user":"keep target patch"}]\n')
        self.assertFalse((self.data / '8.1/patch').exists())

    def test_custom_data_directory_checks_its_previous_snapshot_not_a_sibling(self):
        directory = self.data.with_name('runtime-store')
        shutil.copytree(self.data, directory)
        self.data = directory
        (self.data / '8.1/base/skills.json').write_bytes(b'[]')
        before = self.tree()
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            update.activate(self.f.out, self.data, '8.1', self.validate)
        self.assertEqual(self.tree(), before)
        self.assertFalse(self.validation_calls)

    def test_failed_marker_publication_restores_previous_target_and_current(self):
        before = self.tree()
        original = Path.replace
        def fail_marker(path, target):
            if path.name == 'CURRENT' and Path(target) == self.data / 'CURRENT':
                raise OSError('injected marker failure')
            return original(path, target)
        with patch.object(Path, 'replace', fail_marker), self.assertRaisesRegex(OSError, 'marker failure'):
            update.activate(self.f.out, self.data, '8.1', self.validate)
        self.assertEqual(self.tree(), before)
        self.assertEqual(bundle.verify(self.f.out), self.receipt)
        self.assertFalse(list((self.data.parent / '.cache').glob('data-backup-*')))

    def test_changed_current_shared_or_bundle_refuses_before_publication(self):
        with self.assertRaisesRegex(ValueError, 'Expected CURRENT'):
            update.activate(self.f.out, self.data, '7.1')
        own = self.data / 'overlay-common/own.json'
        own.write_bytes(b'{"changed":true}')
        before = self.tree()
        with self.assertRaisesRegex(ValueError, 'Shared rules differ'):
            update.activate(self.f.out, self.data, '8.1', self.validate)
        self.assertEqual(self.tree(), before)
        own.write_bytes(b'{"rule":"pobr"}')
        (self.f.out / 'data/9.1/base/skills.json').write_bytes(b'[]')
        before = self.tree()
        with self.assertRaisesRegex(ValueError, 'hash mismatch'):
            update.activate(self.f.out, self.data, '8.1', self.validate)
        self.assertEqual(self.tree(), before)

    def test_validation_failure_or_mutation_cannot_select_candidate(self):
        before = self.tree()
        def fail(stage, version):
            self.validate(stage, version)
            raise ValueError("runtime gate failed")
        with self.assertRaisesRegex(ValueError, "runtime gate failed"):
            update.activate(self.f.out, self.data, '8.1', fail)
        self.assertEqual(self.tree(), before)
        def mutate(stage, version):
            self.validate(stage, version)
            (stage / version / 'base/skills.json').write_bytes(b'[]')
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            update.activate(self.f.out, self.data, '8.1', mutate)
        self.assertEqual(self.tree(), before)

    def test_validator_cannot_change_target_patch_or_current_marker(self):
        before = self.tree()
        for name in ('patch', 'current'):
            def mutate(stage, version):
                self.validate(stage, version)
                path = stage / version / 'patch/base/skills.json' if name == 'patch' else stage / 'CURRENT'
                path.write_bytes(b'changed')
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'Validator changed'):
                update.activate(self.f.out, self.data, '8.1', mutate)
            self.assertEqual(self.tree(), before)

    def test_default_runtime_gates_use_candidate_and_never_bless_pins(self):
        calls = []
        def run(command, **kwargs): calls.append((command, kwargs))
        with patch.object(subprocess, 'run', run), patch.dict(update.os.environ, {'POBR_BLESS_PINS': '1'}):
            update.validate_candidate(self.root / 'candidate-data', '9.1')
        self.assertEqual(len(calls), 3)
        self.assertEqual(calls[0][0], ['bash', 'devs/scripts/driver.sh', 'versions'])
        self.assertIn('pobr-gamedata', calls[1][0])
        self.assertIn('parity_no_regression', calls[2][0])
        for _, kwargs in calls:
            self.assertTrue(kwargs['check'])
            self.assertEqual(kwargs['env']['POBR_DATA_VERSION'], '9.1')
            self.assertEqual(kwargs['env']['POBR_DATA_ROOT'], str(self.root / 'candidate-data'))
            self.assertNotIn('POBR_BLESS_PINS', kwargs['env'])
            self.assertEqual(kwargs['env']['CARGO_BUILD_RUSTC_WRAPPER'], '')

    def test_unlisted_candidate_file_or_symlink_and_parent_alias_refuse(self):
        extra = self.f.out / 'data/9.1/extra.json'
        extra.write_bytes(b'{}')
        before = self.tree()
        with self.assertRaisesRegex(ValueError, 'unlisted'):
            update.activate(self.f.out, self.data, '8.1', self.validate)
        self.assertEqual(self.tree(), before)
        extra.unlink()
        self.patch.unlink()
        self.patch.symlink_to(self.data / '9.1/base/skills.json')
        with self.assertRaisesRegex(ValueError, 'Unsafe data directory'):
            update.activate(self.f.out, self.data, '8.1', self.validate)
        with self.assertRaisesRegex(ValueError, 'Unsafe data directory'):
            update.activate(self.f.out, self.data / '..' / 'data', '8.1')

    def test_concurrent_current_change_is_not_overwritten(self):
        original = shutil.copytree
        def change_after_copy(*args, **kwargs):
            result = original(*args, **kwargs)
            (self.data / 'CURRENT').write_bytes(b'7.1\n')
            return result
        target = update.fingerprint(self.data / '9.1')
        with patch.object(shutil, 'copytree', change_after_copy), self.assertRaisesRegex(ValueError, 'changed before selection'):
            update.activate(self.f.out, self.data, '8.1', self.validate)
        self.assertEqual((self.data / 'CURRENT').read_bytes(), b'7.1\n')
        self.assertEqual(update.fingerprint(self.data / '9.1'), target)


if __name__ == '__main__':
    unittest.main()
