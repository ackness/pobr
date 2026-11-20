#!/usr/bin/env python3
"""Diff two `data/<patch>/` directories and print a human-readable summary.

Two levels of detail:
  - Default (file summary): list added/removed files plus entry count and byte deltas.
        python3 -m pipeline.diagnostics.diff_data data/0.5.5.5 data/0.5.5.8
  - --semantic: keyed diff by domain (added, removed, and value-changed passives,
    skills, or modifiers).
        python3 -m pipeline.diagnostics.diff_data data/0.5.5.5 data/0.5.5.8 --semantic
        python3 -m pipeline.diagnostics.diff_data A B --semantic --domain tree --limit 40
        python3 -m pipeline.diagnostics.diff_data A B --semantic --json /tmp/diff.json

Designed after PoB2: the calc engine is version-independent, and only the data
pool and passive tree change across patches. Making semantic deltas explicit
shows which overlays or tests need follow-up when adopting a new patch.
"""

import argparse
import json
from pathlib import Path


def load(p):
    try:
        with open(p, encoding="utf-8") as f:
            return json.load(f)
    except Exception as e:
        return {"__error__": str(e)}


def count_entries(obj):
    """Estimate entry count: list -> len; dict excluding _meta -> top-level keys or max list len."""
    if isinstance(obj, list):
        return len(obj)
    if isinstance(obj, dict):
        body = {k: v for k, v in obj.items() if k != "_meta"}
        list_vals = [v for v in body.values() if isinstance(v, list)]
        if list_vals:
            return max(len(v) for v in list_vals)
        return len(body)
    return 0


def rel_files(root):
    return {str(p.relative_to(root)) for p in Path(root).rglob("*.json")}


# -- File summary (default backwards-compatible mode) --------------------------
def file_summary(old_root, new_root):
    old_files = rel_files(old_root)
    new_files = rel_files(new_root)
    added = sorted(new_files - old_files)
    removed = sorted(old_files - new_files)
    common = sorted(new_files & old_files)

    print(f"# Data diff: {old_root} -> {new_root}\n")
    print(f"- Old files: {len(old_files)}  New files: {len(new_files)}")
    print(f"- Added: {len(added)}  Removed: {len(removed)}  Common: {len(common)}\n")

    if added:
        print("## Added files")
        for f in added:
            print(f"  + {f}  ({count_entries(load(Path(new_root)/f))} entries)")
        print()
    if removed:
        print("## Removed files")
        for f in removed:
            print(f"  - {f}")
        print()

    print("## Common files: entry and byte deltas")
    rows = []
    for f in common:
        op = Path(old_root) / f
        npth = Path(new_root) / f
        rows.append(
            (f, count_entries(load(op)), count_entries(load(npth)),
             op.stat().st_size, npth.stat().st_size)
        )
    rows.sort(key=lambda r: abs(r[2] - r[1]), reverse=True)
    print(f"{'File':<48} {'Old':>8} {'New':>8} {'Delta':>7} {'Old KB':>8} {'New KB':>8}")
    for f, oc, nc, ob, nb in rows:
        d = nc - oc
        mark = "" if d == 0 and ob == nb else "  *"
        print(f"{f:<48} {oc:>8} {nc:>8} {d:>+7} {ob/1024:>8.0f} {nb/1024:>8.0f}{mark}")


# -- Semantic keyed diff -----------------------------------------------------
# (file, mode, container, key_field, compare_fields)
#   mode "list"      : top-level [ {key_field:..}, .. ]
#   mode "container" : top-level { container: [ {key_field:..}, .. ], _meta:.. }
#   mode "topdict"   : top-level { <key>: <value>, .., _meta:.. } (key is entry id)
# compare_fields=None compares the whole entry (used for topdict).
DOMAINS = {
    "tree": ("base/passive_tree.json", "list", None, "id",
             ["name", "kind", "stats"]),
    "mods": ("base/mods.json", "list", None, "id",
             ["mod_type", "domain", "generation_type", "level", "stats"]),
    "bases": ("base/base_items.json", "list", None, "id",
              ["name", "item_class", "tags", "implicits", "weapon", "armour", "spirit"]),
    "skill_levels": ("base/granted_effect_levels.json", "topdict", None, None, None),
    "skill_stats": ("base/granted_effect_stat_sets.json", "list", None, "effect_id",
                    ["sets"]),
    "special_mods": ("overlay/special_mods.json", "container", "entries", "id",
                     ["pattern", "mods", "vendor_pattern", "handler_id", "source_note"]),
    "uniques": ("overlay/uniques.json", "container", "uniques", "name",
                ["base", "item_type", "raw", "variants"]),
}


