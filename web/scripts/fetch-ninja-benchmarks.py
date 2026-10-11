#!/usr/bin/env python3
"""Collect performance-only PoE2 fixtures from the current public poe.ninja index.

Requires Python 3 and curl, no packages. Live discovery uses the site's index-state
APIs. Selection is independent of calculation results: for each declared base
class, query its representative ascendancy and take the lexicographically first
(account, character) among the index's first 100 rows. No failed candidate is
replaced. --selection-file replays the recorded candidates and pinned snapshot;
poe.ninja can eventually retire a snapshot, in which case replay fails visibly.
"""
import argparse
import binascii
import hashlib
import json
import re
import struct
import subprocess
import sys
import xml.etree.ElementTree as ET
import zlib
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote, urlencode

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "examples/demo-bd-test/tools"))
from make_fixture import decode_pob_code, extract_meta

ORIGIN = "https://poe.ninja"
INDEX_URL = ORIGIN + "/poe2/api/data/build-index-state"
STATE_URL = ORIGIN + "/poe2/api/data/index-state"
SELECTION = "base-class-first-v1"
# Declared index filters, not inferred main-skill identities. XML supplies the
# authoritative base class, ascendancy, and selected main socket group.
REPRESENTATIVES = (
    ("Druid", "Oracle"), ("Huntress", "Amazon"),
    ("Mercenary", "Gemling Legionnaire"), ("Monk", "Martial Artist"),
    ("Ranger", "Deadeye"), ("Sorceress", "Stormweaver"),
    ("Warrior", "Titan"), ("Witch", "Blood Mage"),
)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def fetch(url, timeout):
    result = subprocess.run(
        ["curl", "--fail-with-body", "--location", "--silent", "--show-error",
         "--compressed", "--connect-timeout", str(min(timeout, 10)),
         "--max-time", str(timeout), "--user-agent", "Mozilla/5.0", url],
        capture_output=True, timeout=timeout + 5,
    )
    if result.returncode:
        raise ValueError(f"{url}: curl exit {result.returncode}: "
                         f"{result.stderr.decode(errors='replace').strip()} "
                         f"{result.stdout[:300].decode(errors='replace')}")
    return result.stdout


def fetch_json(url, timeout):
    raw = fetch(url, timeout)
    data = json.loads(raw)
    if not isinstance(data, dict):
        raise ValueError(f"{url}: expected JSON object")
    return data, raw


def varint(data, pos):
    value = 0
    for shift in range(0, 70, 7):
        if pos >= len(data):
            raise ValueError("truncated protobuf varint")
        byte = data[pos]
        pos += 1
        value |= (byte & 127) << shift
        if not byte & 128:
            return value, pos
    raise ValueError("oversized protobuf varint")


def protobuf_fields(data):
    """Read the site's SearchResult wire format without protobuf dependencies.

    Schema is published in the live search frontend module. Only fields needed
    for public index selection are interpreted; unknown valid fields are kept.
    """
    fields = {}
    pos = 0
    while pos < len(data):
        tag, pos = varint(data, pos)
        field, wire = tag >> 3, tag & 7
        if not field:
            raise ValueError("invalid protobuf field zero")
        if wire == 0:
            value, pos = varint(data, pos)
        elif wire in (1, 2, 5):
            if wire == 2:
                size, pos = varint(data, pos)
            else:
                size = 8 if wire == 1 else 4
            if pos + size > len(data):
                raise ValueError("truncated protobuf field")
            value = data[pos:pos + size]
            pos += size
        else:
            raise ValueError(f"unsupported protobuf wire type {wire}")
        fields.setdefault(field, []).append(value)
    return fields


def search_result(data):
    wrapper = protobuf_fields(data)
    if len(wrapper.get(1, [])) != 1 or not isinstance(wrapper[1][0], bytes):
        raise ValueError("missing public SearchResult")
    return protobuf_fields(wrapper[1][0])


