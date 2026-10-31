#!/usr/bin/env python3
"""Read-only, standard-library source/fixture receipts; not a Rust assembler replay."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

PLAYER_MANIFEST = "c0412e13d92141476cae28d7dc9b58db9ec74cc82c8084847af6fed62f141bd5"
PLAYER_LOCK = "973c6658dbb822649c09320c32808c3e8f69ee97c2f35f72c94ded54d30eb6cc"
PARENT_MANIFEST = "56e71e00a72c8079d100b64861fc030206b12aaeb5ad655ccb4ab4e6726332d0"
PARENT_LOCK = "d7a59a052109f411a62b1398ac844ff31ca8afd6d4158e3488446a19cb75743f"
REVISION = "cf190d05b09b7aac17e484107cd8152b5ae00fa1"
ROOT = Path(__file__).resolve().parents[3]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repoe", type=Path, required=True)
    parser.add_argument("--player", type=Path, required=True)
    parser.add_argument("--parent", type=Path, required=True)
    args = parser.parse_args()
    manifest_bytes = (args.player / "manifest.json").read_bytes()
    assert sha(manifest_bytes) == PLAYER_MANIFEST
    m = json.loads(manifest_bytes)
    retained = (ROOT / "tools/pobr-data-adapter/tests/fixtures/native-facts/player-context-manifest.json").read_bytes()
    assert retained == manifest_bytes
    assert len(m["files"]) == 7
    assert {p.name for p in args.player.iterdir()} == {"manifest.json", *m["files"]}
    for name, fp in m["files"].items():
        p = args.player / name
        assert p.is_file() and not p.is_symlink()
        b = p.read_bytes()
        assert len(b) == fp["bytes"] and sha(b) == fp["sha256"]
    assert sha((args.player / "source-lock.json").read_bytes()) == PLAYER_LOCK
    parent_bytes = (args.parent / "manifest.json").read_bytes()
    assert sha(parent_bytes) == PARENT_MANIFEST
    assert (args.player / "parent-manifest.json").read_bytes() == parent_bytes
    assert sha((args.parent / "source-lock.json").read_bytes()) == PARENT_LOCK
    parent = json.loads(parent_bytes)
    assert m["parent"]["source_identity"] == parent["source_identity"]
    for field in ("game", "realm", "game_patch"):
        assert m["source_identity"][field] == parent["source_identity"][field]
    assert len(m["producer"]) == 14
    for name, expected in m["producer"].items():
        blob = subprocess.check_output(["git", "-C", str(args.repoe), "show", f"{REVISION}:{name}"])
        assert sha(blob) == expected, name
    decoder_pin = json.loads((ROOT / "tools/pobr-data-adapter/tests/fixtures/native-facts/pypoe-sources.json").read_bytes())
    assert len(m["decoder"]["sources"]) == 117
    assert m["decoder"]["sources"].keys() == decoder_pin["sources"].keys()
    for name, actual in m["decoder"]["sources"].items():
        assert actual in decoder_pin["sources"][name], name
    files = json.loads((args.player / "metadata.json").read_bytes())["files"]
    assignments = records = 0
    for path, f in files.items():
        raw = b"\xff\xfe"
        for i, r in enumerate(f["records"], 1):
            assert r["line"] == i and r["byte_start"] == len(raw)
            raw += r["literal"].encode("utf-16-le")
            remaining = r["byte_end"] - len(raw)
            raw += {0: b"", 2: "\n".encode("utf-16-le"), 4: "\r\n".encode("utf-16-le")}[remaining]
        assert len(raw) == f["bytes"] == m["source_identity"]["raw_resources"][path]["bytes"]
        assert sha(raw) == m["source_identity"]["raw_resources"][path]["sha256"]
        for a in f["assignments"]:
            assert raw[a["value_byte_start"]:a["value_byte_end"]].decode("utf-16-le") == a["value_raw"]
            assert a["scope"] == f["records"][a["line"] - 1]["scope"]
        assignments += len(f["assignments"])
        records += len(f["records"])
    c = json.loads((args.player / "player_context.json").read_bytes())
    physical = {name: json.loads((args.parent / f"tables/{name}.json").read_bytes()) for name in ("Characters", "PassiveSkills", "Stats", "ClassPassiveSkillOverrides")}
    physical.update({name: json.loads((args.player / f"{name}.json").read_bytes()) for name in ("StartingPassiveSkills", "KeywordPopups")})
    refs = []

    def visit(value):
        if isinstance(value, dict):
            if "origin" in value:
                local = value["table"] in ("StartingPassiveSkills", "KeywordPopups")
                assert value["origin"] == ("player" if local else "parent")
                expected_payload = f'{value["table"]}.json' if local else f'tables/{value["table"]}.json'
                assert value["payload"] == expected_payload
                row = physical[value["table"]]["rows"][value["row"]]
                assert row["_row"] == value["row"] and row["values"].get("Id") == value["id"]
                assert value["pointer"] == f'/rows/{value["row"]}/values'
                refs.append(value)
            for nested in value.values():
                visit(nested)
        elif isinstance(value, list):
            for nested in value:
                visit(nested)

    visit(c)
    assert c["character"]["source"]["row"] == 7
    assert c["character"]["values"] == physical["Characters"]["rows"][7]["values"]
    assert c["character"]["values"]["Id"] == "Metadata/Characters/Int/IntFourb"
    assert c["character"]["values"]["AOFile"] == "Metadata/Characters/Int/IntFourB.ao"
    assert not c["selected_class_overrides"]
    assert len(c["all_starting_relations"]) == 6
    assert sum(len(r["options"]) for r in c["all_starting_relations"]) == 12
    assert [r["row"] for r in c["starting_relation"]["options"]] == [2779, 1146]
    policy = json.loads((ROOT / "tools/pobr-data-adapter/src/native_facts/runtime/player-stat-policy.json").read_bytes())
    assert len(c["metadata_stats"]) == 99 and {s["key_raw"] for s in c["metadata_stats"]} == policy.keys()
    assert all(p["reason"] and p["disposition"] in {"consumed", "inactive", "outside-output-scope", "unresolved"} for p in policy.values())
    for stat in c["metadata_stats"]:
        a = files[stat["path"]]["assignments"][stat["assignment"]]
        assert a["plain_stats_scope"]
        for field in ("key_raw", "value_raw", "scope"):
            assert stat[field] == a[field]
    critical = [s for s in c["metadata_stats"] if s["key_raw"] == "base_critical_hit_damage_bonus"]
    assert len(critical) == 1 and float(critical[0]["value_raw"]) == 100
    lock = json.loads((args.player / "source-lock.json").read_bytes())
    selected = lock["sources"][m["source_identity"]["source"]]
    assert sha(canonical(selected)) == m["source_identity"]["selected_source_sha256"]
    print(f"PASS: 8-file package; 14 cf Git fingerprints; 117 decoder pins; exact parent/locks; {records} lines/{assignments} spans; {len(refs)} origin-qualified refs; 99 policy entries")
    print("UNRUN: Rust compilation/tests and new v2 assembler replay; no source/raw acquisition review claimed")


if __name__ == "__main__":
    main()
