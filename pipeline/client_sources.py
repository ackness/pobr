"""Acquire only explicitly locked official-client resources; never discover a patch.

Bundle framing is implemented here independently. Native ooz decompression runs
in an explicitly selected, fingerprinted external Python environment, not PoBR.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import struct
import subprocess
import tempfile
import tomllib
import urllib.request
from urllib.parse import urlsplit


MIB = 1024 * 1024
MAX_NETWORK = 64 * MIB
MAX_RANGE = 4 * MIB
MAX_RESOURCE = 16 * MIB
MAX_BUNDLE_RAW = 512 * MIB
MAX_BUNDLE_COMPRESSED = 256 * MIB
MAX_CHUNKS = 4096
MAX_CHUNK_RAW = MIB
MAX_CHUNK_COMPRESSED = 2 * MIB
MAX_RECORDS = 4096
RECEIPT = "client-sources.json"
OFFICIAL_HOST = "patch-poe2.poecdn.com"


def sha256(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False) + "\n").encode()


def decode(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"Duplicate JSON key: {key}")
            result[key] = value
        return result

    def invalid(value):
        raise ValueError(f"Non-JSON number: {value}")

    return json.loads(raw, object_pairs_hook=unique, parse_constant=invalid)


def integer(value, low, high, label):
    if type(value) is not int or not low <= value <= high:
        raise ValueError(f"Invalid {label}: expected integer in {low}..{high}")
    return value


def digest(value):
    if not isinstance(value, str) or not re.fullmatch(r"[a-f0-9]{64}", value):
        raise ValueError("Invalid SHA-256")
    return value


def relative(name):
    if (not isinstance(name, str) or not name or "\\" in name or ":" in name
            or any(part in {"", ".", ".."} for part in name.split("/"))):
        raise ValueError(f"Unsafe relative path: {name!r}")
    return Path(name)


def safe_path(path):
    path = Path(os.path.abspath(path))
    # Do not resolve away symlinks (including a dangling destination).
    for part in [*reversed(path.parents), path]:
        if part.is_symlink():
            raise ValueError(f"Symlinks are not allowed: {part}")
    return path


def read_file(path, cap):
    path = safe_path(path)
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > cap:
            raise ValueError(f"Input is not a bounded regular file: {path}")
        raw = stream.read(cap + 1)
    if len(raw) > cap:
        raise ValueError(f"Input exceeds cap: {path}")
    return raw


def write_new(path, raw):
    path = safe_path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    safe_path(path)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(raw)
    except BaseException:
        # Only unlink after our exclusive create succeeded. A failed receipt
        # write must not leave a completion marker; never unlink a prior file.
        path.unlink()
        raise


def official_url(url, base):
    if not isinstance(url, str):
        raise ValueError("Missing official source URL")
    parsed = urlsplit(url)
    if (parsed.scheme != "https" or parsed.netloc != OFFICIAL_HOST
            or parsed.query or parsed.fragment or not url.startswith(base)
            or not re.fullmatch(r"[A-Za-z0-9_./-]+", parsed.path)
            or any(p in {".", "..", ""} for p in parsed.path[1:].split("/"))):
        raise ValueError(f"URL is outside the fixed official source: {url}")
    return url


def load_source(path, key):
    raw = read_file(path, 2 * MIB)
    lock = decode(raw)
    if not isinstance(lock, dict) or type(lock.get("schema_version")) is not int or lock["schema_version"] != 1:
        raise ValueError("Unsupported source lock schema")
    source = lock["sources"][key]
    if (not isinstance(source, dict) or source.get("kind") != "official-client-resource-candidate"
            or source.get("game") != "poe2"):
        raise ValueError("Source is not a fixed PoE2 official-client resource candidate")
    patch = source.get("game_patch")
    if not isinstance(patch, str) or not re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", patch):
        raise ValueError("Source needs an explicit numeric patch")
    base = f"https://{OFFICIAL_HOST}/{patch}/"
    if source.get("base_url") != base:
        raise ValueError("Source base_url differs from its fixed patch")
    resources = source.get("resources")
    if not isinstance(resources, dict) or not 1 <= len(resources) <= 128:
        raise ValueError("Invalid selected resource count")
    total = 0
    names = set()
    for name, record in resources.items():
        if not isinstance(record, dict):
            raise ValueError("Resource pin must be an object")
        path = relative(name)
        if path.parts[0] not in {"Data", "Metadata"} or len(path.parts) < 2:
            raise ValueError("Expected a game-relative Data/ or Metadata/ path")
        if name.casefold() in names:
            raise ValueError("Case-colliding resource paths")
        names.add(name.casefold())
        digest(record["sha256"])
        size = integer(record["bytes"], 1, MAX_RESOURCE, "resource bytes")
        if integer(record["uncompressed_length"], 1, MAX_RESOURCE, "resource length") != size:
            raise ValueError("Locked resource length differs from bytes")
        integer(record["uncompressed_offset"], 0, MAX_BUNDLE_RAW - size, "resource offset")
        url = official_url(record["bundle_url"], base + "Bundles2/")
        if not url.endswith(".bundle.bin"):
            raise ValueError("Only fixed resource bundles are supported; no index discovery")
        total += size
    integer(total, 1, MAX_NETWORK, "selected raw byte total")
    return source, sha256(raw)


def response_metadata(url, start, length, status, final_url, headers):
    if status != 206:
        raise ValueError("Expected HTTP 206 for bounded Range request")
    if final_url != url:
        raise ValueError("Redirect/final URL differs from the exact locked source")
    normalized = {}
    for key, value in headers.items():
        key = key.lower()
        if key in normalized:
            raise ValueError(f"Duplicate response header: {key}")
        normalized[key] = value
    headers = normalized
    match = re.fullmatch(r"bytes ([0-9]{1,10})-([0-9]{1,10})/([0-9]{1,10})",
                         headers.get("content-range", ""))
    if not match:
        raise ValueError("Malformed Content-Range")
    first, last, total = map(int, match.groups())
    if (first != start or last != start + length - 1 or last >= total
            or not 1 <= total <= MAX_BUNDLE_COMPRESSED):
        raise ValueError("Content-Range differs from requested range or total bound")
    if headers.get("content-length") != str(length):
        raise ValueError("Content-Length differs from requested range")
    if headers.get("content-encoding", "identity").lower() != "identity":
        raise ValueError("Encoded HTTP range is unsupported")
    etag = headers.get("etag", "")
    if not re.fullmatch(r'"[\x21\x23-\x7e]{1,254}"', etag):
        raise ValueError("A strong ETag is required")
    return total, etag


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError("Redirects are forbidden for fixed official source ranges")


class RangeCache:
    """Content-addressed bodies and immutable URL/range/validator records."""

    def __init__(self, root, source):
        self.root = safe_path(root)
        self.urls = {r["bundle_url"] for r in source["resources"].values()}
        self.loaded = {}
        self.used = {}
        self.checked_bytes = 0

    def body(self, record):
        raw = read_file(self.root / "blobs" / (record["sha256"] + ".bin"), MAX_RANGE)
        if len(raw) != record["length"] or sha256(raw) != record["sha256"]:
            raise ValueError("Tampered cache body: size/hash mismatch")
        return raw

    def records(self, url):
        if url not in self.urls:
            raise ValueError("URL is not selected by the source lock")
        if url in self.loaded:
            return self.loaded[url]
        directory = safe_path(self.root / "ranges" / sha256(url.encode()))
        records = []
        if directory.exists():
            paths = []
            for path in directory.iterdir():
                paths.append(path)
                if len(paths) > MAX_RECORDS:
                    raise ValueError("Cache record count exceeds cap")
            paths.sort()
            for path in paths:
                r = decode(read_file(path, 8192))
                if (r.get("schema") != "pobr-client-range/v1" or r.get("url") != url
                        or r.get("final_url") != url):
                    raise ValueError("Cache source metadata mismatch")
                start = integer(r["start"], 0, MAX_BUNDLE_COMPRESSED, "cache offset")
                length = integer(r["length"], 1, MAX_RANGE, "cache length")
                total = integer(r["total"], start + length, MAX_BUNDLE_COMPRESSED, "cache total")
                response_metadata(url, start, length, 206, r["final_url"], {
                    "Content-Range": f"bytes {start}-{start + length - 1}/{total}",
                    "Content-Length": str(length), "ETag": r["etag"],
                })
                digest(r["sha256"])
                if path.name != f"{start}-{start + length - 1}.json":
                    raise ValueError("Cache range filename mismatch")
                self.checked_bytes += length
                if self.checked_bytes > MAX_NETWORK:
                    raise ValueError("Selected cache bytes exceed cap")
                self.check_identity(records, r)
                # Verify every body, including overlaps, rather than trust local metadata.
                self.check_overlap(records, r, self.body(r))
                records.append(r)
        self.loaded[url] = records
        return records

    @staticmethod
    def check_identity(records, record):
        if any((r["total"], r["etag"]) != (record["total"], record["etag"]) for r in records):
            raise ValueError("Source total/ETag drift")

    def check_overlap(self, records, record, raw):
        a, b = record["start"], record["start"] + record["length"]
        for old in records:
            c, d = old["start"], old["start"] + old["length"]
            lo, hi = max(a, c), min(b, d)
            if lo < hi and raw[lo - a:hi - a] != self.body(old)[lo - c:hi - c]:
                raise ValueError("Conflicting overlapping cache ranges")

    def info(self, url):
        records = self.records(url)
        return (records[0]["total"], records[0]["etag"]) if records else None

    def add(self, url, start, raw, total, etag, provenance):
        records = self.records(url)
        integer(len(raw), 1, MAX_RANGE, "range body bytes")
        record = {"schema": "pobr-client-range/v1", "url": url, "final_url": url,
                  "start": start, "length": len(raw), "total": total, "etag": etag,
                  "sha256": sha256(raw), "provenance": provenance}
        response_metadata(url, start, len(raw), 206, url, {
            "Content-Range": f"bytes {start}-{start + len(raw) - 1}/{total}",
            "Content-Length": str(len(raw)), "ETag": etag,
        })
        self.check_identity(records, record)
        self.check_overlap(records, record, raw)
        for old in records:
            if (old["start"], old["length"]) == (start, len(raw)):
                return old
        if len(records) >= MAX_RECORDS or self.checked_bytes + len(raw) > MAX_NETWORK:
            raise ValueError("Selected cache exceeds range/byte cap")
        blob = self.root / "blobs" / (record["sha256"] + ".bin")
        if blob.exists():
            if read_file(blob, MAX_RANGE) != raw:
                raise ValueError("Tampered cache blob")
        else:
            write_new(blob, raw)
        write_new(self.root / "ranges" / sha256(url.encode()) / f"{start}-{start + len(raw) - 1}.json",
                  encoded(record))
        records.append(record)
        self.checked_bytes += len(raw)
        return record

    def read(self, url, start, length):
        records = self.records(url)
        position, end, result = start, start + length, bytearray()
        used = []
        while position < end:
            candidates = [r for r in records if r["start"] <= position < r["start"] + r["length"]]
            if not candidates:
                return None
            r = max(candidates, key=lambda r: (r["start"] + r["length"], -r["start"]))
            stop = min(end, r["start"] + r["length"])
            raw = self.body(r)
            result.extend(raw[position - r["start"]:stop - r["start"]])
            used.append(r)
            position = stop
        for r in used:
            self.used[(url, r["start"], r["length"])] = r
        return bytes(result)


class Ranges:
    def __init__(self, cache, mode, budget=0, request_cap=MAX_RANGE, opener=None):
        if mode not in {"offline", "online"}:
            raise ValueError("Explicit offline/online mode required")
        integer(request_cap, 61, MAX_RANGE, "per-request cap")
        integer(budget, 1 if mode == "online" else 0, MAX_NETWORK, "network budget")
        if mode == "offline" and budget:
            raise ValueError("Offline mode cannot have a network budget")
        self.cache, self.mode, self.budget, self.request_cap = cache, mode, budget, request_cap
        self.opener = opener  # Offline mode never even constructs a network opener.
        self.reserved = self.downloaded = self.requests = 0

    def read(self, url, start, length):
        integer(start, 0, MAX_BUNDLE_COMPRESSED, "range start")
        integer(length, 1, self.request_cap - 1, "range length (including overflow guard)")
        if start + length > MAX_BUNDLE_COMPRESSED:
            raise ValueError("Range exceeds bundle cap")
        cached = self.cache.read(url, start, length)
        if cached is not None:
            return cached
        if self.mode == "offline":
            raise ValueError(f"Offline cache miss: {url} bytes={start}-{start + length - 1}")
        # Reserve a whole bounded body plus a byte to detect an oversized response.
        # Failures also consume reservations; retries cannot reset this run's budget.
        if self.reserved + length + 1 > self.budget:
            raise ValueError("Cumulative network byte budget exhausted")
        self.reserved += length + 1
        self.requests += 1
        if self.opener is None:
            self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
        info = self.cache.info(url)
        headers = {"Range": f"bytes={start}-{start + length - 1}", "Accept-Encoding": "identity",
                   "User-Agent": "PoBR-fixed-client-source/1"}
        if info:
            headers["If-Match"] = info[1]
        request = urllib.request.Request(url, headers=headers)
        with self.opener.open(request, timeout=25) as response:
            total, etag = response_metadata(url, start, length, response.status,
                                             response.geturl(), response.headers)
            if info and (total, etag) != info:
                raise ValueError("Source total/ETag drift")
            raw = response.read(length + 1)
            self.downloaded += len(raw)
            if len(raw) != length:
                raise ValueError("HTTP body length differs from requested range")
        self.cache.add(url, start, raw, total, etag, {"kind": "http-range"})
        return self.cache.read(url, start, length)


# This is our boundary protocol, not copied decoder code. -I -S excludes CWD,
# PYTHONPATH, user site and .pth startup code; only the explicit env's platlib is
# added. No RePoE/PyPoE module or candidate resource is imported.
DECODER_WORKER = r'''
import base64, csv, hashlib, importlib.metadata, importlib.util, importlib.machinery
import io, json, pathlib, resource, sys, sysconfig
# Darwin rejects RLIMIT_AS/DATA/RSS. Report that limit honestly; the native API
# still receives at most 1 MiB output size, and CPU/file/time bounds apply.
address_space_limit = None if sys.platform == "darwin" else 256 * 1024 * 1024
if address_space_limit is not None:
    resource.setrlimit(resource.RLIMIT_AS, (address_space_limit, address_space_limit))
resource.setrlimit(resource.RLIMIT_CPU, (10, 10))
resource.setrlimit(resource.RLIMIT_FSIZE, (2 * 1024 * 1024, 2 * 1024 * 1024))
site = pathlib.Path(sysconfig.get_path("platlib"))
sys.path.insert(0, str(site))
dists = [d for d in importlib.metadata.distributions(path=[str(site)])
         if d.metadata["Name"].lower() == "pyooz"]
if len(dists) != 1: raise ValueError("Expected one installed pyooz distribution")
d = dists[0]
spec = importlib.util.find_spec("ooz")
if not spec or not isinstance(spec.loader, importlib.machinery.ExtensionFileLoader):
    raise ValueError("ooz must be the installed native extension")
p = pathlib.Path(spec.origin)
if p.parent != site: raise ValueError("ooz module outside explicit environment")
def read(path):
    if any(x.is_symlink() for x in [path, *path.parents]): raise ValueError("Symlink in decoder files")
    if path.stat().st_size > 16 * 1024 * 1024: raise ValueError("Decoder file too large")
    return path.read_bytes()
def sha(raw): return hashlib.sha256(raw).hexdigest()
module = read(p)
record = d.read_text("RECORD")
rows = [r for r in csv.reader(io.StringIO(record)) if r[0] == p.name]
expected = "sha256=" + base64.urlsafe_b64encode(hashlib.sha256(module).digest()).decode().rstrip("=")
if len(rows) != 1 or rows[0][1:] != [expected, str(len(module))]:
    raise ValueError("ooz binary does not match installed distribution RECORD")
fingerprints = {}
for name in ("METADATA", "WHEEL", "RECORD"):
    paths = [d.locate_file(f) for f in d.files if str(f).endswith(".dist-info/" + name)]
    if len(paths) != 1: raise ValueError("Missing decoder distribution metadata")
    fingerprints[name] = sha(read(pathlib.Path(paths[0])))
identity = {"python": sys.version, "executable": sys.executable,
            "distribution": "pyooz", "version": d.version, "module_path": str(p),
            "module_sha256": sha(module), "distribution_files": fingerprints,
            "limits": {"address_space_bytes": address_space_limit, "cpu_seconds": 10,
                       "output_file_bytes": 2 * 1024 * 1024, "chunk_output_bytes": 1024 * 1024}}
if len(sys.argv) == 1:
    sys.stdout.write(json.dumps(identity, sort_keys=True))
else:
    if identity != json.loads(sys.argv[1]): raise ValueError("Decoder identity changed")
    size = int(sys.argv[2])
    if not 1 <= size <= 1024 * 1024: raise ValueError("Decoder output cap")
    data = sys.stdin.buffer.read(2 * 1024 * 1024 + 1)
    if not 1 <= len(data) <= 2 * 1024 * 1024: raise ValueError("Decoder input cap")
    import ooz
    result = ooz.decompress(data, size)
    if len(result) != size: raise ValueError("Decoder result length mismatch")
    sys.stdout.buffer.write(result)
'''


class ExternalDecoder:
    def __init__(self, python, lock_path, source):
        self.python = str(Path(python).absolute())
        if not Path(python).is_absolute() or not Path(python).is_file():
            raise ValueError("Decoder Python must be an explicit absolute executable path")
        self.lock_path = safe_path(lock_path)
        self.lock_bytes = read_file(self.lock_path, 2 * MIB)
        lock = tomllib.loads(self.lock_bytes.decode())
        packages = [p for p in lock["package"] if p["name"] == "pyooz"]
        pypoe = [p for p in lock["package"] if p["name"] == "pypoe"]
        if len(packages) != 1 or len(pypoe) != 1:
            raise ValueError("Decoder uv.lock must pin pyooz and PyPoE")
        revision = source["decoder"]["revision"]
        git_source = pypoe[0].get("source", {}).get("git", "")
        repository = source["decoder"].get("repository", "https://github.com/repoe-fork/pypoe")
        if (git_source.split("#")[-1] != revision
                or git_source.split("?")[0].split("#")[0].removesuffix(".git") != repository.removesuffix(".git")):
            raise ValueError("Decoder uv.lock PyPoE revision differs from source lock")
        self.identity = decode(self.run([], b"", 65536))
        if self.identity["version"] != packages[0]["version"]:
            raise ValueError("Installed pyooz version differs from decoder uv.lock")
        self.receipt = {**self.identity, "uv_lock_path": str(self.lock_path),
                        "uv_lock_sha256": sha256(self.lock_bytes),
                        "uv_pyooz_package_sha256": sha256(encoded(packages[0])),
                        "uv_pypoe_revision": revision,
                        "identity_sha256": sha256(encoded(self.identity)),
                        "verification": "installed module matches RECORD; version matches uv.lock; "
                                        "local fingerprints, not wheel provenance or schema certification"}

    def run(self, args, raw, cap):
        # File-backed stdout/stderr plus worker rlimits avoid unbounded PIPE capture.
        with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
            result = subprocess.run([self.python, "-I", "-S", "-B", "-c", DECODER_WORKER, *args],
                                    input=raw, stdout=output, stderr=errors, timeout=25, check=False,
                                    cwd="/", env={"PATH": "/usr/bin:/bin"})
            if result.returncode:
                errors.seek(0)
                raise ValueError("External decoder failed: " + errors.read(4096).decode(errors="replace"))
            if output.tell() > cap:
                raise ValueError("External decoder exceeded output cap")
            output.seek(0)
            return output.read(cap + 1)

    def decompress(self, raw, size):
        integer(len(raw), 1, MAX_CHUNK_COMPRESSED, "compressed chunk size")
        integer(size, 1, MAX_CHUNK_RAW, "decompressed chunk size")
        if read_file(self.lock_path, 2 * MIB) != self.lock_bytes:
            raise ValueError("Decoder uv.lock changed")
        result = self.run([json.dumps(self.identity, sort_keys=True), str(size)], raw, size)
        if len(result) != size:
            raise ValueError("External decoder result length mismatch")
        return result


class Bundle:
    def __init__(self, url, ranges, decoder):
        self.url, self.ranges, self.decoder = url, ranges, decoder
        header = ranges.read(url, 0, 60)
        (raw32, compressed32, header_size, encoder, flag, raw64, compressed64,
         count, chunk_size, *reserved) = struct.unpack("<IIIIIQQIIIIII", header)
        integer(raw64, 1, MAX_BUNDLE_RAW, "bundle raw size")
        integer(compressed64, 1, MAX_BUNDLE_COMPRESSED - 60, "bundle compressed size")
        integer(count, 1, MAX_CHUNKS, "bundle chunk count")
        integer(chunk_size, 1, MAX_CHUNK_RAW, "bundle chunk size")
        if (raw32 != raw64 or compressed32 != compressed64 or header_size != 48 + 4 * count
                or encoder not in {9, 12} or flag != 1 or any(reserved)
                or count != (raw64 + chunk_size - 1) // chunk_size):
            raise ValueError("Malformed or unsupported bundle framing")
        if 60 + 4 * count + compressed64 != ranges.cache.info(url)[0]:
            raise ValueError("Bundle framing total differs from HTTP total")
        table = ranges.read(url, 60, 4 * count)
        sizes = struct.unpack(f"<{count}I", table)
        offsets = [60 + 4 * count]
        for size in sizes:
            integer(size, 1, MAX_CHUNK_COMPRESSED, "compressed chunk size")
            offsets.append(offsets[-1] + size)
        if sum(sizes) != compressed64:
            raise ValueError("Bundle chunk table compressed size mismatch")
        self.size, self.chunk_size, self.sizes, self.offsets = raw64, chunk_size, sizes, offsets

    def read(self, start, length):
        integer(length, 1, MAX_RESOURCE, "resource length")
        integer(start, 0, self.size - length, "resource offset")
        first, last = start // self.chunk_size, (start + length - 1) // self.chunk_size
        result = bytearray()
        i = first
        while i <= last:
            end = i + 1
            while end <= last and self.offsets[end + 1] - self.offsets[i] < self.ranges.request_cap:
                end += 1
            compressed = self.ranges.read(self.url, self.offsets[i], self.offsets[end] - self.offsets[i])
            position = 0
            for index in range(i, end):
                size = self.sizes[index]
                expected = min(self.chunk_size, self.size - index * self.chunk_size)
                raw = self.decoder.decompress(compressed[position:position + size], expected)
                if len(raw) != expected:
                    raise ValueError("Decoder result length mismatch")
                lo = max(start, index * self.chunk_size) - index * self.chunk_size
                hi = min(start + length, (index + 1) * self.chunk_size) - index * self.chunk_size
                result.extend(raw[lo:hi])
                position += size
            i = end
        return bytes(result)


def import_archive(source_lock, source_key, cache_root, archive_root, ledger_path):
    source, lock_hash = load_source(source_lock, source_key)
    archive_root = safe_path(archive_root)
    ledger = read_file(ledger_path, 2 * MIB)
    cache = RangeCache(cache_root, source)
    selected, total_bytes = [], 0
    entries = ledger.splitlines()
    if len(entries) > MAX_RECORDS:
        raise ValueError("Archive ledger count exceeds cap")
    for line in entries:
        entry = decode(line)
        url = entry.get("url")
        if url not in cache.urls:
            continue
        headers = {k.lower(): v for k, v in entry["headers"].items()}
        match = re.fullmatch(r"bytes ([0-9]{1,10})-([0-9]{1,10})/([0-9]{1,10})",
                             headers.get("content-range", ""))
        if not match or entry.get("method") != "GET" or "error" in entry:
            raise ValueError("Invalid selected archive range entry")
        start, end, _ = map(int, match.groups())
        length = integer(end - start + 1, 1, MAX_RANGE, "archive range bytes")
        total, etag = response_metadata(url, start, length, entry["status"], entry["final_url"], headers)
        raw = read_file(archive_root / relative(entry["path"]), MAX_RANGE)
        if (entry["downloaded_bytes"] != length or len(raw) != length
                or sha256(raw) != digest(entry["sha256"])):
            raise ValueError("Archive body size/hash mismatch")
        total_bytes += length
        integer(total_bytes, 1, MAX_NETWORK, "archive import bytes")
        selected.append((url, start, raw, total, etag, {
            "kind": "archive-import", "ledger_sha256": sha256(ledger),
            "entry_sha256": sha256(line), "archive_path": entry["path"],
        }))
    if not selected:
        raise ValueError("Archive has no ranges for selected source")
    for args in selected:
        cache.add(*args)
    receipt = {"schema": "pobr-client-cache-import/v1", "source": source_key,
               "source_lock_sha256": lock_hash, "archive_ledger_sha256": sha256(ledger),
               "ranges": len(selected), "body_bytes": total_bytes, "network_requests": 0}
    name = cache.root / "imports" / (sha256(encoded(receipt)) + ".json")
    if name.exists():
        if read_file(name, MIB) != encoded(receipt):
            raise ValueError("Import receipt mismatch")
    else:
        write_new(name, encoded(receipt))
    return receipt


def acquire(source_lock, source_key, cache_root, output, python, decoder_lock, mode,
            budget=0, request_cap=MAX_RANGE):
    output = safe_path(output)
    if output.exists():
        raise FileExistsError(f"Refusing to replace existing output: {output}")
    # Keep candidates away from active production data and all orchestration inputs.
    published = Path(__file__).resolve().parent.parent / "data"
    inputs = [published, safe_path(cache_root), safe_path(source_lock), safe_path(decoder_lock)]
    if any(output == p or output.is_relative_to(p) or p.is_relative_to(output) for p in inputs):
        raise ValueError("Output must be separate from data/, cache and lock inputs")
    source, lock_hash = load_source(source_lock, source_key)
    ranges = Ranges(RangeCache(cache_root, source), mode, budget, request_cap)
    decoder = ExternalDecoder(python, decoder_lock, source)
    bundles, resources = {}, {}
    # Atomic mkdir claims the new raw root. Each resource is written directly to
    # its RePoE game-relative path only after matching the selected source lock.
    output.parent.mkdir(parents=True, exist_ok=True)
    safe_path(output)
    output.mkdir()
    for name, record in sorted(source["resources"].items()):
        url = record["bundle_url"]
        if url not in bundles:
            bundles[url] = Bundle(url, ranges, decoder)
        raw = bundles[url].read(record["uncompressed_offset"], record["bytes"])
        if len(raw) != record["bytes"] or sha256(raw) != record["sha256"]:
            raise ValueError(f"Locked raw hash/size mismatch: {name}")
        write_new(output / relative(name), raw)
        resources[name] = dict(record)
    # Recheck on-disk bytes and mutable inputs before writing completion last.
    for name, record in resources.items():
        raw = read_file(output / relative(name), MAX_RESOURCE)
        if len(raw) != record["bytes"] or sha256(raw) != record["sha256"]:
            raise ValueError(f"Output changed before completion: {name}")
    if sha256(read_file(source_lock, 2 * MIB)) != lock_hash:
        raise ValueError("Source lock changed before completion")
    if read_file(decoder.lock_path, 2 * MIB) != decoder.lock_bytes:
        raise ValueError("Decoder lock changed before completion")
    receipt = {
        "schema": "pobr-client-sources/v1", "source": source_key, "source_lock_sha256": lock_hash,
        "game": source["game"], "game_patch": source["game_patch"], "realm": source["realm"],
        "scope": source.get("scope"), "source_decoder_declaration": source["decoder"],
        "decoder": decoder.receipt, "mode": mode, "resources": resources,
        "network": {"requests": ranges.requests, "body_bytes": ranges.downloaded,
                    "reserved_bytes": ranges.reserved, "budget_bytes": budget,
                    "per_request_cap_bytes": request_cap},
        "ranges": sorted(ranges.cache.used.values(), key=lambda r: (r["url"], r["start"], r["length"])),
        "verification": "raw bytes match the selected trusted source lock; cache hashes and validators "
                        "provide replay integrity, not independent authenticity, schema or gameplay certification",
    }
    write_new(output / RECEIPT, encoded(receipt))
    return receipt


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    acquire_parser = commands.add_parser("acquire", help="Reconstruct every resource in one fixed source")
    import_parser = commands.add_parser("import-archive", help="Import recorded compressed HTTP ranges offline")
    for command in [acquire_parser, import_parser]:
        command.add_argument("--source-lock", required=True, type=Path)
        command.add_argument("--source", required=True)
        command.add_argument("--cache", required=True, type=Path)
    import_parser.add_argument("--archive-root", required=True, type=Path)
    import_parser.add_argument("--archive-ledger", required=True, type=Path)
    acquire_parser.add_argument("--out", required=True, type=Path, help="New raw root for RePoE.native_facts")
    acquire_parser.add_argument("--decoder-python", required=True, type=Path)
    acquire_parser.add_argument("--decoder-lock", required=True, type=Path, help="External environment's uv.lock")
    acquire_parser.add_argument("--mode", required=True, choices=["offline", "online"])
    acquire_parser.add_argument("--network-budget-bytes", type=int, default=0,
                                help="Required online; cumulative per run, at most 64 MiB")
    acquire_parser.add_argument("--max-request-bytes", type=int, default=MAX_RANGE)
    args = parser.parse_args(argv)
    try:
        if args.command == "import-archive":
            receipt = import_archive(args.source_lock, args.source, args.cache,
                                     args.archive_root, args.archive_ledger)
        else:
            receipt = acquire(args.source_lock, args.source, args.cache, args.out,
                              args.decoder_python, args.decoder_lock, args.mode,
                              args.network_budget_bytes, args.max_request_bytes)
        print(json.dumps({"status": "completed", "source": args.source,
                          "receipt_sha256": sha256(encoded(receipt)),
                          "receipt": str(args.out / RECEIPT) if args.command == "acquire" else receipt},
                         sort_keys=True))
    except (ValueError, OSError, KeyError, TypeError, struct.error, subprocess.SubprocessError) as error:
        parser.exit(1, f"client_sources: {error}\n")


if __name__ == "__main__":
    main()
