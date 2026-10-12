#!/usr/bin/env python3
"""Keep only the most recent committed data versions.

The repository keeps the active version (data/CURRENT) plus the versions most
recently added to git, three in total by default. Version strings are not
compared: the international and Chinese clients number their patches
differently, so "recent" means the order the snapshots were committed.

Usage:
    python3 pipeline/prune-data.py            # list what would be removed
    python3 pipeline/prune-data.py --apply    # git rm the older versions
"""
import argparse
import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DATA = REPO / "data"


def added_at(version: str) -> int:
    out = subprocess.run(
        ["git", "log", "--diff-filter=A", "--format=%ct", "--", f"data/{version}/manifest.json"],
        cwd=REPO, capture_output=True, text=True, check=True,
    ).stdout.split()
    # Not committed yet: a snapshot that was just generated is the newest.
    return int(out[-1]) if out else 2**62


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--keep", type=int, default=3)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()

    current = (DATA / "CURRENT").read_text(encoding="utf-8").split()[0]
    versions = sorted(
        (p.name for p in DATA.iterdir() if p.name[0].isdigit() and (p / "manifest.json").is_file()),
        key=added_at, reverse=True,
    )
    keep = [current] + [v for v in versions if v != current][: max(args.keep - 1, 0)]
    drop = [v for v in versions if v not in keep]
    print("keep:  ", " ".join(keep))
    print("remove:", " ".join(drop) or "(nothing)")
    if args.apply:
        for version in drop:
            subprocess.run(["git", "rm", "-r", "-q", "--ignore-unmatch", f"data/{version}"], cwd=REPO, check=True)
            subprocess.run(["rm", "-rf", str(DATA / version)], check=True)
    elif drop:
        print("Run again with --apply to remove them.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
