"""Pinned owner-data transports shared by every import.

Two transports, one rule: verify bytes/SHA-256 before use and cache by content.
- GitHub Release assets (`repository` + `tag` + `asset`): new publications.
- Git blobs at an exact commit (`repository` + `revision` + `path`): the
  already-locked `handoff/*` artifacts.
Downloads are build inputs, never player assets.
"""

import hashlib
import http.client
import json
import math
import os
from pathlib import Path
import re
import selectors
import shutil
import stat
import subprocess
import tempfile
import time
from typing import NamedTuple
from urllib.error import HTTPError
from urllib.parse import quote
import urllib.request
import zipfile

from pipeline.runtime.data_bundle import decode, relative_path


ROOT = Path(__file__).resolve().parents[2]
REPOSITORY = re.compile(r"[A-Za-z0-9_-][A-Za-z0-9_.-]*/[A-Za-z0-9_-][A-Za-z0-9_.-]*")
TAG = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*(?:/[A-Za-z0-9][A-Za-z0-9._-]*)*")
ASSET = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*")
# Metadata assets (SHA256SUMS, receipt.json) are small; never read them unbounded.
MAX_METADATA = 4 * 1024 * 1024


def download_cache():
    override = os.environ.get("POBR_DATA_CACHE")
    if override:
        return Path(override).expanduser()
    return ROOT / ".cache/data-import-downloads"


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


def replace_atomically(path, raw):
    """Replace a regular output without exposing partially written bytes."""
    path = Path(path).absolute()
    require(".." not in path.parts and all(not p.is_symlink() for p in (path, *path.parents)),
            "Unsafe atomic output path")
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(prefix=f".{path.name}.", dir=path.parent, delete=False) as handle:
        handle.write(raw)
        temporary = Path(handle.name)
    try:
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n").encode()


def valid_tag(tag):
    require(isinstance(tag, str) and TAG.fullmatch(tag) and ".." not in tag and not tag.endswith(".lock"),
            "Every external data artifact needs a safe owner tag")
    return tag


class ReleaseAsset(NamedTuple):
    repository: str
    tag: str
    asset: str


def release_address(source, asset=None):
    """The Release asset `source` pins, or another asset of the same Release."""
    require(isinstance(source.get("repository"), str) and REPOSITORY.fullmatch(source["repository"]),
            "Invalid repository")
    asset = source.get("asset") if asset is None else asset
    require(isinstance(asset, str) and ASSET.fullmatch(asset), "Invalid release asset name")
    return ReleaseAsset(source["repository"], valid_tag(source.get("tag")), asset)


def source_address(source, name=None):
    """Git entries carry a `revision` and a path; every other entry is a Release asset."""
    if "revision" in source:
        return git_address(source, name)
    require("path" not in source, "A Release asset has no Git revision/path")
    return release_address(source)


def git_address(source, name):
    require(isinstance(source.get("repository"), str) and REPOSITORY.fullmatch(source["repository"]),
            "Invalid repository")
    require(re.fullmatch(r"[a-f0-9]{40}", source["revision"]), "Full immutable Git revision required")
    relative_path(name)
    return (f'https://github.com/{source["repository"]}.git', source["revision"], name)


class GitInputs:
    """Fetch only fixed shallow commits and requested blobs, without a checkout."""

    def __init__(self, root, timeout=120, attempts=3, backoff=1.0):
        require(timeout is None or (type(timeout) in (int, float) and math.isfinite(timeout) and timeout > 0),
                "Invalid Git input timeout")
        require(type(attempts) is int and attempts > 0, "Invalid Git attempt count")
        self.root = root
        self.repositories = {}
        self.timeout = timeout
        self.attempts = attempts
        self.backoff = backoff

    def _run_with_retry(self, run, description):
        for attempt in range(1, self.attempts + 1):
            try:
                return run()
            except (subprocess.TimeoutExpired, subprocess.CalledProcessError) as error:
                if attempt == self.attempts:
                    raise ValueError(f"Git {description} failed after {attempt} attempts: {error}") from error
                time.sleep(self.backoff * 2 ** (attempt - 1))

    def __call__(self, address, *, max_bytes=None):
        require(max_bytes is None or (type(max_bytes) is int and max_bytes > 0), "Invalid Git blob size limit")
        url, revision, name = address
        key = (url, revision)
        env = {**os.environ, "GIT_TERMINAL_PROMPT": "0"}
        if key not in self.repositories:
            root = self.root / str(len(self.repositories))
            def fetch_repo():
                if root.exists():
                    shutil.rmtree(root)
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
            self._run_with_retry(fetch_repo, f"fetch {url}@{revision}")
            self.repositories[key] = root
        if max_bytes is not None:
            size = self._run_with_retry(
                lambda: subprocess.check_output(["git", "-C", str(self.repositories[key]), "cat-file", "-s",
                                                 f"{revision}:{name}"], env=env, timeout=self.timeout),
                f"stat {revision}:{name}")
            require(int(size) <= max_bytes, "Git metadata blob exceeds size limit")
        return self._run_with_retry(
            lambda: subprocess.check_output(["git", "-C", str(self.repositories[key]), "cat-file", "blob",
                                             f"{revision}:{name}"], env=env, timeout=self.timeout),
            f"read {revision}:{name}")


