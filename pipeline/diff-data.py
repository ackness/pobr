#!/usr/bin/env python3
"""对比两个 data/<patch>/ 目录，产出人读 diff 摘要。

用法: python3 pipeline/diff-data.py data/4.5.0.3.4 data/4.5.2.1.3
"""
import json
import sys
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
        # 常见形态：{"_meta":..., "<域>": [...]} —— 取最大的 list 值
        list_vals = [v for v in body.values() if isinstance(v, list)]
        if list_vals:
            return max(len(v) for v in list_vals)
        return len(body)
    return 0


def rel_files(root):
    return {
        str(p.relative_to(root))
        for p in Path(root).rglob("*.json")
    }


def main():
    old_root, new_root = sys.argv[1], sys.argv[2]
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
        oc = count_entries(load(op))
        nc = count_entries(load(npth))
        ob = op.stat().st_size
        nb = npth.stat().st_size
        rows.append((f, oc, nc, ob, nb))
    # 按条目数变化绝对值排序
    rows.sort(key=lambda r: abs(r[2] - r[1]), reverse=True)
    print(f"{'文件':<48} {'旧条':>8} {'新条':>8} {'Δ条':>7} {'旧KB':>8} {'新KB':>8}")
    for f, oc, nc, ob, nb in rows:
        d = nc - oc
        mark = "" if d == 0 and ob == nb else "  *"
        print(f"{f:<48} {oc:>8} {nc:>8} {d:>+7} {ob/1024:>8.0f} {nb/1024:>8.0f}{mark}")


if __name__ == "__main__":
    main()