def index_candidates(data):
    columns = {}
    for raw in search_result(data).get(12, []):
        col = protobuf_fields(raw)
        name = col.get(1, [b""])[0].decode()
        if name in columns:
            raise ValueError(f"duplicate index column {name}")
        columns[name] = col
    names = columns.get("name", {}).get(7, [])
    accounts = columns.get("account", {}).get(7, [])
    if not names or len(names) != len(accounts):
        raise ValueError("missing or mismatched public name/account columns")
    candidates = [{"account": a.decode(), "character": n.decode()}
                  for a, n in zip(accounts, names)]
    if any(not c["account"] or not c["character"] for c in candidates):
        raise ValueError("empty public character identity")
    return sorted(candidates, key=lambda c: (c["account"], c["character"]))


def dictionary_values(data):
    if data[:4] != b"NDIC":
        return [s.decode() for s in protobuf_fields(data).get(2, [])]
    if len(data) < 36 or struct.unpack_from("<I", data, 4)[0] != 2:
        raise ValueError("unsupported native dictionary header")
    count = struct.unpack_from("<I", data, 12)[0]
    syncs, lengths = struct.unpack_from("<II", data, 28)
    pos = 36 + syncs * 8
    end = pos + lengths
    text = end
    if end > len(data):
        raise ValueError("truncated native dictionary lengths")
    values = []
    for _ in range(count):
        size, pos = varint(data, pos)
        if pos > end or text + size > len(data):
            raise ValueError("truncated native dictionary value")
        values.append(data[text:text + size].decode())
        text += size
    if pos != end or text != len(data):
        raise ValueError("unexpected native dictionary trailing data")
    return values


def discover(timeout, cache):
    index, index_raw = fetch_json(INDEX_URL, timeout)
    state, state_raw = fetch_json(STATE_URL, timeout)
    # The UI lists the latest softcore seasonal league first. Standard and
    # Hardcore are permanent leagues and cannot satisfy latest-season sampling.
    leagues = state.get("buildLeagues")
    if not isinstance(leagues, list) or not leagues:
        raise ValueError("no current build leagues in live index state")
    league = leagues[0]
    if league.get("hardcore") or league.get("name") in ("Standard", "Hardcore"):
        raise ValueError("first build league is not the latest softcore season")
    if not any(l.get("leagueName") == league["name"] and
               l.get("leagueUrl") == league["url"] for l in index.get("leagueBuilds", [])):
        raise ValueError("live league index and snapshot state disagree")
    snapshots = [s for s in state.get("snapshotVersions", []) if s.get("url") == league["url"]]
    if len(snapshots) != 1:
        raise ValueError("missing or ambiguous latest-season snapshot")
    snapshot = snapshots[0]
    base_url = ORIGIN + f"/poe2/api/builds/{snapshot['version']}/search"
    query = {"overview": snapshot["snapshotName"]}
    index_url = base_url + "?" + urlencode(query)
    raw = fetch(index_url, timeout)
    refs = [protobuf_fields(d) for d in search_result(raw).get(6, [])]
    class_refs = [d for d in refs if d.get(1) == [b"class"]]
    if len(class_refs) != 1:
        raise ValueError("missing or ambiguous class dictionary")
    dict_url = ORIGIN + "/poe2/api/builds/dictionary/" + class_refs[0][2][0].decode()
    dict_raw = fetch(dict_url, timeout)
    classes = dictionary_values(dict_raw)
    missing = [a for _, a in REPRESENTATIVES if a not in classes]
    if missing:
        raise ValueError(f"representative ascendancies absent from latest index: {missing}")
    cache.mkdir(parents=True, exist_ok=True)
    (cache / "build-index-state.json").write_bytes(index_raw)
    (cache / "index-state.json").write_bytes(state_raw)
    (cache / "index.pb").write_bytes(raw)
    (cache / "class-dictionary.bin").write_bytes(dict_raw)
    manifest = {"selection": SELECTION, "league": league, "snapshot": snapshot,
                "index_url": INDEX_URL, "snapshot_url": STATE_URL,
                "public_index_url": index_url, "public_index_sha256": sha256(raw),
                "class_dictionary_url": dict_url, "available_classes": classes,
                "discovery": {"buildLeagues": state["buildLeagues"],
                              "leagueBuilds": index["leagueBuilds"]},
                "candidates": [], "failures": []}
    for base, ascendancy in REPRESENTATIVES:
        url = base_url + "?" + urlencode({**query, "class": ascendancy})
        rows_raw = fetch(url, timeout)
        rows = index_candidates(rows_raw)
        (cache / f"index-{slug(base)}.pb").write_bytes(rows_raw)
        manifest["candidates"].append({**rows[0], "base_class": base,
            "index_ascendancy": ascendancy, "index_url": url,
            "index_sha256": sha256(rows_raw), "index_rows": len(rows),
            "selection_reason": "lexicographically first account/character in index response"})
    return manifest


