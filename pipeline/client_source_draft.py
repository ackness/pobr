"""Collect named, caller-located resources into an UNREVIEWED acquisition draft.

This first-acquisition stage observes raw hashes. It cannot create a reviewed
source lock, certify location evidence, or adopt production data.
"""

import argparse
import json
from pathlib import Path
import re
import struct
import subprocess
import sys

import client_sources as client


SELECTION_SCHEMA = "pobr-client-source-selection/v1"
RECEIPT_SCHEMA = "pobr-client-source-draft/v1"
STATUS = "unreviewed-acquisition-draft"
RECEIPT = "draft-receipt.json"
SELECTION_COPY = "source-selection.json"
LOCATION_STATUS = "caller-supplied-not-independently-checked"
HASH_SEMANTICS = "observed during acquisition; not verified against pre-existing trusted raw pins"


def fields(value, required, label):
    if not isinstance(value, dict) or set(value) != set(required):
        raise ValueError(f"Invalid {label} fields; expected exactly: {', '.join(required)}")


def text(value, label, cap=2048):
    if (not isinstance(value, str) or not 1 <= len(value) <= cap
            or any(ord(c) < 32 or ord(c) == 127 for c in value)):
        raise ValueError(f"Invalid {label}")
    return value


def resource_paths(names):
    """Validate portable game paths consistently for discovery and acquisition."""
    paths, spellings = set(), {}
    for name in names:
        text(name, "resource path", 512)
        path = client.relative(name)
        if path.parts[0] not in {"Data", "Metadata"} or len(path.parts) < 2:
            raise ValueError("Expected a game-relative Data/ or Metadata/ path")
        for part in path.parts:
            if (not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_. -]{0,127}", part)
                    or part.endswith((".", " "))
                    or part.split(".")[0].upper() in {"CON", "PRN", "AUX", "NUL",
                        *(f"COM{i}" for i in range(1, 10)), *(f"LPT{i}" for i in range(1, 10))}):
                raise ValueError("Unsafe portable game path")
        folded = name.casefold()
        if folded in paths:
            raise ValueError("Case-colliding resource paths")
        paths.add(folded)
        for depth in range(1, len(path.parts) + 1):
            prefix = "/".join(path.parts[:depth])
            previous = spellings.setdefault(prefix.casefold(), prefix)
            if previous != prefix:
                raise ValueError("Case-colliding resource directory paths")
    if any(parent.as_posix().casefold() in paths for name in names for parent in Path(name).parents):
        raise ValueError("Resource file/directory path collision")


