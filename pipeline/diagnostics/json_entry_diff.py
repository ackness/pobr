#!/usr/bin/env python3
"""Entity-level diff of two extract-lua JSON outputs, for the vendor-delta report.

Every top-level list (except `_meta`) is treated as an entry collection; the
diff reports added / removed / changed entries *by identity*, so a game patch's
overlay churn reads as "these rules/uniques/special-mods moved", not a wall of
JSON line hunks. Emits markdown to stdout.

Usage:  json_entry_diff.py <old.json> <new.json>
        json_entry_diff.py --selftest
"""
import hashlib
import json
import sys

# Identity key priority — first present string/int field names the entry.
# Covers the shapes we diff: special_vendor(`id`), uniques(`name`),
# parser-rules forms(`pattern`)/name_map(`phrase`)/...
ID_KEYS = ["id", "name", "phrase", "pattern", "key", "text", "stat", "line", "raw"]
NAME_CAP = 40  # ponytail: cap listed names; raise if a section needs the full set


def identity(entry):
    if isinstance(entry, dict):
        for k in ID_KEYS:
            v = entry.get(k)
            if isinstance(v, (str, int)):
                return str(v)
        return "hash:" + hashlib.sha1(json.dumps(entry, sort_keys=True).encode()).hexdigest()[:12]
    return str(entry)


def canon(entry):
    return json.dumps(entry, sort_keys=True)


def collections(doc):
    """Top-level lists keyed by name; a bare list doc becomes `(root)`."""
    if isinstance(doc, list):
        return {"(root)": doc}
    if isinstance(doc, dict):
        return {k: v for k, v in doc.items() if k != "_meta" and isinstance(v, list)}
    return {}


def diff_list(old, new):
    om, nm = {}, {}
    for e in old:
        om.setdefault(identity(e), e)
    for e in new:
        nm.setdefault(identity(e), e)
    added = sorted(k for k in nm if k not in om)
    removed = sorted(k for k in om if k not in nm)
    changed = sorted(k for k in nm if k in om and canon(nm[k]) != canon(om[k]))
    return added, removed, changed


def fmt(names):
    shown = names[:NAME_CAP]
    more = len(names) - len(shown)
    s = ", ".join("`%s`" % n for n in shown)
    if more > 0:
        s += f" … (+{more} more)"
    return s


def render(old, new):
    oc, nc = collections(old), collections(new)
    lines = []
    for k in sorted(set(oc) | set(nc)):
        a, r, c = diff_list(oc.get(k, []), nc.get(k, []))
        if not (a or r or c):
            continue
        lines.append(f"**`{k}`** — +{len(a)} added, -{len(r)} removed, ~{len(c)} changed")
        if a:
            lines.append(f"- added: {fmt(a)}")
        if r:
            lines.append(f"- removed: {fmt(r)}")
        if c:
            lines.append(f"- changed: {fmt(c)}")
        lines.append("")
    return "\n".join(lines) if lines else "_No entry-level changes._"


def selftest():
    old = {"_meta": {"vendor_commit": "aaa"}, "forms": [
        {"pattern": "keep", "form": "BASE"},
        {"pattern": "gone", "form": "INC"},
        {"pattern": "edit", "form": "BASE"},
    ]}
    new = {"_meta": {"vendor_commit": "bbb"}, "forms": [
        {"pattern": "keep", "form": "BASE"},
        {"pattern": "edit", "form": "MORE"},   # changed
        {"pattern": "fresh", "form": "BASE"},  # added
    ]}
    a, r, c = diff_list(old["forms"], new["forms"])
    assert a == ["fresh"], a
    assert r == ["gone"], r
    assert c == ["edit"], c
    # _meta churn must not register as a change.
    assert "No entry-level changes" not in render(old, new)
    assert render({"_meta": {"x": 1}}, {"_meta": {"x": 2}}) == "_No entry-level changes._"
    print("selftest OK")


def main():
    if len(sys.argv) >= 2 and sys.argv[1] == "--selftest":
        return selftest()
    if len(sys.argv) != 3:
        sys.exit("usage: json_entry_diff.py <old.json> <new.json> | --selftest")
    with open(sys.argv[1]) as f:
        old = json.load(f)
    with open(sys.argv[2]) as f:
        new = json.load(f)
    print(render(old, new))


if __name__ == "__main__":
    main()