def slug(value):
    return re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")


def export_fixture(data, candidate, manifest, fetched_at, api_url):
    if data.get("account") != candidate["account"] or data.get("name") != candidate["character"]:
        raise ValueError("API character identity differs from selected public index candidate")
    if data.get("league") != manifest["league"]["name"]:
        raise ValueError("API character is not in the selected latest season")
    code = data.get("pathOfBuildingExport")
    if not isinstance(code, str) or not code.strip():
        raise ValueError("missing pathOfBuildingExport")
    code = code.strip()
    try:
        xml = decode_pob_code(code)
        root = ET.fromstring(xml)
    except (binascii.Error, zlib.error, ET.ParseError) as error:
        raise ValueError(f"invalid PoB export: {error}") from error
    if root.tag != "PathOfBuilding2":
        raise ValueError("export is not PathOfBuilding2 XML")
    meta = extract_meta(xml)
    ch = meta["character"]
    if ch["class"] != candidate["base_class"] or ch["ascendancy"] != candidate["index_ascendancy"]:
        raise ValueError(f"export class differs from selected filter: {ch}")
    if not ch["main_skill"] or ch["level"] <= 0:
        raise ValueError("export lacks a real selected main skill or character level")
    tree = root.find("Tree/Spec")
    tree_version = tree.get("treeVersion") if tree is not None else None
    if not tree_version or not re.fullmatch(r"\d+(?:_\d+)+", tree_version):
        raise ValueError("export has no usable game tree version")
    game_version = tree_version.replace("_", ".")
    if data.get("passiveTreeName") != "PassiveTree-" + game_version:
        raise ValueError("export game tree version disagrees with API passiveTreeName")
    identity = sha256((candidate["account"] + "\0" + candidate["character"]).encode())[:12]
    name = "-".join(["ninja", fetched_at[:10], slug(ch["class"]),
                     slug(ch["ascendancy"]), slug(ch["main_skill"]), identity])
    source = {"site": "poe.ninja", "url": ORIGIN + "/poe2/builds/" +
              manifest["league"]["url"] + "/character/" +
              quote(candidate["account"], safe="") + "/" + quote(candidate["character"], safe=""),
              "account": candidate["account"], "character": candidate["character"],
              "league": data["league"], "game_version": game_version,
              "fetched_at": fetched_at, "api_url": api_url,
              "snapshot": manifest["snapshot"]["version"],
              "snapshot_name": manifest["snapshot"]["snapshotName"],
              "index_url": candidate["index_url"], "selection": SELECTION,
              "game_version_evidence": {"xml_tree_version": tree_version,
                                        "api_passive_tree": data["passiveTreeName"]}}
    return code, {"name": name, "source": source, "character": ch,
                  "pob": {**meta["pob"], "code_sha256": sha256(code.encode()),
                          "code_chars": len(code), "decoded_bytes": len(xml)}}


def save_fixture(output, code, meta):
    folder = output / meta["name"]
    if folder.exists():
        old_code = (folder / "code.txt").read_text().strip()
        old_meta = json.loads((folder / "meta.json").read_text())
        if old_code != code or old_meta["pob"]["code_sha256"] != sha256(code.encode()):
            raise ValueError(f"refusing to overwrite different or corrupt fixture: {folder}")
        return
    folder.mkdir(parents=True)
    (folder / "code.txt").write_text(code + "\n")
    (folder / "meta.json").write_text(json.dumps(meta, ensure_ascii=False, indent=2) + "\n")


