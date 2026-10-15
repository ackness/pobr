#!/usr/bin/env python3
"""Collect build codes for the accuracy report from the public poe.ninja index.

    python3 devs/scripts/fetch-reference-builds.py examples/reference-builds/tuned --pick first
    python3 devs/scripts/fetch-reference-builds.py examples/reference-builds/holdout --pick last

One character per ascendancy of the latest softcore league: the index's rows for
that ascendancy sorted by (account, character), without the characters a
committed corpus already holds, and then the first or the last of them. The
choice never looks at a calculation, so a set fetched this way says how PoBR
does on builds nobody has worked on. A character whose export is unusable is
recorded as a failure and not replaced.

Requires Python 3 and curl. `<set>/selection.json` records the league, the
snapshot and every candidate; `<set>/builds/<name>/` holds `code.txt` and
`meta.json` in the layout of `examples/wasm-benchmark/builds`.
"""
import argparse
import importlib.util
import json
import subprocess
import sys
import xml.etree.ElementTree as ET
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlencode

ROOT = Path(__file__).resolve().parents[2]
# The index wire format, the export checks and the fixture layout are the
# benchmark collector's; only the selection differs.
_SPEC = importlib.util.spec_from_file_location(
    "fetch_ninja_benchmarks", ROOT / "web/scripts/fetch-ninja-benchmarks.py")
ninja = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(ninja)

BASE_CLASSES = frozenset(base for base, _ in ninja.REPRESENTATIVES)
PICKS = ("first", "last")


def committed_characters(examples):
    """(account, character) of every build already under `examples/`."""
    taken = set()
    for meta in sorted(examples.glob("**/builds/*/meta.json")):
        source = json.loads(meta.read_text()).get("source") or {}
        if source.get("account") and source.get("character"):
            taken.add((source["account"], source["character"]))
    return taken


def choose(rows, taken, pick):
    """The row a selection takes: `rows` are sorted by (account, character)."""
    free = [row for row in rows if (row["account"], row["character"]) not in taken]
    if not free:
        return None
    return free[0] if pick == "first" else free[-1]


def ascendancies(classes):
    return sorted(name for name in classes if name not in BASE_CLASSES)


def discover(pick, timeout, taken):
    state, _ = ninja.fetch_json(ninja.STATE_URL, timeout)
    leagues = state.get("buildLeagues")
    if not isinstance(leagues, list) or not leagues:
        raise ValueError("no current build leagues in live index state")
    league = leagues[0]
    if league.get("hardcore") or league.get("name") in ("Standard", "Hardcore"):
        raise ValueError("first build league is not the latest softcore season")
    snapshots = [s for s in state.get("snapshotVersions", []) if s.get("url") == league["url"]]
    if len(snapshots) != 1:
        raise ValueError("missing or ambiguous latest-season snapshot")
    snapshot = snapshots[0]
    base_url = ninja.ORIGIN + f"/poe2/api/builds/{snapshot['version']}/search"
    query = {"overview": snapshot["snapshotName"]}
    raw = ninja.fetch(base_url + "?" + urlencode(query), timeout)
    refs = [ninja.protobuf_fields(d) for d in ninja.search_result(raw).get(6, [])]
    class_refs = [d for d in refs if d.get(1) == [b"class"]]
    if len(class_refs) != 1:
        raise ValueError("missing or ambiguous class dictionary")
    dict_url = ninja.ORIGIN + "/poe2/api/builds/dictionary/" + class_refs[0][2][0].decode()
    classes = ninja.dictionary_values(ninja.fetch(dict_url, timeout))

    manifest = {"selection": f"ascendancy-{pick}-v1", "league": league, "snapshot": snapshot,
                "snapshot_url": ninja.STATE_URL, "class_dictionary_url": dict_url,
                "candidates": [], "failures": []}
    for ascendancy in ascendancies(classes):
        url = base_url + "?" + urlencode({**query, "class": ascendancy})
        rows_raw = ninja.fetch(url, timeout)
        rows = ninja.index_candidates(rows_raw)
        row = choose(rows, taken, pick)
        if row is None:
            manifest["failures"].append({"index_ascendancy": ascendancy, "index_url": url,
                                         "error": "every indexed character is already committed"})
            continue
        taken.add((row["account"], row["character"]))
        manifest["candidates"].append({**row, "index_ascendancy": ascendancy, "index_url": url,
                                       "index_sha256": ninja.sha256(rows_raw),
                                       "index_rows": len(rows)})
    return manifest


def base_class_of(data):
    """The base class an export declares; the index filter only names the ascendancy."""
    code = data.get("pathOfBuildingExport")
    if not isinstance(code, str) or not code.strip():
        raise ValueError("missing pathOfBuildingExport")
    build = ET.fromstring(ninja.decode_pob_code(code.strip())).find("Build")
    if build is None or not build.get("className"):
        raise ValueError("export has no class")
    return build.get("className")


def collect(args):
    manifest_path = args.set / "selection.json"
    if args.replay:
        manifest = json.loads(manifest_path.read_text())
    else:
        if manifest_path.exists():
            raise ValueError(f"{manifest_path} exists; --replay re-fetches what it records")
        manifest = discover(args.pick, args.timeout, committed_characters(ROOT / "examples"))
    fetched_at = datetime.now(timezone.utc).isoformat(timespec="seconds")
    manifest["fetched_at"] = fetched_at
    manifest["failures"] = [f for f in manifest["failures"] if "candidate" not in f]
    manifest["fixtures"] = []
    for candidate in manifest["candidates"]:
        api_url = ninja.ORIGIN + f"/poe2/api/builds/{manifest['snapshot']['version']}/character?" + urlencode({
            "account": candidate["account"], "name": candidate["character"],
            "overview": manifest["snapshot"]["snapshotName"]})
        try:
            data, _ = ninja.fetch_json(api_url, args.timeout)
            candidate["base_class"] = base_class_of(data)
            code, meta = ninja.export_fixture(data, candidate, manifest, fetched_at, api_url)
            meta["source"]["selection"] = manifest["selection"]
            ninja.save_fixture(args.set / "builds", code, meta)
            manifest["fixtures"].append({"fixture": meta["name"], "character": meta["character"],
                                         "code_sha256": meta["pob"]["code_sha256"]})
            print(f"{meta['name']}: {meta['character']}")
        except (ValueError, KeyError, OSError, ET.ParseError, subprocess.TimeoutExpired) as error:
            manifest["failures"].append({"candidate": candidate, "api_url": api_url, "error": str(error)})
            print(f"FAILED {candidate['account']}/{candidate['character']}: {error}", file=sys.stderr)
    args.set.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    print(f"Collected {len(manifest['fixtures'])}/{len(manifest['candidates'])}; "
          f"failures={len(manifest['failures'])}; manifest={manifest_path}")
    return 1 if manifest["failures"] else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("set", type=Path, help="Directory of the set, e.g. examples/reference-builds/tuned")
    parser.add_argument("--pick", choices=PICKS, default="first",
                        help="Which of an ascendancy's free rows to take")
    parser.add_argument("--replay", action="store_true",
                        help="Fetch the candidates the set's selection.json records")
    parser.add_argument("--timeout", type=int, default=30, help="Per-request timeout in seconds")
    args = parser.parse_args()
    if args.timeout <= 0 or args.timeout > 120:
        parser.error("--timeout must be between 1 and 120 seconds")
    args.set = args.set if args.set.is_absolute() else Path.cwd() / args.set
    try:
        return collect(args)
    except (ValueError, KeyError, OSError, subprocess.TimeoutExpired) as error:
        print(f"Acquisition failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