def load_selection(path):
    """Validate locations, never reinterpret observed hashes as expected pins."""
    raw = client.read_file(path, 2 * client.MIB)
    selection = client.decode(raw)
    fields(selection, ("schema", "game", "realm", "game_patch", "base_url", "decoder",
                       "location_evidence", "resources"), "location-only selection")
    if selection["schema"] != SELECTION_SCHEMA or selection["game"] != "poe2":
        raise ValueError("Unsupported location-only selection schema/game")
    if selection["realm"] != "international":
        raise ValueError("Only the explicit international PoE2 CDN realm is supported")
    patch = text(selection["game_patch"], "numeric patch", 64)
    if not re.fullmatch(r"[0-9]+(?:\.[0-9]+)+", patch):
        raise ValueError("Selection needs an explicit numeric patch")
    base = f"https://{client.OFFICIAL_HOST}/{patch}/"
    if selection["base_url"] != base:
        raise ValueError("Selection base_url differs from its fixed patch")
    decoder = selection["decoder"]
    fields(decoder, ("repository", "revision", "pyooz_version", "uv_lock_sha256"), "decoder pins")
    if decoder["repository"] != "https://github.com/repoe-fork/pypoe":
        raise ValueError("Unsupported decoder repository")
    if not re.fullmatch(r"[a-f0-9]{40}", text(decoder["revision"], "decoder revision", 40)):
        raise ValueError("Decoder needs a full Git revision")
    text(decoder["pyooz_version"], "pyooz version", 64)
    client.digest(decoder["uv_lock_sha256"])
    evidence = selection["location_evidence"]
    if not isinstance(evidence, list) or not 1 <= len(evidence) <= 128:
        raise ValueError("Invalid location evidence count")
    evidence_ids = set()
    for entry in evidence:
        fields(entry, ("id", "reference", "sha256"), "location evidence")
        identifier = text(entry["id"], "evidence id", 128)
        if identifier in evidence_ids:
            raise ValueError("Duplicate location evidence id")
        evidence_ids.add(identifier)
        # Opaque references are preserved, never opened, fetched or authenticated.
        text(entry["reference"], "evidence reference")
        client.digest(entry["sha256"])
    resources = selection["resources"]
    if not isinstance(resources, dict) or not 1 <= len(resources) <= 128:
        raise ValueError("Invalid selected resource count")
    resource_paths(resources)
    total = 0
    for name, record in resources.items():
        # Reject extra fields, especially bytes/sha256/expected_sha256 raw pins.
        fields(record, ("bundle_url", "uncompressed_offset", "uncompressed_length",
                        "location_evidence"), "location-only resource (no raw hash pins)")
        length = client.integer(record["uncompressed_length"], 1, client.MAX_RESOURCE, "resource length")
        client.integer(record["uncompressed_offset"], 0, client.MAX_BUNDLE_RAW - length, "resource offset")
        url = client.official_url(record["bundle_url"], base + "Bundles2/")
        if not url.endswith(".bundle.bin"):
            raise ValueError("Only fixed resource bundles are supported; no index discovery")
        refs = record["location_evidence"]
        if (not isinstance(refs, list) or not 1 <= len(refs) <= len(evidence_ids)
                or any(not isinstance(ref, str) or ref not in evidence_ids for ref in refs)
                or len(set(refs)) != len(refs)):
            raise ValueError("Resource needs unique known location evidence references")
        total += length
    client.integer(total, 1, client.MAX_NETWORK, "selected raw byte total")
    return selection, raw


def tool_identity():
    return {path.name: client.sha256(client.read_file(path, 2 * client.MIB))
            for path in (Path(__file__), Path(client.__file__))}


