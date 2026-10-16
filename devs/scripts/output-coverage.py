#!/usr/bin/env python3
"""Measure display-stat breadth and agreement on the worked build sets.

Build `cargo build -p pobr-cli` first, then run this script. Oracle caches are
read only; --refresh-missing writes missing entries to this worktree's cache.
No holdout build may be used. Standard library only.
"""

import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys

REPO = Path(__file__).resolve().parents[2]
_SPEC = importlib.util.spec_from_file_location("compare_build", REPO / "devs/scripts/compare-build.py")
compare = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(compare)

DEFAULT_SETS = (
    "examples/wasm-benchmark/builds",
    "examples/reference-builds/tuned/builds",
    "examples/demo-bd-test/builds",
)
EXCLUDED = re.compile(
    r"dodge|suppression|brand|darkness|seal|pvp|loot|impale|corruptingblood|"
    r"devotion|tribute|soulcost|soulgainprevention|^Spec:",
    re.IGNORECASE,
)


def catalog_stats(source):
    """Read the literal computed declarations, retaining id -> PoB-key aliases."""
    entries = re.findall(
        r'computed\(\s*"([^"]+)"\s*,\s*Cat::\w+\s*,\s*Vt::\w+\s*,\s*"([^"]+)"',
        source,
    )
    if not entries or len({key for key, _ in entries}) != len(entries):
        raise ValueError("missing or duplicate computed display-stat declarations")
    return dict(entries)


def sidebar_stats(source):
    """Unique player sidebar keys; hand children are individual numeric stats."""
    player = source.split("local minionDisplayStats", 1)[0]
    keys = set()
    for line in player.splitlines():
        match = re.search(r'\bstat\s*=\s*"([^"]+)"', line.split("--", 1)[0])
        if match and not EXCLUDED.search(match[1]):
            child = re.search(r'\bchildStat\s*=\s*"([^"]+)"', line)
            keys.add(f"{match[1]}.{child[1]}" if child else match[1])
    return keys


def number(value):
    if isinstance(value, bool):
        return None
    try:
        value = float(value)
    except (TypeError, ValueError):
        return None
    return value if math.isfinite(value) else None


def oracle_value(oracle, key):
    if "." in key:
        hand, key = key.split(".", 1)
        table = oracle.get(f"calcs{hand}Output", {})
    else:
        table = oracle.get("calcsOutput", {})
    return number(table.get(key)) if isinstance(table, dict) else None


def relative_error(ours, theirs):
    if math.isclose(ours, theirs, rel_tol=0, abs_tol=1e-9):
        return 0.0
    return abs(ours - theirs) / abs(theirs) if theirs else None


def summarize(stat, pob_key, builds, worst=3):
    pairs = []
    for build in builds:
        ours = number(build["values"].get(stat))
        theirs = oracle_value(build["oracle"], pob_key)
        if ours is not None and theirs is not None:
            error = relative_error(ours, theirs)
            pair = {"build": build["build"], "engine": ours, "oracle": theirs,
                    "relative_error": error}
            # PoBR reports the requested reservation even when it exceeds the
            # pool. PoB2 caps the displayed reserved amount, but retains the
            # negative remainder. Show that convention without changing the
            # agreement counts or hiding differences in the skills themselves.
            if stat == "SpiritReserved":
                pool = oracle_value(build["oracle"], "Spirit")
                remaining = oracle_value(build["oracle"], "SpiritUnreserved")
                if pool is not None and remaining is not None and remaining < 0 and theirs == pool:
                    pair["oracle_before_pool_cap"] = pool - remaining
            pairs.append(pair)
    disagreements = [p for p in pairs if p["relative_error"] is None or p["relative_error"] > 0.05]
    disagreements.sort(key=lambda p: (p["relative_error"] is None, p["relative_error"] or 0), reverse=True)
    return {
        "stat": stat, "pob_key": pob_key, "both": len(pairs),
        "nonzero": sum(p["engine"] != 0 or p["oracle"] != 0 for p in pairs),
        "within_5pct": sum(p["relative_error"] is not None and p["relative_error"] <= 0.05 for p in pairs),
        "within_10pct": sum(p["relative_error"] is not None and p["relative_error"] <= 0.10 for p in pairs),
        "worst": disagreements[:worst], "disagreements": disagreements,
    }


def build_files(paths):
    files = set()
    for raw in paths:
        path = Path(raw).resolve()
        if "holdout" in path.parts:
            raise ValueError("holdout builds are only measured by `mise run accuracy`")
        found = [path] if path.name == "code.txt" and path.is_file() else sorted(path.glob("**/code.txt"))
        if not found:
            raise ValueError(f"no builds in {path}")
        for code in found:
            if "holdout" in code.resolve().parts:
                raise ValueError("holdout builds are only measured by `mise run accuracy`")
            files.add(code)
    return sorted(files)