def extract(obj, mode, container, key_field):
    if isinstance(obj, dict) and "__error__" in obj:
        return None
    if mode == "list":
        seq = obj if isinstance(obj, list) else []
        return {e[key_field]: e for e in seq if isinstance(e, dict) and key_field in e}
    if mode == "container":
        seq = obj.get(container, []) if isinstance(obj, dict) else []
        return {e[key_field]: e for e in seq if isinstance(e, dict) and key_field in e}
    if mode == "topdict":
        return {k: v for k, v in obj.items() if k != "_meta"} if isinstance(obj, dict) else {}
    return {}


def keyed_diff(old, new, fields):
    ok, nk = set(old), set(new)
    added = sorted(nk - ok)
    removed = sorted(ok - nk)
    changed = []  # (key, [changed_field, ..] | None)
    for k in sorted(ok & nk):
        oe, ne = old[k], new[k]
        if fields is None:
            if oe != ne:
                changed.append((k, None))
        elif isinstance(oe, dict) and isinstance(ne, dict):
            diff = [f for f in fields if oe.get(f) != ne.get(f)]
            if diff:
                changed.append((k, diff))
        elif oe != ne:
            changed.append((k, None))
    return added, removed, changed


def short(v, n=140):
    s = json.dumps(v, ensure_ascii=False, sort_keys=True)
    return s if len(s) <= n else s[: n - 1] + "…"


def render_change(name, old_e, new_e, fields):
    """Compact diff formatting focusing on calc-relevant fields (stats / sets / mods)."""
    if fields is None:  # topdict full-value change
        ol = len(old_e) if hasattr(old_e, "__len__") else "?"
        nl = len(new_e) if hasattr(new_e, "__len__") else "?"
        return f"    ~ {name}  (entry value changed; size {ol}->{nl})"
    lines = [f"    ~ {name}  fields: {', '.join(fields)}"]
    for f in fields:
        ov, nv = old_e.get(f), new_e.get(f)
        lines.append(f"        {f}: {short(ov, 90)}  ->  {short(nv, 90)}")
    return "\n".join(lines)


def semantic_diff(old_root, new_root, only_domain, limit, json_out):
    report = {}
    print(f"# Semantic diff: {old_root} -> {new_root}\n")
    for dom, (rel, mode, container, key_field, fields) in DOMAINS.items():
        if only_domain and dom != only_domain:
            continue
        op, npth = Path(old_root) / rel, Path(new_root) / rel
        if not op.exists() or not npth.exists():
            continue
        old = extract(load(op), mode, container, key_field)
        new = extract(load(npth), mode, container, key_field)
        if old is None or new is None:
            print(f"## {dom}  ({rel}) -- skipped (failed to parse file)\n")
            continue
        added, removed, changed = keyed_diff(old, new, fields)
        report[dom] = {
            "file": rel,
            "added": added,
            "removed": removed,
            "changed": [{"key": k, "fields": fs} for k, fs in changed],
        }
        print(f"## {dom}  ({rel})")
        print(f"   old {len(old)} / new {len(new)} -- "
              f"added {len(added)}  removed {len(removed)}  changed {len(changed)}")
        if added:
            print(f"   + added {len(added)}:")
            for k in added[:limit]:
                print(f"      + {k}")
            if len(added) > limit:
                print(f"      ... {len(added) - limit} more")
        if removed:
            print(f"   - removed {len(removed)}:")
            for k in removed[:limit]:
                print(f"      - {k}")
            if len(removed) > limit:
                print(f"      ... {len(removed) - limit} more")
        if changed:
            print(f"   ~ changed {len(changed)}:")
            for k, fs in changed[:limit]:
                print(render_change(k, old[k], new[k], fs))
            if len(changed) > limit:
                print(f"      ... {len(changed) - limit} more")
        print()

    if json_out:
        Path(json_out).write_text(
            json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        print(f"(machine-readable diff written to {json_out})")


def main():
    ap = argparse.ArgumentParser(description="Diff two data/<patch>/ directories")
    ap.add_argument("old_root")
    ap.add_argument("new_root")
    ap.add_argument("--semantic", action="store_true",
                    help="Run keyed semantic diff by domain (added/removed/changed nodes, skills, modifiers)")
    ap.add_argument("--domain", choices=sorted(DOMAINS), default=None,
                    help="Diff only the selected domain")
    ap.add_argument("--limit", type=int, default=15,
                    help="Max examples to show per category (default 15)")
    ap.add_argument("--json", dest="json_out", default=None,
                    help="Also write the semantic diff as JSON")
    args = ap.parse_args()

    if args.semantic:
        semantic_diff(args.old_root, args.new_root, args.domain, args.limit, args.json_out)
    else:
        file_summary(args.old_root, args.new_root)


if __name__ == "__main__":
    main()