class ReleaseInputs:
    """Download GitHub Release assets.

    Like the Git transport, the default is anonymous HTTPS against public owner
    repositories. `POBR_RELEASE_TRANSPORT=gh` uses `gh release download` and its
    login instead (a private owner repository). `POBR_RELEASE_BASE_URL` replaces
    `https://github.com` (a mirror, or a local fixture server in tests).
    """

    def __init__(self, base=None, transport=None, timeout=120, attempts=3, backoff=1.0):
        self.base = (base or os.environ.get("POBR_RELEASE_BASE_URL") or "https://github.com").rstrip("/")
        self.transport = transport or os.environ.get("POBR_RELEASE_TRANSPORT") or "https"
        require(self.transport in ("https", "gh"), "POBR_RELEASE_TRANSPORT must be https or gh")
        require(type(attempts) is int and attempts > 0, "Invalid download attempt count")
        self.timeout, self.attempts, self.backoff = timeout, attempts, backoff

    def url(self, address):
        return (f"{self.base}/{address.repository}/releases/download/"
                f"{quote(address.tag, safe='/')}/{quote(address.asset)}")

    def __call__(self, address, *, max_bytes=None):
        require(isinstance(address, ReleaseAsset), "Release transport needs a Release asset address")
        require(max_bytes is None or (type(max_bytes) is int and max_bytes > 0), "Invalid download size limit")
        runner = self.download_gh if self.transport == "gh" else self.download
        for attempt in range(1, self.attempts + 1):
            try:
                return runner(address, max_bytes)
            except TransientDownload as error:
                if attempt == self.attempts:
                    raise ValueError(f"Cannot download {address.repository} {address.tag} {address.asset} "
                                     f"after {attempt} attempts: {error}") from error
                time.sleep(self.backoff * 2 ** (attempt - 1))

    def download_gh(self, address, max_bytes):
        cmd = ["gh", "release", "download", address.tag, "--repo", address.repository,
               "--pattern", address.asset, "--output", "-"]
        env = {**os.environ, "GH_PROMPT_DISABLED": "1"}
        proc = subprocess.Popen(cmd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        deadline = None if self.timeout is None else time.monotonic() + self.timeout
        chunks = []
        stderr_chunks = []
        total = 0
        oversized = False
        try:
            with selectors.DefaultSelector() as sel:
                sel.register(proc.stdout, selectors.EVENT_READ, "stdout")
                sel.register(proc.stderr, selectors.EVENT_READ, "stderr")
                while sel.get_map():
                    remaining = None if deadline is None else max(0.0, deadline - time.monotonic())
                    if remaining == 0.0:
                        proc.kill()
                        proc.wait()
                        raise TransientDownload(f"gh timed out after {self.timeout}s")
                    events = sel.select(remaining)
                    if not events and deadline is not None and time.monotonic() >= deadline:
                        proc.kill()
                        proc.wait()
                        raise TransientDownload(f"gh timed out after {self.timeout}s")
                    for key, _ in events:
                        chunk = os.read(key.fd, 65536)
                        if not chunk:
                            sel.unregister(key.fileobj)
                            continue
                        if key.data == "stdout":
                            total += len(chunk)
                            if max_bytes is not None and total > max_bytes:
                                oversized = True
                                proc.kill()
                                sel.unregister(proc.stdout)
                                break
                            chunks.append(chunk)
                        else:
                            if sum(len(part) for part in stderr_chunks) < MAX_METADATA:
                                stderr_chunks.append(chunk)
                    if oversized:
                        break
            proc.wait(timeout=5)
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
            proc.stdout.close()
            proc.stderr.close()
        if oversized:
            raise ValueError(f"Release asset exceeds size limit: {address.asset}")
        if proc.returncode != 0:
            err = b"".join(stderr_chunks).decode("utf-8", errors="replace").strip()
            lowered = err.lower()
            permanent = any(marker in lowered for marker in (
                "not found", "404", "401", "403", "unauthorized", "forbidden",
                "no assets match", "not logged into", "authentication"))
            if permanent:
                raise ValueError(f"Cannot download {address.repository} {address.tag} {address.asset}: {err}")
            raise TransientDownload(err or f"gh exited with {proc.returncode}")
        return b"".join(chunks)

    def download(self, address, max_bytes):
        request = urllib.request.Request(self.url(address), headers={"User-Agent": "pobr-data-import"})
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                declared = response.headers.get("Content-Length")
                raw = response.read() if max_bytes is None else response.read(max_bytes + 1)
        except HTTPError as error:
            error.close()
            if error.code == 429 or error.code >= 500:
                raise TransientDownload(error) from error
            # 404 and other client errors are answers, not outages: never retried.
            raise ValueError(f"Cannot download {address.repository} {address.tag} {address.asset}: {error}") from error
        except (OSError, http.client.HTTPException) as error:  # URLError, reset, timeout, IncompleteRead
            raise TransientDownload(error) from error
        require(max_bytes is None or len(raw) <= max_bytes, f"Release asset exceeds size limit: {address.asset}")
        if declared is not None and int(declared) != len(raw):
            raise TransientDownload(f"truncated: {len(raw)} of {declared} bytes")
        return raw


class TransientDownload(Exception):
    """A download failure worth retrying (network, 5xx/429, truncated body)."""


class OwnerInputs:
    """Dispatch each address to its transport; one instance per prepare."""

    def __init__(self, root, releases=None):
        self.git = GitInputs(root)
        self.releases = releases or ReleaseInputs()

    def __call__(self, address, **options):
        transport = self.releases if isinstance(address, ReleaseAsset) else self.git
        return transport(address, **options)


def parse_sums(raw):
    """GNU `sha256sum` output: `<sha256>  <name>` (or ` *<name>` for binary mode)."""
    require(len(raw) <= MAX_METADATA, "Oversized SHA256SUMS")
    sums = {}
    for line in raw.decode("utf-8").splitlines():
        if not line.strip():
            continue
        found = re.fullmatch(r"([a-f0-9]{64}) [ *](.+)", line)
        require(found is not None, f"Invalid SHA256SUMS line: {line!r}")
        digest, name = found.groups()
        require(ASSET.fullmatch(name) and name not in sums, f"Invalid or duplicate SHA256SUMS entry: {name!r}")
        sums[name] = digest
    require(sums, "Empty SHA256SUMS")
    return sums


def acquire(source, name, pin, cache, offline, fetcher):
    address = source_address(source, name)
    validate_pin(pin)
    path = cache / pin["sha256"]
    if path.exists() or path.is_symlink():
        return pinned(regular(path), pin)
    require(not offline, f"Offline cache miss: {name}")
    # A Release download is bounded by its pin; Git blobs are read whole by the Git transport.
    raw = pinned(fetcher(address, max_bytes=pin["bytes"]) if isinstance(address, ReleaseAsset) else fetcher(address),
                 pin)
    # Verify the complete blob before caching, then install atomically so concurrent
    # checkouts or interrupted writes never leave a corrupt cache entry.
    replace_atomically(path, raw)
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
            require(isinstance(manifest, dict) and isinstance(manifest.get("files"), dict),
                    "Invalid archive file inventory")
            require(set(names) == {"manifest.json", *manifest["files"]}, "Unlisted/missing archive file")
            for name, pin in manifest["files"].items():
                write(destination / relative_path(name), pinned(package.read(name), pin))
            write(destination / "manifest.json", raw)
    return manifest
