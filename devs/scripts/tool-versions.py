#!/usr/bin/env python3
"""Resolve tool requests through mise.lock for official CI installers."""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tomllib


ROOT = Path(__file__).resolve().parents[2]
TOOLS = {
    "node": "node", "pnpm": "pnpm", "python": "python", "gh": "gh",
    "wasm_pack": "aqua:wasm-bindgen/wasm-pack", "actionlint": "actionlint",
}


def versions(config, rust, lock):
    if lock.get("lockfile_version") != 3:
        raise ValueError("mise.lock must use the supported lockfile version 3")
    result = {}
    for output, key in TOOLS.items():
        request = config["tools"][key]
        if not isinstance(request, str) or (request != "latest" and not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", request)):
            raise ValueError(f"{key} must request latest or an exact stable version, got {request!r}")
        entries = lock.get("tools", {}).get(key, [])
        if not isinstance(entries, list):
            raise ValueError(f"{key}: invalid mise.lock entries")
        matching = [entry for entry in entries if isinstance(entry, dict)
                    and isinstance(entry.get("specifiers"), list)
                    and request in entry["specifiers"]]
        if len(matching) != 1:
            raise ValueError(f"{key}: expected one mise.lock entry for {request!r}; run mise lock")
        resolved = matching[0].get("version")
        if not isinstance(resolved, str) or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", resolved):
            raise ValueError(f"{key}: locked version must be exact and stable, got {resolved!r}")
        if request != "latest" and resolved != request:
            raise ValueError(f"{key}: locked version {resolved!r} disagrees with {request!r}")
        result[output] = resolved
    toolchain = rust["toolchain"]
    result["rust"] = toolchain["channel"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", result["rust"]):
        raise ValueError("Rust must use an exact stable channel")
    result["rust_components"] = ",".join(toolchain["components"])
    result["rust_targets"] = ",".join(toolchain["targets"])
    result["rust_profile"] = toolchain.get("profile", "minimal")
    return result


COMMANDS = {
    "node": ["node", "--version"], "pnpm": ["pnpm", "--version"],
    "python": ["python3", "--version"], "gh": ["gh", "--version"],
    "wasm_pack": ["wasm-pack", "--version"], "actionlint": ["actionlint", "-version"],
    "rust": ["rustc", "--version"],
}


def check_active(values, run=subprocess.run):
    for key, command in COMMANDS.items():
        result = run(command, capture_output=True, text=True, check=True)
        found = re.search(r"[0-9]+\.[0-9]+\.[0-9]+", result.stdout + result.stderr)
        if found is None or found.group() != values[key]:
            raise ValueError(f"{key}: expected {values[key]}, got {found.group() if found else 'unknown'} from {shutil.which(command[0])}")
        print(f"{key}: {values[key]}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="reject active tools shadowing the declared pins")
    args = parser.parse_args()
    with (ROOT / "mise.toml").open("rb") as source:
        config = tomllib.load(source)
    with (ROOT / "rust-toolchain.toml").open("rb") as source:
        rust = tomllib.load(source)
    with (ROOT / "mise.lock").open("rb") as source:
        lock = tomllib.load(source)
    values = versions(config, rust, lock)
    if args.check:
        check_active(values)
        return
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.write("".join(f"{key}={value}\n" for key, value in values.items()))
    else:
        print(json.dumps(values, indent=2))


if __name__ == "__main__":
    main()
