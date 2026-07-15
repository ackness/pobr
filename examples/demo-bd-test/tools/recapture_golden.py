#!/usr/bin/env python3
"""Re-capture parity fixture golden player_stats against the vendored PoB2.

The golden player_stats in each build's meta.json are the PoB2 numbers a build
was computed with. When the vendored PoB2 (and thus the game data version)
advances — e.g. 0.5.0 -> 0.5.4b — those goldens must be recomputed against the
new version, otherwise parity compares PoBR@new-data against PoB2@old-numbers.

This runs the headless oracle (tools/pob2-oracle/run.sh, at the currently
vendored PoB2 commit) on each fixture's decoded.xml and overwrites ONLY the
values of player_stats keys already tracked in meta.json — refreshing values,
never expanding the asserted key set. Keys the new version no longer computes
are dropped. All other meta.json fields and formatting are preserved.

Usage:
    python3 recapture_golden.py            # all fixtures under builds/
    python3 recapture_golden.py <name>...  # only the named fixture dirs

Reports per-build how many stats changed and the largest relative drift, so a
version migration can see exactly where PoB2 rebalanced. A build that fails to
load under the vendored version is left stale and flagged.
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve()
REPO = HERE.parents[3]
BUILDS = REPO / "examples/demo-bd-test/builds"
RUN = REPO / "tools/pob2-oracle/run.sh"


def oracle_main_output(xml: Path) -> dict:
    fd, out = tempfile.mkstemp(suffix=".json")
    os.close(fd)
    try:
        r = subprocess.run([str(RUN), str(xml), out], capture_output=True, text=True, timeout=180)
        if r.returncode != 0:
            raise RuntimeError(f"oracle exit {r.returncode}: {r.stderr.strip()[-300:]}")
        return json.load(open(out))["mainOutput"]
    finally:
        os.unlink(out)


def recapture(d: Path) -> str:
    meta = json.loads((d / "meta.json").read_text())
    gold = meta.get("player_stats", {})
    mo = oracle_main_output(d / "decoded.xml")
    dropped = [k for k in gold if k not in mo]
    for k in dropped:
        del gold[k]
    changed, max_key, max_drift = 0, None, 0.0
    for k, v in list(gold.items()):
        if isinstance(v, (int, float)) and isinstance(mo[k], (int, float)):
            nv = mo[k]
            if abs(v) > 1e-9 and abs(nv - v) / abs(v) > 0.001:
                changed += 1
                drift = (nv - v) / abs(v)
                if abs(drift) > abs(max_drift):
                    max_key, max_drift = k, drift
            gold[k] = nv
    meta["player_stats"] = gold
    (d / "meta.json").write_text(json.dumps(meta, ensure_ascii=False, indent=2) + "\n")
    md = f"{max_key} {max_drift * 100:+.1f}%" if max_key else "-"
    dd = f" | dropped {dropped}" if dropped else ""
    return f"{changed}/{len(gold)} changed, max drift {md}{dd}"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("names", nargs="*", help="fixture dir names (default: all)")
    args = ap.parse_args()
    dirs = (
        [BUILDS / n for n in args.names]
        if args.names
        else sorted(d for d in BUILDS.iterdir() if (d / "meta.json").exists())
    )
    failed = []
    for d in dirs:
        try:
            print(f"  ok  {d.name}: {recapture(d)}")
        except Exception as e:
            failed.append(d.name)
            print(f"  FAIL {d.name}: {e}")
    print(f"\n{len(dirs) - len(failed)} recaptured, {len(failed)} failed.")
    if failed:
        sys.exit(1)


if __name__ == "__main__":
    main()
