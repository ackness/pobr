"""Offline tests for failure-safe snapshot publication."""
from contextlib import contextmanager
import fcntl
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("data_snapshot", Path(__file__).with_name("data_snapshot.py"))
snapshot = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(snapshot)


def tree_fingerprint(path):
    return {p.relative_to(path).as_posix(): snapshot.digest(p) for p in path.rglob("*") if p.is_file()}


@contextmanager
def staged(root, version):
    """A writable copy of one version, as locked_update stages owner data."""
    with tempfile.TemporaryDirectory(prefix="data-update-", dir=root / ".cache") as temporary:
        data_root = Path(temporary) / "data"
        shutil.copytree(root / "data" / version, data_root / version)
        yield data_root


class SnapshotTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="pobr snapshot tests ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.write("data/CURRENT", "9.1\n")
        self.write("data/9.1/base/skill_gems.json", [{"id": "original"}])
        self.write("data/9.1/patch/base/skill_gems.json", [{"id": "user patch"}])
        self.write("data/9.1/generated/modifier-audit.json", {"original": True})
        self.write("data/overlay-common/special_mods.json", {"curated": True})
        self.write("data/8.1/base/skill_gems.json", [{"id": "golden"}])
        (self.root / ".cache").mkdir()

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value if isinstance(value, str) else json.dumps(value), encoding="utf-8")
        return path

    def test_second_update_cannot_acquire_workspace_lock(self):
        with (self.root / ".cache/data-update.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(ValueError, "Another data update"):
                with snapshot.workspace_lock(self.root):
                    pass

    def test_failed_install_restores_original_target_and_current(self):
        before = tree_fingerprint(self.root / "data")
        original_replace = Path.replace
        with staged(self.root, "9.1") as data:
            def replace(path, target):
                if path == data / "9.1":
                    raise OSError("injected install failure")
                return original_replace(path, target)
            with patch.object(Path, "replace", replace), self.assertRaisesRegex(OSError, "install failure"):
                snapshot.promote(data, self.root / "data", "9.1", activate=True)
        self.assertEqual(before, tree_fingerprint(self.root / "data"))

    def test_failed_install_and_failed_rollback_retain_original_backup(self):
        original_replace = Path.replace
        with staged(self.root, "9.1") as data:
            def replace(path, target):
                if path == data / "9.1" or path.parent.name.startswith("data-backup-"):
                    raise OSError("injected rename failure")
                return original_replace(path, target)
            with patch.object(Path, "replace", replace), self.assertRaisesRegex(OSError, "original snapshot retained"):
                snapshot.promote(data, self.root / "data", "9.1", activate=True)
        backups = list((self.root / ".cache").glob("data-backup-*/9.1/base/skill_gems.json"))
        self.assertEqual(len(backups), 1)
        self.assertIn("original", backups[0].read_text())


if __name__ == "__main__":
    unittest.main()