def collect(selection_path, cache_root, output, python, decoder_lock, mode,
            budget=0, request_cap=client.MAX_RANGE):
    output, cache_root = client.safe_path(output), client.safe_path(cache_root)
    if output.exists():
        raise FileExistsError(f"Refusing to replace existing output: {output}")
    inputs = [Path(__file__).resolve().parent.parent / "data", cache_root,
              client.safe_path(selection_path), client.safe_path(decoder_lock),
              Path(python).absolute().resolve()]
    if any(output == p or output.is_relative_to(p) or p.is_relative_to(output) for p in inputs):
        raise ValueError("Output must be separate from data/, cache and source/decoder inputs")
    selection, selection_bytes = load_selection(selection_path)
    tools = tool_identity()
    # One shared cache/range reader for the entire run: all existing framing,
    # validators, cache bounds, cumulative reservations and decoder limits apply.
    ranges = client.Ranges(client.RangeCache(cache_root, selection), mode, budget, request_cap)
    decoder = client.ExternalDecoder(python, decoder_lock, selection)
    pins = selection["decoder"]
    if (decoder.receipt["uv_lock_sha256"] != pins["uv_lock_sha256"]
            or decoder.receipt["version"] != pins["pyooz_version"]):
        raise ValueError("Decoder identity differs from selection pins")
    output.parent.mkdir(parents=True, exist_ok=True)
    client.safe_path(output)
    output.mkdir()
    client.write_new(output / SELECTION_COPY, selection_bytes)
    bundles, observations = {}, []
    for name, record in selection["resources"].items():
        url = record["bundle_url"]
        if url not in bundles:
            bundles[url] = client.Bundle(url, ranges, decoder)
        raw = bundles[url].read(record["uncompressed_offset"], record["uncompressed_length"])
        if len(raw) != record["uncompressed_length"]:
            raise ValueError(f"Raw length differs from indexed selection: {name}")
        client.write_new(output / client.relative(name), raw)
        observations.append({"path": name, **record, "observed_bytes": len(raw),
                             "observed_sha256": client.sha256(raw)})
    # Recheck output and mutable source/decoder inputs; no completion marker on
    # failure. Hashes here detect local change, not pre-existing raw-pin matches.
    for record in observations:
        raw = client.read_file(output / client.relative(record["path"]), client.MAX_RESOURCE)
        if len(raw) != record["observed_bytes"] or client.sha256(raw) != record["observed_sha256"]:
            raise ValueError(f"Output changed before completion: {record['path']}")
    if (client.read_file(selection_path, 2 * client.MIB) != selection_bytes
            or client.read_file(output / SELECTION_COPY, 2 * client.MIB) != selection_bytes):
        raise ValueError("Source selection changed before completion")
    if client.read_file(decoder.lock_path, 2 * client.MIB) != decoder.lock_bytes:
        raise ValueError("Decoder lock changed before completion")
    if client.decode(decoder.run([], b"", 65536)) != decoder.identity:
        raise ValueError("Decoder identity changed before completion")
    if tool_identity() != tools:
        raise ValueError("Collector code changed before completion")
    receipt = {
        "schema": RECEIPT_SCHEMA, "status": STATUS, "production_ready": False,
        "selection": {"path": SELECTION_COPY, "bytes": len(selection_bytes),
                      "sha256": client.sha256(selection_bytes), "schema": SELECTION_SCHEMA},
        "game": selection["game"], "realm": selection["realm"], "game_patch": selection["game_patch"],
        "base_url": selection["base_url"], "source_decoder_declaration": pins,
        "decoder": decoder.receipt, "tool_sha256": tools, "collector_python": sys.version,
        "mode": mode, "resources": observations, "raw_hash_semantics": HASH_SEMANTICS,
        "location_claims": {"status": LOCATION_STATUS, "evidence": selection["location_evidence"]},
        "network": {"requests": ranges.requests, "body_bytes": ranges.downloaded,
                    "reserved_bytes": ranges.reserved, "budget_bytes": budget,
                    "per_request_cap_bytes": request_cap},
        "ranges": sorted(ranges.cache.used.values(), key=lambda r: (r["url"], r["start"], r["length"])),
        "unresolved_validation": [
            "Caller-supplied resource names/locations and evidence fingerprints are not independently checked against indexes.",
            "Realm is caller-declared; fixed official HTTPS URLs and cache integrity do not independently authenticate game content.",
            "Raw hashes are observations; no reviewed source lock or trusted expected raw hashes were supplied.",
            "DAT/schema compatibility, table semantics, foreign keys, gameplay and domain completeness are not checked.",
            "Decoder fingerprints establish local identity, not upstream artifact authenticity.",
            "Source-lock review/promotion, strict replay and production adoption remain separate steps.",
        ],
    }
    client.write_new(output / RECEIPT, client.encoded(receipt))
    return receipt


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selection", required=True, type=Path, help="Location-only selection JSON; no expected raw hashes")
    parser.add_argument("--cache", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path, help="Absent raw output root")
    parser.add_argument("--decoder-python", required=True, type=Path)
    parser.add_argument("--decoder-lock", required=True, type=Path, help="Explicit external uv.lock")
    parser.add_argument("--mode", required=True, choices=["offline", "online"])
    parser.add_argument("--network-budget-bytes", type=int, default=0, help="Required online; cumulative, at most 64 MiB")
    parser.add_argument("--max-request-bytes", type=int, default=client.MAX_RANGE)
    args = parser.parse_args(argv)
    try:
        receipt = collect(args.selection, args.cache, args.out, args.decoder_python, args.decoder_lock,
                          args.mode, args.network_budget_bytes, args.max_request_bytes)
        print(json.dumps({"status": STATUS, "production_ready": False,
                          "receipt": str(args.out / RECEIPT),
                          "receipt_sha256": client.sha256(client.encoded(receipt))}, sort_keys=True))
    except (ValueError, OSError, KeyError, TypeError, struct.error, subprocess.SubprocessError) as error:
        parser.exit(1, f"client_source_draft: {error}\n")


if __name__ == "__main__":
    main()
