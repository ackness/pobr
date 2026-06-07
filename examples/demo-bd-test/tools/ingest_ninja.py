#!/usr/bin/env python3
"""把 poe.ninja character API 抓取的 JSON 转成统一 fixture。

输入: 一个 JSON 文件，含 poe.ninja /poe2/api/builds/<snapshot>/character 响应的精简字段:
  account/name/league/class/level/lastSeenUtc/keystones/defensiveStats/skills/pob

产物: builds/<class>-<ascendancy>-<main-skill>/
  code.txt   = pob 码
  stats.json = poe.ninja Stats 页数据（defensiveStats + 技能 DPS + keystones）
  meta.json / decoded.xml = 由 make_fixture.py 生成
  notes.md   = 自动生成的摘要
"""

import json
import re
import sys
import urllib.parse
from datetime import date
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from make_fixture import decode_pob_code, extract_meta  # noqa: E402

HERE = Path(__file__).resolve().parent.parent  # demo-bd-test/


def slugify(s: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", (s or "").lower()).strip("-")


def main(char_json: Path) -> None:
    c = json.loads(char_json.read_text())
    code = c.pop("pob").strip()
    xml = decode_pob_code(code)
    m = extract_meta(xml)
    ch = m["character"]
    slug = "-".join(
        filter(None, [slugify(ch["class"]), slugify(ch["ascendancy"]), slugify(ch["main_skill"] or "unknown")])
    )
    d = HERE / "builds" / slug
    # 重名（同职业同主技能的不同角色）时追加序号，避免覆盖已有 fixture
    n = 2
    while d.exists() and (d / "code.txt").exists() and (d / "code.txt").read_text().strip() != code:
        d = HERE / "builds" / f"{slug}-{n}"
        n += 1
    slug = d.name
    d.mkdir(parents=True, exist_ok=True)

    (d / "code.txt").write_text(code + "\n")
    (d / "decoded.xml").write_bytes(xml)

    url = (
        "https://poe.ninja/poe2/builds/runesofaldur/character/"
        + urllib.parse.quote(c["account"]) + "/" + urllib.parse.quote(c["name"])
    )
    meta = {
        "name": slug,
        "source": {
            "site": "poe.ninja",
            "url": url,
            "account": c["account"],
            "character": c["name"],
            "league": c.get("league"),
            "game_version": "0.5.0",
            "fetched_at": date.today().isoformat(),
        },
    }
    meta.update(m)
    import hashlib

    meta["pob"]["code_sha256"] = hashlib.sha256(code.encode()).hexdigest()
    meta["pob"]["code_chars"] = len(code)
    meta["pob"]["decoded_bytes"] = len(xml)
    (d / "meta.json").write_text(json.dumps(meta, ensure_ascii=False, indent=2) + "\n")

    stats = {k: c.get(k) for k in ("defensiveStats", "skills", "keystones", "level", "class", "lastSeenUtc", "updatedUtc")}
    (d / "stats.json").write_text(json.dumps(stats, ensure_ascii=False, indent=2) + "\n")

    dps_lines = []
    for s in c.get("skills", []):
        for dd in s.get("dps", []):
            dps_lines.append(f"  - {dd['name']}: DPS {dd['dps']:,}（DoT {dd['dotDps']:,}）")
    ds = c.get("defensiveStats", {})
    notes = f"""# {ch['class']} / {ch['ascendancy']} — {ch['main_skill']}

- **来源**: [{c['account']} / {c['name']}]({url})
- **联赛**: {c.get('league')}（0.5.0 Runes of Aldur），抓取于 {date.today().isoformat()}
- **等级**: {ch['level']}
- **主技能组**: {ch['main_skill']}（详见 meta.json `skill_groups`）
- **防御**（poe.ninja Stats）: Life {ds.get('life')} / ES {ds.get('energyShield')} / EHP {ds.get('effectiveHealthPool')}，抗性 {ds.get('fireResistance')}/{ds.get('coldResistance')}/{ds.get('lightningResistance')}/{ds.get('chaosResistance')}
- **技能 DPS**（poe.ninja 计算）:
{chr(10).join(dps_lines) if dps_lines else '  - 无'}
- **Keystones**: {', '.join(c.get('keystones') or []) or '无'}
"""
    (d / "notes.md").write_text(notes)
    print(slug)


if __name__ == "__main__":
    main(Path(sys.argv[1]))