def collect(args):
    manifest = json.loads(args.selection_file.read_text()) if args.selection_file else discover(args.timeout, args.cache)
    if manifest.get("selection") != SELECTION or len(manifest.get("candidates", [])) != 8:
        raise ValueError("selection manifest must declare base-class-first-v1 and eight candidates")
    pairs = [(c.get("base_class"), c.get("index_ascendancy")) for c in manifest["candidates"]]
    identities = [(c.get("account"), c.get("character")) for c in manifest["candidates"]]
    if sorted(pairs) != sorted(REPRESENTATIVES) or len(set(identities)) != 8:
        raise ValueError("selection manifest must preserve eight distinct declared base-class candidates")
    if any(not all(isinstance(value, str) and value for value in identity) for identity in identities):
        raise ValueError("selection manifest contains an empty or invalid public character identity")
    fetched_at = datetime.now(timezone.utc).isoformat(timespec="seconds")
    manifest["fetched_at"] = fetched_at
    manifest["failures"] = []
    manifest["fixtures"] = []
    args.cache.mkdir(parents=True, exist_ok=True)
    for candidate in manifest["candidates"]:
        api_url = ORIGIN + f"/poe2/api/builds/{manifest['snapshot']['version']}/character?" + urlencode({
            "account": candidate["account"], "name": candidate["character"],
            "overview": manifest["snapshot"]["snapshotName"]})
        try:
            try:
                data, raw = fetch_json(api_url, args.timeout)
            except (ValueError, subprocess.TimeoutExpired) as error:
                # The public profile model is the site's alternate export source.
                # Its league and identity still must pass the same validations.
                manifest["failures"].append({"candidate": candidate, "api_url": api_url,
                                             "error": str(error), "fallback_attempted": True})
                api_url = ORIGIN + "/poe2/api/profile/characters/" + quote(candidate["account"], safe="") + "/" + quote(candidate["character"], safe="") + "/model/0"
                profile, raw = fetch_json(api_url, args.timeout)
                data = profile.get("charModel")
                if not isinstance(data, dict):
                    raise ValueError("public profile response lacks charModel")
            identity = sha256((candidate["account"] + "\0" + candidate["character"]).encode())[:12]
            (args.cache / f"character-{identity}.json").write_bytes(raw)
            code, meta = export_fixture(data, candidate, manifest, fetched_at, api_url)
            save_fixture(args.output, code, meta)
            manifest["fixtures"].append({"fixture": meta["name"], "character": meta["character"],
                                         "code_sha256": meta["pob"]["code_sha256"], "api_url": api_url})
            print(f"{meta['name']}: {meta['character']}")
        except (ValueError, KeyError, OSError, ET.ParseError, subprocess.TimeoutExpired) as error:
            manifest["failures"].append({"candidate": candidate, "api_url": api_url, "error": str(error)})
            print(f"FAILED {candidate['account']}/{candidate['character']}: {error}", file=sys.stderr)
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    args.manifest.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    print(f"Collected {len(manifest['fixtures'])}/8; failures={len(manifest['failures'])}; manifest={args.manifest}")
    return 1 if manifest["failures"] or len(manifest["fixtures"]) != 8 else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selection-file", type=Path, help="Replay a recorded pinned snapshot and candidates")
    parser.add_argument("--selection", choices=[SELECTION], default=SELECTION)
    parser.add_argument("--timeout", type=int, default=30, help="Finite per-request timeout in seconds")
    parser.add_argument("--output", type=Path, default=ROOT / "examples/wasm-benchmark/builds")
    parser.add_argument("--cache", type=Path, default=ROOT / ".cache/ninja-benchmarks")
    parser.add_argument("--manifest", type=Path, default=ROOT / "examples/wasm-benchmark/selection.json")
    args = parser.parse_args()
    if args.timeout <= 0 or args.timeout > 120:
        parser.error("--timeout must be between 1 and 120 seconds")
    try:
        return collect(args)
    except (ValueError, KeyError, OSError, subprocess.TimeoutExpired) as error:
        print(f"Acquisition failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
