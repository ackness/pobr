"""Locate explicit paths in a fixed official index, producing an unreviewed selection."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess

import client_source_draft as draft

client = draft.client
REQUEST = "pobr-client-index-request/v1"
RECEIPT = "pobr-client-index-receipt/v1"
STATUS = "unreviewed-index-locations"
MASK = (1 << 64) - 1
M = 0xC6A4A7935BD1E995
MAX_BUNDLES = 100_000
MAX_FILES = 8_000_000


def hash64(raw, seed):
    """Little-endian MurmurHash64A; Austin Appleby's public-domain algorithm.

    Reference: https://github.com/aappleby/smhasher/blob/master/src/MurmurHash2.cpp
    This implementation does not import or copy the GPL PyPoE implementation.
    """
    value = (seed ^ (len(raw) * M)) & MASK
    full = len(raw) // 8 * 8
    for (word,) in struct.iter_unpack("<Q", raw[:full]):
        word = word * M & MASK
        word ^= word >> 47
        value = (value ^ (word * M & MASK)) * M & MASK
    if raw[full:]:
        value = (value ^ int.from_bytes(raw[full:], "little")) * M & MASK
    value ^= value >> 47
    value = value * M & MASK
    return value ^ (value >> 47)


def seed_from_root(root):
    # A current-format index begins its directory table with the empty root.
    # Invert hash64(b"", seed); this establishes a hash convention, not authenticity.
    if root == 0x07E47507B4A92E53:
        raise ValueError("Legacy FNV index hashing is unsupported")
    value = root ^ (root >> 47)
    value = value * pow(M, -1, 1 << 64) & MASK
    seed = value ^ (value >> 47)
    if hash64(b"", seed) != root:
        raise ValueError("Index root hash cannot establish a Murmur64A seed")
    return seed


def load_request(path):
    raw = client.read_file(path, 2 * client.MIB)
    request = client.decode(raw)
    draft.fields(request, ("schema", "game", "realm", "game_patch", "decoder", "resources"), "index request")
    if request["schema"] != REQUEST or request["game"] != "poe2" or request["realm"] != "international":
        raise ValueError("Expected an explicit international PoE2 index request")
    if not re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", draft.text(request["game_patch"], "patch", 64)):
        raise ValueError("Expected an explicit numeric patch")
    pins = request["decoder"]
    draft.fields(pins, ("repository", "revision", "pyooz_version", "uv_lock_sha256"), "decoder pins")
    if pins["repository"] != "https://github.com/repoe-fork/pypoe":
        raise ValueError("Unsupported decoder repository")
    if not re.fullmatch(r"[a-f0-9]{40}", draft.text(pins["revision"], "decoder revision", 40)):
        raise ValueError("Decoder needs a full revision")
    draft.text(pins["pyooz_version"], "pyooz version", 64)
    client.digest(pins["uv_lock_sha256"])
    paths = request["resources"]
    if not isinstance(paths, list) or not 1 <= len(paths) <= 128:
        raise ValueError("Expected 1..128 resource paths")
    draft.resource_paths(paths)
    return request, raw


class Stream:
    """Sequential bounded bundle reader: retain at most one decompressed chunk."""

    def __init__(self, bundle):
        self.bundle = bundle
        self.offset = 0
        self.chunk = -1
        self.data = b""
        self.sha = hashlib.sha256()
        self.chunks = []

    def take(self, count):
        client.integer(count, 1, client.MAX_RESOURCE, "index read")
        if self.offset + count > self.bundle.size:
            raise ValueError("Truncated index prefix")
        result = bytearray()
        while len(result) < count:
            index, within = divmod(self.offset, self.bundle.chunk_size)
            if index != self.chunk:
                start = index * self.bundle.chunk_size
                length = min(self.bundle.chunk_size, self.bundle.size - start)
                self.data = self.bundle.read(start, length)
                self.chunk = index
                self.chunks.append({"chunk": index, "raw_offset": start, "bytes": length,
                                    "sha256": client.sha256(self.data)})
            amount = min(count - len(result), len(self.data) - within)
            result.extend(self.data[within:within + amount])
            self.offset += amount
        self.sha.update(result)
        return bytes(result)

    def unpack(self, fmt):
        return struct.unpack(fmt, self.take(struct.calcsize(fmt)))


def locations(bundle, paths):
    stream = Stream(bundle)
    (bundle_count,) = stream.unpack("<I")
    client.integer(bundle_count, 1, MAX_BUNDLES, "index bundle count")
    bundles = []
    for _ in range(bundle_count):
        (length,) = stream.unpack("<I")
        client.integer(length, 1, 256, "index bundle name length")
        name = stream.take(length).decode("ascii")
        (size,) = stream.unpack("<I")
        client.integer(size, 1, client.MAX_BUNDLE_RAW, "indexed bundle raw bytes")
        bundles.append((name, size))
    (file_count,) = stream.unpack("<I")
    client.integer(file_count, 1, MAX_FILES, "index file count")
    file_offset = stream.offset
    if file_offset + file_count * 20 + 24 > bundle.size:
        raise ValueError("File table exceeds index framing")
    # The seed follows the file table. Retain a bounded disk copy while deriving
    # requested hashes from the root, then scan only for the selected records.
    # A temporary file keeps the 160 MiB maximum table out of Python memory.
    import tempfile
    with tempfile.TemporaryFile() as records:
        table_sha = hashlib.sha256()
        remaining = file_count
        while remaining:
            count = min(50_000, remaining)
            data = stream.take(count * 20)
            for _, index, offset, size in struct.iter_unpack("<QIII", data):
                if index >= bundle_count or offset + size > bundles[index][1]:
                    raise ValueError("File record exceeds indexed bundle bounds")
            records.write(data)
            table_sha.update(data)
            remaining -= count
        (directories,) = stream.unpack("<I")
        client.integer(directories, 1, 250_000, "index directory count")
        root, _, _, _ = stream.unpack("<QIII")
        if stream.offset - 20 + directories * 20 >= bundle.size:
            raise ValueError("Directory table exceeds index framing")
        seed = seed_from_root(root)
        wanted = {}
        for name in paths:
            hashed = hash64(name.lower().encode("ascii"), seed)
            if hashed in wanted:
                raise ValueError("Requested resource hash collision")
            wanted[hashed] = name
        found, number = {}, 0
        records.seek(0)
        while data := records.read(1_000_000):
            for hashed, index, offset, size in struct.iter_unpack("<QIII", data):
                if hashed in wanted:
                    name = wanted[hashed]
                    if name in found:
                        raise ValueError(f"Duplicate selected index hash: {name}")
                    client.integer(size, 1, client.MAX_RESOURCE, "selected resource bytes")
                    bundle_name, bundle_size = bundles[index]
                    found[name] = {"record_index": number, "path_hash": hashed,
                                   "bundle_index": index, "bundle_name": bundle_name,
                                   "bundle_raw_bytes": bundle_size, "offset": offset, "length": size}
                number += 1
    missing = [name for name in paths if name not in found]
    if missing:
        raise ValueError("Paths not found in the selected index: " + ", ".join(missing))
    client.integer(sum(record["length"] for record in found.values()), 1, client.MAX_NETWORK, "selected raw total")
    return found, {"bundle_count": bundle_count, "file_count": file_count, "file_table_offset": file_offset,
                   "file_table_bytes": file_count * 20, "file_table_sha256": table_sha.hexdigest(),
                   "directory_count": directories, "first_directory_hash": root, "hash_seed": seed,
                   "parsed_prefix_bytes": stream.offset, "parsed_prefix_sha256": stream.sha.hexdigest(),
                   "decoded_chunks": stream.chunks}


def tools():
    return {path.name: client.sha256(client.read_file(path, 2 * client.MIB))
            for path in (Path(__file__), Path(client.__file__), Path(draft.__file__))}


def discover(request_path, cache_root, output, python, decoder_lock, mode, budget=0,
             request_cap=client.MAX_RANGE, archive=None, ledger=None):
    output, cache_root = client.safe_path(output), client.safe_path(cache_root)
    if output.exists():
        raise FileExistsError("Refusing to replace existing discovery output")
    inputs = [Path(__file__).resolve().parent.parent / "data", cache_root,
              client.safe_path(request_path), client.safe_path(decoder_lock)]
    if bool(archive) != bool(ledger):
        raise ValueError("Archive root and ledger must be supplied together")
    if archive:
        inputs.extend([client.safe_path(archive), client.safe_path(ledger)])
    if any(output == path or output.is_relative_to(path) or path.is_relative_to(output) for path in inputs):
        raise ValueError("Output must be separate from data/, cache and source/decoder inputs")
    request, request_bytes = load_request(request_path)
    identity = tools()
    base = f"https://{client.OFFICIAL_HOST}/{request['game_patch']}/"
    url = base + "Bundles2/_.index.bin"
    # Only the index URL is available to this range reader, never game resources.
    source = {"decoder": request["decoder"], "resources": {"index": {"bundle_url": url}}}
    cache = client.RangeCache(cache_root, source)
    ranges = client.Ranges(cache, mode, budget, request_cap)
    decoder = client.ExternalDecoder(python, decoder_lock, source)
    pins = request["decoder"]
    if (decoder.receipt["uv_lock_sha256"] != pins["uv_lock_sha256"]
            or decoder.receipt["version"] != pins["pyooz_version"]):
        raise ValueError("Decoder identity differs from index request")
    imported = client.import_ranges(cache, archive, ledger) if archive else None
    matched, framing = locations(client.Bundle(url, ranges, decoder), request["resources"])
    proof = {"schema": "pobr-client-index-locations/v1", "status": STATUS, "production_ready": False,
             "game_patch": request["game_patch"], "realm": request["realm"], "index_url": url,
             "request_sha256": client.sha256(request_bytes), "framing": framing, "matched": matched,
             "limitations": ["Caller path spelling and patch/realm are declarations; hashes are case-insensitive, non-cryptographic matches.",
                             "Only bundle/file tables and the first directory record are parsed, not the complete index or directory names.",
                             "The current-format empty-root Murmur64A convention is assumed; no raw resource or DAT is decoded.",
                             "Observed index/cache fingerprints are not upstream authentication or reviewed source-lock approval."]}
    proof_bytes = client.encoded(proof)
    selection = {"schema": draft.SELECTION_SCHEMA, "game": request["game"], "realm": request["realm"],
                 "game_patch": request["game_patch"], "base_url": base, "decoder": pins,
                 "location_evidence": [{"id": "index-locations", "reference": "index-locations.json",
                                        "sha256": client.sha256(proof_bytes)}], "resources": {}}
    for name in request["resources"]:
        record = matched[name]
        bundle_url = client.official_url(base + "Bundles2/" + record["bundle_name"] + ".bundle.bin", base + "Bundles2/")
        selection["resources"][name] = {"bundle_url": bundle_url, "uncompressed_offset": record["offset"],
                                        "uncompressed_length": record["length"], "location_evidence": ["index-locations"]}
    selection_bytes = client.encoded(selection)
    if (client.read_file(request_path, 2 * client.MIB) != request_bytes or tools() != identity
            or client.read_file(decoder.lock_path, 2 * client.MIB) != decoder.lock_bytes
            or client.decode(decoder.run([], b"", 65536)) != decoder.identity):
        raise ValueError("Discovery request/tools/decoder changed before completion")
    output.parent.mkdir(parents=True, exist_ok=True)
    client.safe_path(output)
    output.mkdir()
    client.write_new(output / "request.json", request_bytes)
    client.write_new(output / "index-locations.json", proof_bytes)
    client.write_new(output / "selection.json", selection_bytes)
    # Validate through the acquisition interface before publishing completion.
    draft.load_selection(output / "selection.json")
    for name, raw in [("request.json", request_bytes), ("index-locations.json", proof_bytes),
                      ("selection.json", selection_bytes)]:
        if client.read_file(output / name, 2 * client.MIB) != raw:
            raise ValueError("Discovery output changed before completion")
    receipt = {"schema": RECEIPT, "status": STATUS, "production_ready": False, "mode": mode,
               "files": {name: {"bytes": len(raw), "sha256": client.sha256(raw)} for name, raw in
                         [("request.json", request_bytes), ("index-locations.json", proof_bytes), ("selection.json", selection_bytes)]},
               "tools": identity, "decoder": decoder.receipt, "archive_import": imported,
               "network": {"requests": ranges.requests, "body_bytes": ranges.downloaded,
                           "reserved_bytes": ranges.reserved, "budget_bytes": budget, "per_request_cap_bytes": request_cap},
               "ranges": sorted(cache.used.values(), key=lambda r: (r["url"], r["start"], r["length"]))}
    client.write_new(output / "index-receipt.json", client.encoded(receipt))
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("request", "cache", "out", "decoder-python", "decoder-lock"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--mode", required=True, choices=["offline", "online"])
    parser.add_argument("--network-budget-bytes", type=int, default=0)
    parser.add_argument("--max-request-bytes", type=int, default=client.MAX_RANGE)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--ledger", type=Path)
    args = parser.parse_args()
    try:
        receipt = discover(args.request, args.cache, args.out, args.decoder_python, args.decoder_lock,
                           args.mode, args.network_budget_bytes, args.max_request_bytes, args.archive, args.ledger)
    except (ValueError, OSError, KeyError, TypeError, struct.error, UnicodeError, subprocess.SubprocessError) as error:
        parser.exit(1, f"client_index: {error}\n")
    print(json.dumps({"status": STATUS, "receipt_sha256": client.sha256(client.encoded(receipt)),
                      "selection": str(args.out / "selection.json")}, sort_keys=True))


if __name__ == "__main__":
    main()
