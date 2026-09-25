#!/usr/bin/env python3
"""Record COMBAT offence references without changing EFFECTIVE golden values."""
import argparse
import base64
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import zlib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vendor-root", type=Path, required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[3]
    vendor = args.vendor_root.resolve()
    version = re.search(
        r'GOLDEN_PARITY_DATA_VERSION: &str = "([^"]+)"',
        (repo / "crates/pobr-data/src/lib.rs").read_text(encoding="utf-8"),
    ).group(1)
    rules = json.loads((repo / f"data/{version}/overlay/mod_parser_rules.json").read_text(encoding="utf-8"))
    commit = rules["_meta"]["vendor_commit"]
    actual = subprocess.check_output(["git", "-C", str(vendor), "rev-parse", "HEAD"], text=True).strip()
    if actual != commit:
        raise RuntimeError(f"Expected vendor {commit}, got {actual}")
    subprocess.run(["git", "-C", str(vendor), "diff", "--exit-code", "HEAD"], check=True)
    lua = os.environ.get("LUAJIT") or shutil.which("luajit")
    if not lua:
        raise RuntimeError("luajit is required")
    env = dict(os.environ, CI="true", LUA_PATH=(
        f"{repo}/tools/pob2-oracle/shim/?.lua;{vendor}/runtime/lua/?.lua;"
        f"{vendor}/runtime/lua/?/init.lua;./?.lua;;"
    ))
    result = {"data_version": version, "vendor_commit": commit, "mode": "COMBAT", "builds": {}}
    with tempfile.TemporaryDirectory(prefix="pobr-panel-golden-") as temp:
        for fixture in sorted((repo / "examples/demo-bd-test/builds").iterdir()):
            if not (fixture / "meta.json").exists():
                continue
            meta = json.loads((fixture / "meta.json").read_text(encoding="utf-8"))
            code = (fixture / "code.txt").read_text(encoding="utf-8").strip()
            digest = hashlib.sha256(code.encode()).hexdigest()
            if digest != meta["pob"]["code_sha256"]:
                raise RuntimeError(f"Stale fixture metadata: {fixture.name}")
            xml = zlib.decompress(base64.urlsafe_b64decode(code + "=" * (-len(code) % 4)))
            if xml != (fixture / "decoded.xml").read_bytes():
                raise RuntimeError(f"Stale decoded XML: {fixture.name}")
            output = Path(temp) / "output.json"
            subprocess.run([
                lua, str(repo / "tools/pob2-oracle/panel.lua"),
                str(fixture / "decoded.xml"), str(output),
            ], cwd=vendor / "src", env=env, stdin=subprocess.DEVNULL, check=True, timeout=120)
            oracle = json.loads(output.read_text(encoding="utf-8"))
            keys = meta["player_stats"].keys() & {"CritChance", "CritMultiplier", "Speed", "AverageDamage", "TotalDPS"}
            for key in keys:
                value = oracle["effective"][key]
                for reference in (meta["player_stats"].get(key), oracle["calcsEffective"].get(key)):
                    if reference is None or not math.isclose(value, reference, rel_tol=1e-8, abs_tol=1e-8):
                        raise RuntimeError(f"Reference/skill mismatch: {fixture.name}/{key}: {value} vs {reference}")
            stats = {key: oracle["panel"][key] for key in keys}
            result["builds"][fixture.name] = {"code_sha256": digest, "player_stats": stats}
            print(f"Recorded {fixture.name}", flush=True)
    target = repo / "examples/demo-bd-test/panel-golden.json"
    target.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
