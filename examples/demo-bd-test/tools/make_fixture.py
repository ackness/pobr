#!/usr/bin/env python3
"""从 PoB2 build code 生成统一格式的测试 fixture。

用法:
    python3 make_fixture.py <fixture_dir>            # 读 <dir>/code.txt
    python3 make_fixture.py <fixture_dir> --code <c> # 直接传码并写入 code.txt

产物（写入 <fixture_dir>/）:
    code.txt     原始 PoB2 code（单行，URL-safe Base64(zlib(XML))）
    decoded.xml  解码后的 PathOfBuilding2 XML
    meta.json    结构化元数据：职业/升华/等级/主技能/技能组/全部 PlayerStat
                 （已存在的 meta.json 中 source / notes 字段会被保留）

与 Rust 侧对应: pobr-build::decode_pob_code（URL-safe Base64, padding 容错, zlib inflate）。
"""

import argparse
import base64
import hashlib
import json
import sys
import xml.etree.ElementTree as ET
import zlib
from pathlib import Path


def decode_pob_code(code: str) -> bytes:
    code = code.strip()
    pad = "=" * (-len(code) % 4)
    return zlib.decompress(base64.urlsafe_b64decode(code + pad))


def extract_meta(xml_bytes: bytes) -> dict:
    root = ET.fromstring(xml_bytes)
    build = root.find("Build")
    if build is None:
        raise ValueError("no <Build> element")

    meta = {
        "character": {
            "class": build.get("className"),
            "ascendancy": build.get("ascendClassName"),
            "level": int(build.get("level", "0")),
        },
        "pob": {
            "target_version": build.get("targetVersion"),
            "main_socket_group": int(build.get("mainSocketGroup", "0")),
        },
    }

    # 技能组（active SkillSet 中的 Skill 列表）
    skills_el = root.find("Skills")
    groups = []
    main_skill = None
    if skills_el is not None:
        active_set_id = skills_el.get("activeSkillSet", "1")
        skill_set = None
        for ss in skills_el.findall("SkillSet"):
            if ss.get("id") == active_set_id:
                skill_set = ss
                break
        if skill_set is None:
            skill_set = skills_el.find("SkillSet")
        if skill_set is not None:
            for idx, sk in enumerate(skill_set.findall("Skill"), start=1):
                gems = [
                    {
                        "name": g.get("nameSpec"),
                        "skill_id": g.get("skillId"),
                        "level": int(g.get("level", "0")),
                        "quality": int(g.get("quality", "0")),
                        "enabled": g.get("enabled") == "true",
                    }
                    for g in sk.findall("Gem")
                ]
                group = {
                    "index": idx,
                    "enabled": sk.get("enabled") == "true",
                    "main_active_skill": sk.get("mainActiveSkill"),
                    "gems": gems,
                }
                groups.append(group)
            msg = meta["pob"]["main_socket_group"]
            if 1 <= msg <= len(groups):
                g = groups[msg - 1]
                try:
                    mas = int(g["main_active_skill"] or "1")
                except ValueError:
                    mas = 1
                # mainActiveSkill 按“非 support gem”序号计；poe.ninja 导出把主动技能
                # 放在前面，这里取第 mas 个非 SupportGem 名称作为主技能。
                actives = [
                    gem["name"]
                    for gem in g["gems"]
                    if gem["skill_id"] and not gem["skill_id"].startswith("Support")
                ]
                if actives:
                    main_skill = actives[min(mas, len(actives)) - 1]
    meta["character"]["main_skill"] = main_skill
    meta["skill_groups"] = groups

    # 全量 PlayerStat（PoB 计算的黄金数值，测试断言用）
    stats = {}
    for ps in build.findall("PlayerStat"):
        name, value = ps.get("stat"), ps.get("value")
        try:
            stats[name] = float(value)
        except (TypeError, ValueError):
            stats[name] = value
    meta["player_stats"] = stats

    # 物品数量（粗略 sanity 信息）
    items_el = root.find("Items")
    meta["item_count"] = len(items_el.findall("Item")) if items_el is not None else 0
    return meta


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("fixture_dir", type=Path)
    ap.add_argument("--code", help="PoB2 code 字符串（省略则读 <dir>/code.txt）")
    args = ap.parse_args()

    d = args.fixture_dir
    d.mkdir(parents=True, exist_ok=True)
    code_file = d / "code.txt"
    if args.code:
        code_file.write_text(args.code.strip() + "\n")
    code = code_file.read_text().strip()

    xml_bytes = decode_pob_code(code)
    (d / "decoded.xml").write_bytes(xml_bytes)

    meta = {
        "name": d.name,
        "source": {
            "site": "poe.ninja",
            "url": None,
            "league": None,
            "game_version": None,
            "fetched_at": None,
        },
    }
    meta_file = d / "meta.json"
    if meta_file.exists():  # 保留手工维护字段
        old = json.loads(meta_file.read_text())
        for key in ("source", "notes"):
            if key in old:
                meta[key] = old[key]

    meta.update(extract_meta(xml_bytes))
    meta["pob"]["code_sha256"] = hashlib.sha256(code.encode()).hexdigest()
    meta["pob"]["code_chars"] = len(code)
    meta["pob"]["decoded_bytes"] = len(xml_bytes)

    meta_file.write_text(json.dumps(meta, ensure_ascii=False, indent=2) + "\n")
    c = meta["character"]
    print(
        f"{d.name}: {c['class']}/{c['ascendancy']} lv{c['level']} "
        f"main={c['main_skill']} groups={len(meta['skill_groups'])} "
        f"stats={len(meta['player_stats'])} xml={len(xml_bytes)}B"
    )


if __name__ == "__main__":
    main()
