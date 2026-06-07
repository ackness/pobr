# demo-bd-test —— 真实 PoB2 构筑码测试样本

来自 [poe.ninja/poe2/builds](https://poe.ninja/poe2/builds/runesofaldur) 的真实构筑，用于 `pobr-build` 解码 / 解析 / 计算 parity 测试。

当前样本：**Runes of Aldur 联赛（0.5.0）天梯 top-100**，8 个基础职业 × 每职业 ≥2 个不同升华，共 17 个 + 1 个旧样本，抓取于 2026-06-08（poe.ninja snapshot `1609-20260607-56460`）。

## 目录结构

```
demo-bd-test/
├── ninja-bd-deadeye.txt          # 旧版扁平样本（多个测试 include_str! 引用，勿动）
├── ninja-bd-marial-artist.txt    # 同上（其结构化版本 = builds/monk-martial-artist-flicker-strike）
├── raw/                          # poe.ninja character API 原始抓取 JSON（可重新 ingest）
├── tools/
│   ├── make_fixture.py           # code → decoded.xml + meta.json 生成器
│   └── ingest_ninja.py           # poe.ninja 抓取 JSON → 完整 fixture（含 stats.json/notes.md）
└── builds/                       # 统一格式样本，每个构筑一个子文件夹
    └── <class>-<ascendancy>-<main-skill>/
        ├── code.txt              # 原始 PoB2 code（单行，URL-safe Base64(zlib(XML))）
        ├── decoded.xml           # 解码后的 PathOfBuilding2 XML
        ├── meta.json             # 结构化元数据（见下）
        ├── stats.json            # poe.ninja Stats 页原始数据（防御数值 + 技能 DPS）
        └── notes.md              # 人工可读摘要：来源 URL、技能组、关键数值
```

## meta.json 字段

| 字段 | 说明 |
|------|------|
| `name` | 与文件夹同名 |
| `source` | 抓取来源：site / url / account / character / league / game_version / fetched_at |
| `character` | class / ascendancy / level / main_skill（自动从 XML 提取） |
| `pob` | target_version / main_socket_group / code_sha256 / code_chars / decoded_bytes |
| `skill_groups` | active SkillSet 的全部技能组及 gem（name / skill_id / level / quality / enabled） |
| `player_stats` | `<Build>` 内全部 `PlayerStat`——**PoB2（Lua）算出的黄金数值**（TotalDPS、Life、抗性等），parity 断言的目标值，不是 pobr 当前实现的输出 |
| `item_count` | `<Items>` 内 Item 数量 |

`stats.json` 与 `player_stats` 的区别：前者是 poe.ninja 站点展示的计算结果（含 EHP、各类型最大承伤、压制/闪避等防御口径和逐技能 DPS），后者是 PoB2 导出码内嵌的全量 PlayerStat。两者可互为校验。

## 用法

```bash
# 测试中引用
include_str!("…/builds/<name>/code.txt")   # 断言值取同目录 meta.json 的 player_stats

# Rust 侧解码
cargo run -p pobr-cli -- decode-code "$(cat builds/<name>/code.txt)"

# 重新生成（meta 的 source/notes 不会丢）
python3 tools/make_fixture.py builds/<name> --code "<pob2_code>"

# 从 poe.ninja 原始 JSON 重新 ingest（raw/ 下有本次抓取的原始数据）
python3 tools/ingest_ninja.py <character.json>
```

## 样本清单（2026-06-08，Runes of Aldur）

| fixture | 职业/升华 | lv | 主技能 |
|---------|----------|----|--------|
| warrior-titan-shield-wall | Warrior/Titan | 100 | Shield Wall |
| warrior-smith-of-kitava-shield-wall | Warrior/Smith of Kitava | 100 | Shield Wall |
| ranger-deadeye-explosive-grenade | Ranger/Deadeye | 99 | Explosive Grenade |
| ranger-pathfinder-ice-shot | Ranger/Pathfinder | 99 | Ice Shot |
| witch-abyssal-lich-detonate-dead | Witch/Abyssal Lich | 98 | Detonate Dead |
| witch-blood-mage-coiling-bolts | Witch/Blood Mage | 98 | Coiling Bolts |
| sorceress-stormweaver-comet | Sorceress/Stormweaver | 98 | Comet |
| sorceress-chronomancer-essence-drain | Sorceress/Chronomancer | 98 | Essence Drain |
| sorceress-disciple-of-varashta-comet | Sorceress/Disciple of Varashta | 98 | Comet |
| monk-martial-artist-twister | Monk/Martial Artist | 100 | Twister |
| monk-invoker-frost-bomb | Monk/Invoker | 98 | Frost Bomb |
| mercenary-tactician-wolf-pack | Mercenary/Tactician | 98 | Wolf Pack（CombinedDPS=0，主输出为召唤物） |
| mercenary-gemling-legionnaire-explosive-grenade | Mercenary/Gemling Legionnaire | 98 | Explosive Grenade |
| huntress-spirit-walker-twister | Huntress/Spirit Walker | 99 | Twister |
| huntress-ritualist-bow-shot | Huntress/Ritualist | 99 | Bow Shot |
| druid-oracle-ember-fusillade | Druid/Oracle | 98 | Ember Fusillade |
| druid-oracle-comet | Druid/Oracle | 98 | Comet（Spellslinger 图腾流） |
| monk-martial-artist-flicker-strike | Monk/Martial Artist | 98 | Flicker Strike（旧样本 = ninja-bd-marial-artist.txt） |

注：0.5 中 Disciple of Varashta 是 **Sorceress** 升华（XML className 为准）；Spirit Walker 属 Huntress、Oracle 属 Druid。
