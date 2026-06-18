#!/usr/bin/env python3
"""对比两个 data/<patch>/ 目录，产出人读 diff 摘要。

两种粒度：
  - 默认（文件级）：列文件增删 + 共有文件的条目数 / 字节变化。
        python3 pipeline/diff-data.py data/4.5.0.3.4 data/4.5.2.1.3
  - --semantic（语义级）：按域逐条 keyed-diff——哪个天赋节点/技能/词条
    新增·删除·**数值改动**，回答"某版本改了什么、怎么反映进计算"。
        python3 pipeline/diff-data.py data/4.5.0.3.4 data/4.5.2.1.3 --semantic
        python3 pipeline/diff-data.py A B --semantic --domain tree --limit 40
        python3 pipeline/diff-data.py A B --semantic --json /tmp/diff.json

设计参考 PoB2：calc 引擎版本无关，唯一随版本变的是「数据池 + 天赋树」。
本工具把"数据池跨版本的语义增量"显性化（PoB2 靠 export+CHANGELOG 获得的可见性），
是新版本快速迭代的输入：先看 diff，再决定哪些手工 overlay / 测试需要跟随。
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
    """估算条目数：list → len；dict 去掉 _meta 后 → 顶层 key 数或首个 list 值的 len。"""
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


# ── 文件级摘要（向后兼容的默认模式）─────────────────────────────────────────
def file_summary(old_root, new_root):
    old_files = rel_files(old_root)
    new_files = rel_files(new_root)
    added = sorted(new_files - old_files)
    removed = sorted(old_files - new_files)
    common = sorted(new_files & old_files)

    print(f"# 数据 diff: {old_root} → {new_root}\n")
    print(f"- 旧文件数: {len(old_files)}  新文件数: {len(new_files)}")
    print(f"- 新增文件: {len(added)}  删除文件: {len(removed)}  共有: {len(common)}\n")

    if added:
        print("## 新增文件")
        for f in added:
            print(f"  + {f}  ({count_entries(load(Path(new_root)/f))} 条)")
        print()
    if removed:
        print("## 删除文件")
        for f in removed:
            print(f"  - {f}")
        print()

    print("## 共有文件：条目数变化 / 字节变化")
    rows = []
    for f in common:
        op = Path(old_root) / f
        npth = Path(new_root) / f
        rows.append(
            (f, count_entries(load(op)), count_entries(load(npth)),
             op.stat().st_size, npth.stat().st_size)
        )
    rows.sort(key=lambda r: abs(r[2] - r[1]), reverse=True)
    print(f"{'文件':<48} {'旧条':>8} {'新条':>8} {'Δ条':>7} {'旧KB':>8} {'新KB':>8}")
    for f, oc, nc, ob, nb in rows:
        d = nc - oc
        mark = "" if d == 0 and ob == nb else "  *"
        print(f"{f:<48} {oc:>8} {nc:>8} {d:>+7} {ob/1024:>8.0f} {nb/1024:>8.0f}{mark}")


# ── 语义级 keyed-diff ────────────────────────────────────────────────────────
# (file, mode, container, key_field, compare_fields)
#   mode "list"      : 顶层就是 [ {key_field:..}, .. ]
#   mode "container" : 顶层 { container: [ {key_field:..}, .. ], _meta:.. }
#   mode "topdict"   : 顶层 { <key>: <value>, .., _meta:.. }（key 即条目 id，整值比对）
# compare_fields=None ⇒ 整条比对（topdict 用）。
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
    """单条改动的紧凑展示，聚焦 calc 相关字段（stats / sets / mods 等）。"""
    if fields is None:  # topdict 整值
        ol = len(old_e) if hasattr(old_e, "__len__") else "?"
        nl = len(new_e) if hasattr(new_e, "__len__") else "?"
        return f"    ~ {name}  (条目整值变化; 大小 {ol}→{nl})"
    lines = [f"    ~ {name}  字段: {', '.join(fields)}"]
    for f in fields:
        ov, nv = old_e.get(f), new_e.get(f)
        lines.append(f"        {f}: {short(ov, 90)}  →  {short(nv, 90)}")
    return "\n".join(lines)


def semantic_diff(old_root, new_root, only_domain, limit, json_out):
    report = {}
    print(f"# 语义 diff: {old_root} → {new_root}\n")
    for dom, (rel, mode, container, key_field, fields) in DOMAINS.items():
        if only_domain and dom != only_domain:
            continue
        op, npth = Path(old_root) / rel, Path(new_root) / rel
        if not op.exists() or not npth.exists():
            continue
        old = extract(load(op), mode, container, key_field)
        new = extract(load(npth), mode, container, key_field)
        if old is None or new is None:
            print(f"## {dom}  ({rel}) — 跳过（文件解析失败）\n")
            continue
        added, removed, changed = keyed_diff(old, new, fields)
        report[dom] = {
            "file": rel,
            "added": added,
            "removed": removed,
            "changed": [{"key": k, "fields": fs} for k, fs in changed],
        }
        print(f"## {dom}  ({rel})")
        print(f"   旧 {len(old)} 条 / 新 {len(new)} 条 — "
              f"新增 {len(added)}  删除 {len(removed)}  改动 {len(changed)}")
        if added:
            print(f"   ＋ 新增 {len(added)}:")
            for k in added[:limit]:
                print(f"      + {k}")
            if len(added) > limit:
                print(f"      … 余 {len(added) - limit} 条")
        if removed:
            print(f"   － 删除 {len(removed)}:")
            for k in removed[:limit]:
                print(f"      - {k}")
            if len(removed) > limit:
                print(f"      … 余 {len(removed) - limit} 条")
        if changed:
            print(f"   ～ 改动 {len(changed)}:")
            for k, fs in changed[:limit]:
                print(render_change(k, old[k], new[k], fs))
            if len(changed) > limit:
                print(f"      … 余 {len(changed) - limit} 条")
        print()

    if json_out:
        Path(json_out).write_text(
            json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        print(f"(机读 diff 已写入 {json_out})")


def main():
    ap = argparse.ArgumentParser(description="对比两个 data/<patch>/ 目录")
    ap.add_argument("old_root")
    ap.add_argument("new_root")
    ap.add_argument("--semantic", action="store_true",
                    help="按域逐条 keyed-diff（节点/技能/词条的增删改）")
    ap.add_argument("--domain", choices=sorted(DOMAINS), default=None,
                    help="只 diff 指定域")
    ap.add_argument("--limit", type=int, default=15,
                    help="每类（增/删/改）最多展示几条示例（默认 15）")
    ap.add_argument("--json", dest="json_out", default=None,
                    help="把语义 diff 同时写为机读 JSON")
    args = ap.parse_args()

    if args.semantic:
        semantic_diff(args.old_root, args.new_root, args.domain, args.limit, args.json_out)
    else:
        file_summary(args.old_root, args.new_root)


if __name__ == "__main__":
    main()
