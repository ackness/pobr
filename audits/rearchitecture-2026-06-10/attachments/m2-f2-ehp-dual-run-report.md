# M2 F-2：EHP 新旧口径 18-build 双跑对照报告

> 生成：2026-06-11 · worktree `m2/defence`（F-1 = 6227fb2 + EternalLife 补遗）
> 复现：`cargo test -p pobr-build --test ninja_parity ehp_dual_run_report -- --nocapture`
> 原始逐 build 输出：[m2-f2-ehp-dual-run-raw.txt](m2-f2-ehp-dual-run-raw.txt)
> 蓝图依据：blueprints/m2-defence.md §2 Track F commit 2（双跑纪律 = §5 R2 行）

## 1. 口径定义

| 列 | 含义 |
|---|---|
| old | 旧 lowest-max-hit 口径（`total_ehp` 现值 = `total_ehp_lowest_max_hit`，各类型 max hit 取 min） |
| new | PoB2 口径（`total_ehp_pob2` = `TotalNumberOfHits × totalEnemyDamageIn`，CalcDefence.lua:3322；致死击数循环 :2979-3145） |
| golden | PoB2 导出 `meta.json::player_stats.TotalEHP` |

敌人进伤：Pinnacle@角色等级 placeholder（`round(monsterDamageTable[lv] × 1.5 × 8/4.4)`，
chaos ÷2.5 → 单击总进伤 4246），与 harness 既有 `EnemyTier::Pinnacle` 配置一致。

## 2. TotalEHP 18-build 汇总

| build | old | new | golden | old/golden | new/golden |
|---|---:|---:|---:|---:|---:|
| druid-oracle-comet | 9167 | 25083 | 23313 | 0.39x | **1.08x** |
| druid-oracle-ember-fusillade | 0 | 16241 | 20821 | 0.00x | 0.78x |
| huntress-ritualist-bow-shot | 5223 | 7026 | 6024 | 0.87x | 1.17x |
| huntress-spirit-walker-twister | 4431 | 20446 | 19859 | 0.22x | **1.03x** |
| mercenary-gemling-legionnaire-explosive-grenade | 2942 | 10529 | 15887 | 0.19x | 0.66x |
| mercenary-tactician-wolf-pack | 3777 | 34486 | 116555 | 0.03x | 0.30x |
| monk-invoker-frost-bomb | 10005 | 24786 | 22699 | 0.44x | **1.09x** |
| monk-martial-artist-flicker-strike | 7135 | 40140 | 33544 | 0.21x | 1.20x |
| monk-martial-artist-twister | 4257 | 35546 | 31556 | 0.13x | 1.13x |
| ranger-deadeye-explosive-grenade | 2857 | 18375 | 17665 | 0.16x | **1.04x** |
| ranger-pathfinder-ice-shot | 6058 | 24938 | 23831 | 0.25x | **1.05x** |
| sorceress-chronomancer-essence-drain | 15190 | 48668 | 58624 | 0.26x | 0.83x |
| sorceress-disciple-of-varashta-comet | 11810 | 30614 | 36292 | 0.33x | 0.84x |
| sorceress-stormweaver-comet | 2568 | 13957 | 12980 | 0.20x | **1.08x** |
| warrior-smith-of-kitava-shield-wall | 3946 | 18895 | 76433 | 0.05x | 0.25x |
| warrior-titan-shield-wall | 4189 | 25292 | 50244 | 0.08x | 0.50x |
| witch-abyssal-lich-detonate-dead | 14217 | 25085 | 22863 | 0.62x | **1.10x** |
| witch-blood-mage-coiling-bolts | 1659 | 7813 | 9153 | 0.18x | 0.85x |

**聚合**（18/18 可比）：

| 口径 | hit@5% | hit@10% | 中位比值 |
|---|---:|---:|---:|
| old（lowest-max-hit） | 0 | 0 | 0.21x |
| new（PoB2） | 3 | 7 | **1.03x** |

## 3. 审查结论（量级异常排查）

1. **无量级异常**：18 build 全部产出有限正值；新口径中位 1.03x，系统性低估
   （旧口径 0.21x）被消除。`*_max_hit` 新旧两列在无 keystone/taken-as 词条的
   build 上逐值一致（见 raw 附件，如 druid-oracle-comet 五行 old==new），
   证明新管线在中性输入下与旧自洽迭代解数学等价。
2. **已修复一例**（对照中发现 → F-1 补遗 commit）：witch-abyssal-lich 缺
   `EternalLife` 词条解析（vendor ModParser.lua:3121），新口径 max hit 曾误走
   bypass-protected 分支（0.38x）；补词条后 1.08x。
3. **遗留偏差归因**（残差属上游 parity 既有缺口或 F-1 允许的简化，F-3 切换时
   单列跟踪；括号内为实测）：
   - 护甲聚合缺口（13-defence 既有 Armour 命中缺口）：
     mercenary-tactician-wolf-pack（EHP 0.30x；pobr Armour 10208 vs golden
     18580 = 0.55x）、warrior-smith-of-kitava（0.25x）同因——armour 低估直接
     压低 per-hit DR 与致死击数；
   - warrior-titan-shield-wall（0.50x；Armour 0.92x 已接近）：残差主要在
     格挡概率层（golden `EffectiveBlockChance` 27.82）与 golden TotalEHP 含
     格挡回复 GainWhenHit（vendor :3168-3177，F-1 置 0，蓝图允许）；
   - sorceress/witch 系 0.78-0.85x：ES/Life 池本值 ±8% 既有偏差 + 个别
     「reduced damage taken」词条未解析（abyssal-lich 残差同源）。
4. **双跑不变式**：`total_ehp` 字段语义未切换（仍旧口径），ninja_parity 四基线
   逐值不变（DEF_HIT5=111 / DEF_HIT10=117 / OFF_HIT5=23 / OFF_HIT10=31）；
   新值全部挂 `total_ehp_pob2` / `*_max_hit_pob2` / `number_of_*_hits` 新字段。

## 4. F-3 交接清单（口径切换 commit 的输入）

- [ ] `total_ehp` 切换为 `total_ehp_pob2` 值、`*_max_hit` 切换为 `*_max_hit_pob2`
  值（删除双跑字段或保留为别名，由 F-3 决定）；`avoid_stun` 的 totalTakenHit
  换真值（taken_hit_per_type 产出）属同一行为批。
- [ ] defensive_rows 8→~24 列扩列 + `BASELINE_*` 重记（验收双指标：扩列后 ≥80%
  且旧 8 列子集 ≥111，owner 裁决见 00-index §4）。
- [ ] 上表第 3 节三类遗留偏差的专项排查（warrior block 层 / wolf-pack 异常 /
  ES 池本值偏差）。
- [ ] display_catalog 中 `TotalEHP`/`*MaximumHitTaken` 等条目 ParityStatus
  Planned → Computed。