def measure(binary, code_file, catalog, cache_dirs, commit, refresh):
    result = subprocess.run(
        [str(binary), "calculate-build", "--file", str(code_file), "--reference-basis",
         "--json", "--breakdown", ",".join(["all", *catalog])],
        cwd=REPO, capture_output=True, text=True,
    )
    if result.returncode:
        raise RuntimeError(f"engine failed ({result.returncode}): {result.stderr[-2000:]}")
    report = json.loads(result.stdout)
    entries = {row["stat"]: row for row in report.get("breakdowns", [])}
    differing = [row["stat"] for row in (report.get("reference") or {}).get("differing", [])]
    code = code_file.read_text(encoding="utf-8")
    names = compare.oracle_names(differing, entries)
    expected = compare.oracle_cache_path(code, names, commit)
    cached = next((folder / expected.name for folder in cache_dirs if (folder / expected.name).is_file()), None)
    if cached is None and refresh:
        compare.run_oracle(compare.decode_code(code), expected, names)
        cached = expected
    if cached is None:
        raise RuntimeError(f"oracle cache missing: {expected.name}; use --refresh-missing to calculate locally")
    oracle = json.loads(cached.read_text(encoding="utf-8"))
    if not isinstance(oracle.get("calcsOutput"), dict):
        raise RuntimeError(f"oracle cache has no full calcsOutput: {cached}")
    return {"build": str(code_file.parent.relative_to(REPO)),
            "oracle_cache": str(cached), "oracle": oracle,
            "values": {stat: row.get("value") for stat, row in entries.items()}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="*", help="worked set/build directory or code.txt")
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("POBR_BIN") or REPO / "target/debug/pobr"))
    parser.add_argument("--oracle-cache", action="append", type=Path, default=[])
    parser.add_argument("--refresh-missing", action="store_true")
    parser.add_argument("--json", type=Path)
    parser.add_argument("--worst", type=int, default=3)
    args = parser.parse_args()
    catalog = catalog_stats((REPO / "crates/pobr-core/src/display_catalog.rs").read_text())
    sidebar = sidebar_stats((compare.VENDOR / "src/Modules/BuildDisplayStats.lua").read_text())
    files = build_files(args.paths or [REPO / path for path in DEFAULT_SETS])
    caches = [Path(os.environ.get("POBR_CACHE_DIR") or REPO / ".cache").resolve() / "pob2-oracle", *[p.resolve() for p in args.oracle_cache]]
    commit = compare.vendor_commit()
    builds, failures = [], []
    for index, code in enumerate(files, 1):
        print(f"output-coverage: {index}/{len(files)} {code.parent.name}", file=sys.stderr)
        try:
            builds.append(measure(args.binary.resolve(), code, catalog, caches, commit, args.refresh_missing))
        except (RuntimeError, ValueError) as error:
            failures.append({"build": str(code.parent.relative_to(REPO)), "error": str(error)})
            print(f"  {error}", file=sys.stderr)
    rows = [summarize(stat, key, builds, args.worst) for stat, key in catalog.items()]
    covered = sidebar & set(catalog.values())
    summary = {"builds": len(builds), "requested_builds": len(files), "failures": failures,
               "oracle_commit": commit, "catalog_stats": len(catalog),
               "sidebar_stats": len(sidebar), "sidebar_covered": len(covered),
               "missing_sidebar": sorted(sidebar - covered), "stats": rows,
               "oracle_caches": {build["build"]: build["oracle_cache"] for build in builds}}
    print(f"Sidebar: {len(covered)}/{len(sidebar)}; catalog: {len(catalog)}; builds: {len(builds)}/{len(files)}")
    print(f"{'Stat (PoB key)':58s} {'Both':>5s} {'Nonzero':>7s} {'<=5%':>10s} {'<=10%':>10s}")
    for row in rows:
        label = row["stat"] + (f" ({row['pob_key']})" if row["stat"] != row["pob_key"] else "")
        total = row["both"]
        fmt = lambda n: f"{n}/{total}" if not total else f"{n}/{total} {n / total:.0%}"
        print(f"{label:58s} {total:5d} {row['nonzero']:7d} {fmt(row['within_5pct']):>10s} {fmt(row['within_10pct']):>10s}")
        for pair in row["worst"]:
            error = pair["relative_error"]
            delta = f"{error:.1%}" if error is not None else "oracle zero"
            if "oracle_before_pool_cap" in pair:
                delta += f"; oracle before Spirit pool cap: {pair['oracle_before_pool_cap']:.6g}"
            print(f"  {pair['build']}: {pair['engine']:.6g} / {pair['oracle']:.6g} ({delta})")
    print("Missing sidebar stats: " + ", ".join(summary["missing_sidebar"]))
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(summary, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
