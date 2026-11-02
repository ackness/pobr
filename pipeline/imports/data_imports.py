"""Pinned Git transport shared by every owner-data import.

Fetch an exact owner commit, read named blobs, verify bytes/SHA-256 and cache
them by content. Downloads are build inputs, never player assets.
"""

import hashlib
import json
import math
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile
import zipfile

from pipeline.runtime.data_bundle import decode, relative_path



def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def validate_pin(pin):
    require(isinstance(pin, dict) and type(pin.get("bytes")) is int and pin["bytes"] >= 0
            and isinstance(pin.get("sha256"), str)
            and re.fullmatch(r"[a-f0-9]{64}", pin["sha256"]), "Invalid input pin")


def pinned(raw, pin):
    validate_pin(pin)
    require(len(raw) == pin["bytes"] and sha(raw) == pin["sha256"], "Input bytes/SHA256 mismatch")
    return raw


def regular(path):
    path = Path(path).absolute()
    require(all(not parent.is_symlink() for parent in (path, *path.parents)), "Symlink input/output path")
    require(path.is_file(), f"Not a regular input: {path}")
    return path.read_bytes()


def write(path, raw):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as handle:
        handle.write(raw)


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n").encode()


def git_address(source, name):
    require(re.fullmatch(r"[A-Za-z0-9_-][A-Za-z0-9_.-]*/[A-Za-z0-9_-][A-Za-z0-9_.-]*",
                         source["repository"]), "Invalid repository")
    require(re.fullmatch(r"[a-f0-9]{40}", source["revision"]), "Full immutable Git revision required")
    relative_path(name)
    return (f'https://github.com/{source["repository"]}.git', source["revision"], name)


class GitInputs:
    """Fetch only fixed shallow commits and requested blobs, without a checkout."""

    def __init__(self, root, timeout=None):
        require(timeout is None or (type(timeout) in (int, float) and math.isfinite(timeout) and timeout > 0),
                "Invalid Git input timeout")
        self.root = root
        self.repositories = {}
        self.timeout = timeout

    def __call__(self, address, *, max_bytes=None):
        require(max_bytes is None or (type(max_bytes) is int and max_bytes > 0), "Invalid Git blob size limit")
        url, revision, name = address
        key = (url, revision)
        env = {**os.environ, "GIT_TERMINAL_PROMPT": "0"}
        if key not in self.repositories:
            root = self.root / str(len(self.repositories))
            root.mkdir(parents=True)
            def git(*args):
                subprocess.run(["git", "-C", str(root), *args], env=env, check=True,
                               stdout=subprocess.DEVNULL, timeout=self.timeout)
            git("init", "--bare", "--quiet")
            git("remote", "add", "origin", url)
            git("config", "remote.origin.promisor", "true")
            git("config", "remote.origin.partialclonefilter", "blob:none")
            git("fetch", "--quiet", "--no-tags", "--depth=1", "--filter=blob:none", "origin", revision)
            actual = subprocess.check_output(["git", "-C", str(root), "rev-parse", "FETCH_HEAD"],
                                             env=env, timeout=self.timeout)
            require(actual.decode().strip() == revision, "Git revision mismatch")
            self.repositories[key] = root
        if max_bytes is not None:
            size = subprocess.check_output(["git", "-C", str(self.repositories[key]), "cat-file", "-s",
                                            f"{revision}:{name}"], env=env, timeout=self.timeout)
            require(int(size) <= max_bytes, "Git metadata blob exceeds size limit")
        return subprocess.check_output(["git", "-C", str(self.repositories[key]), "cat-file", "blob",
                                        f"{revision}:{name}"], env=env, timeout=self.timeout)


def acquire(source, name, pin, cache, offline, fetcher):
    address = git_address(source, name)
    validate_pin(pin)
    path = cache / pin["sha256"]
    if path.exists() or path.is_symlink():
        return pinned(regular(path), pin)
    require(not offline, f"Offline cache miss: {name}")
    raw = pinned(fetcher(address), pin)
    # Verify the complete blob before caching. Any interrupted cache write fails
    # its pin on reuse. Declared size is checked without an arbitrary byte cap.
    write(path, raw)
    return raw


def unpack(archive, specification, destination):
    with tempfile.TemporaryFile() as handle:
        handle.write(archive)
        handle.seek(0)
        with zipfile.ZipFile(handle) as package:
            entries = package.infolist()
            names = [entry.filename for entry in entries]
            require(len(names) == len(set(names)) and "manifest.json" in names, "Duplicate/missing archive member")
            for entry in entries:
                relative_path(entry.filename)
                mode = entry.external_attr >> 16
                require(not entry.is_dir() and not stat.S_ISLNK(mode)
                        and stat.S_IFMT(mode) in (0, stat.S_IFREG), "Non-regular archive member")
            require(sum(entry.file_size for entry in entries) == specification["expanded_bytes"],
                    "Expanded archive size mismatch")
            raw = pinned(package.read("manifest.json"), specification["manifest"])
            manifest = decode(raw)
            require(set(names) == {"manifest.json", *manifest["files"]}, "Unlisted/missing archive file")
            for name, pin in manifest["files"].items():
                write(destination / relative_path(name), pinned(package.read(name), pin))
            write(destination / "manifest.json", raw)
    return manifest
