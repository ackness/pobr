#!/usr/bin/env python3
"""Partition every workspace package, keeping libtest fixtures and doctests."""

import argparse
from pathlib import Path
import subprocess
import tomllib


ROOT = Path(__file__).resolve().parents[2]
SHARDS = {
    "build": ("pobr-build", "pobr-cli"),
    "wasm": ("pobr-wasm",),
    "libraries-tools": (),  # All other workspace members, including new ones.
}


def packages(root=ROOT):
    workspace = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]
    return [tomllib.loads((root / member / "Cargo.toml").read_text())["package"]["name"]
            for member in workspace["members"]]


def select(shard, members):
    assigned = {name for group in SHARDS.values() for name in group}
    missing = assigned - set(members)
    if missing:
        raise ValueError(f"Rust shard references missing packages: {sorted(missing)}")
    return ([name for name in members if name not in assigned]
            if shard == "libraries-tools" else list(SHARDS[shard]))


def command(shard, members):
    return ["cargo", "test", "--locked",
            *[arg for name in select(shard, members) for arg in ("-p", name)]]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("shard", choices=SHARDS)
    args = parser.parse_args()
    # No target/test filters: each package retains all tests and doctests.
    raise SystemExit(subprocess.call(command(args.shard, packages()), cwd=ROOT))


if __name__ == "__main__":
    main()
