# M4-G 技能伤害量级特化——勘察结论与剩余缺口登记

> 来源：W5 报告 R3「若干 build DPS 量级差 0.05-0.6x，根因 = 技能特化机制缺失」。
> 本波（M4-G，2026-06-13）按 ninja 命中收益勘察四类、修复两类；其余按本表登记。
> 勘察方法：`tools/pob2-oracle` dump vendor 中间值（summedBase / intermediates /
> skillInfo）对照 PoBR `OutputTable::damage_components` 逐分量定位。

## 0. 已修复（本波 commit）

| 机制 | build | TotalDPS before→after（effective） | commit |
|---|---|---|---|
| 尸体爆炸基伤（`explodeCorpse` + `corpseExplosionLifeMultiplier` × `monsterLifeTable[enemyLevel]`，CalcOffence.lua:2211-2217） | witch-abyssal-lich-detonate-dead | 0.05x → **0.83x**（panel 0.09x → 1.09x） | 搬迁 `e742300` + enemyLevel `f7dface` + 消费 `a06b833` |
| grenade 二次起爆（`GrenadeActivateTwice` → DPS 末端倍率，CalcOffence.lua:1124-1127 / :4407） | ranger-deadeye-explosive-grenade | 0.68x → **1.02x** | `27e6fba` |
| 同上（部分收益） | mercenary-gemling-legionnaire-explosive-grenade | 0.13x → 0.20x（剩余见 §3） | `27e6fba` |

DD 剩余 0.83x 差 = effective 口径减伤乘区全局问题（PoB2 EFFECTIVE 的 EffMult
已折进 AverageHit，PoBR 另乘一道 mitigation；panel 口径 1.09x），非尸体机制本身，
归全局 effective 对齐线，不在本表。

## 1. witch-blood-mage-coiling-bolts（TotalDPS 0.09x）——非「bolt 数」缺口

**勘察结论（oracle 钉值）**：W5 报告的「多段/多发（bolt 数）」假设**不成立**。
vendor 主技能 = 选中 statSet 1（Physical），AvgHit 206919 = 单次施放口径，
无 bolt 数乘子。真实差距构成：

- **CritChance 32.41 vs 72.45（0.45x）**：vendor `IncCritChance 383`，PoBR 聚合
  不足（来源待逐 mod 定位；属暴击聚合线，非技能特化）。CritMultiplier 5.34 已逐位对齐。
- **per-hit 量级 ~0.39x**：vendor SummedBase（Physical 2840-4260 / Chaos 1840-2760）
  显著高于 gem 基伤（phys 1136-1704 / set2 chaos 994-1846）——多出部分 =
  **added damage 通道**（Blood Mage 升华「Skills gain added Physical Damage equal
  to % of Life Cost」族 + 装备 added chaos to spells × addedMult），并叠加
  `DamageGainAs_Physical 150 / DamageGainAs_Chaos 162`（vendor intermediates）。
  PoBR 当前 gain-as 仅 13-16% 档接入，150/162 档来源（升华/notable/支援）未注入。
- **statSet 2（Chaos bolt）的基伤未被 merge**（vendor 选中 set1 时 set2 仅 global
  merge，CalcActiveSkill.lua:124-140——但 oracle Chaos SummedBase 1840 来自 added，
  非 set2 基伤；PoBR 行为与 vendor 此点一致，不是缺口）。

**登记**：① Blood Mage 升华 life-cost→added-phys 机制（数据源 = 升华树 stat，
vendor ModParser/树侧）；② 大档 gain-as 来源核查（oracle `intermediates.DamageGainAs_*`
对拍）；③ 暴击聚合差（IncCritChance 383 vs PoBR 实聚合）。三者皆非 per-skill
特化，归 M4 暴击/聚合线。

## 2. twister（huntress 0.27x / monk 0.22x）——非「hitFrequency」缺口

**勘察结论**：`twister_hit_interval_ms 660`（act_dex.lua:9921）在 vendor
**无任何消费方**（SkillStatMap.lua / CalcOffence.lua 均无 hit_interval 映射；
hitTimeOverride/maxHitRatePerEnemy 不涉 Twister）。golden TotalDPS = AvgDamage ×
Speed（攻速桶），无命中频率乘子——W5 的「持续多段（twister 命中频率）」假设不成立。
真实差距构成（huntress 例）：

- **CritChance 34.08 vs 65.07**：vendor `PreEffectiveCritChance 40.90` →
  final 65.07，中间有 **crit 幸运/条件抬升**（lucky crit 族）；PoBR 34.08 接近
  pre-effective 档，缺的是 effective 抬升段（vendor CalcOffence crit 幸运分支）。
- **pre-crit per-hit ~0.66x**：vendor `IncDamage 350` + `DamageGainAs Chaos 27 /
  Fire 16 / Lightning 10`；PoBR inc/gain-as 聚合缺口待逐 mod 对拍（twister
  `baseMultiplier` 分等级值已入库且已消费，排除该因）。
- `twister_damage_+%_final_per_whirling_slash_stage 80`（statMap → Damage MORE ×
  Multiplier:WhirlwindStages）：build config 无 WhirlwindStages 输入，vendor 同样
  0 层零贡献——非本两 build 的差距来源（但该 statMap Multiplier 通道本身未接，
  其它 build 可能踩中，登记）。

**登记**：① crit 幸运/condition 抬升段（vendor CalcOffence.lua:3700 附近暴击二次
检定族）；② per-skill statMap 的 `Multiplier:<Var>` tag 条目（如
WhirlwindStages per-stage MORE）在 PoBR statmap 引擎的 tag 翻译白名单核查。

## 3. 投掷/齐射速率族（mercenary grenade 0.20x、druid ember-fusillade 0.12x）

- **mercenary-gemling-legionnaire-explosive-grenade**（修后 0.20x）：
  - per-hit 0.36x：vendor AvgDamage 116832 @ gem lv24 q31 + 支援
    （Vorana's Siege / Payload / Innervate…）；PoBR 42493。差距主体在 added/
    gain-as（vendor `DamageGainAs_Cold 46 / Lightning 28`）与 q31 quality 段。
  - Speed 0.26 vs 0.29（0.90x）：grenade 冷却模型——Payload
    `base_cooldown_speed_+% -70`（sup_dex.lua:3577，statmap → CooldownRecovery INC）
    PoBR 的 `CooldownRecovery` 不在 statmap ModName 直通族 → 该条目当前
    UnknownModName 丢弃；Vorana's Siege 的冷却/弹数改写亦未建模。**注意**：
    单独接通 -70% 会让 rate 偏离 golden（vendor 链含 Vorana 抵消项），须整链
    一起做（vendor CalcOffence.lua:2858-3007 cooldown-governed speed 段对拍）。
  - vendor 行号：CalcOffence.lua:1124-1127（已修）、:2858-3007（cooldown speed）、
    SkillStatMap.lua:2798-2800（support_grenade_damage_+%_final）。
- **druid-oracle-ember-fusillade**（0.12x）：oracle 钉值 vendor CritChance **100**
  （CritMult 1.69——Oracle 升华/Spellslinger 链的必暴机制）、Speed 5.4 vs PoBR
  4.875、pre-crit per-hit ~0.29x（`IncDamage 409` + `DamageGainAs_Cold 46.7` +
  `ElementalDamageGainAs_Cold 31` + `More 1.23`）。`ember_fusillade_damage_+%_
  final_per_ember_fired`（act_int.lua qualityStats/constantStats）在 vendor
  **无 statMap 消费**（与 twister hit_interval 同类「数据存在但 vendor 不算」），
  非缺口。差距主体 = 必暴条件链 + 聚合，无 per-skill 数据可补，归 crit/聚合线。

## 4. 方法论沉淀

- 「技能特化」四类中两类（twister hitFrequency、coiling bolts 多发）经 oracle
  证伪——**vendor 对这些 stat 根本不消费**；伤害量级差的大头反复落在三条全局线：
  ① crit 条件/幸运抬升；② added damage / gain-as 大档来源；③ effective 减伤乘区。
  后续波次建议先做这三条线，再回头看单技能。
- oracle 用法：`/Users/…/pobr/tools/pob2-oracle/run.sh <decoded.xml> out.json`
  （worktree 无 vendor 检出时用主仓的 run.sh，路径自解析）；`summedBase` 直接
  暴露 base 段缺口（如 DD 的 Physical 4548 = 32956×0.138 一眼钉死）。

## 5. M4-H 暴击条件/幸运抬升线——结果（2026-06-13）

§1-§3 登记的暴击项全部经 oracle 逐段定位并修复（4 个独立行为 commit），
CritChance/CritMultiplier 列收敛（effective 口径）：

| build | 段位定位 | before → after | commit |
|---|---|---|---|
| witch coiling-bolts | 非「INC 聚合」主体——**底材覆盖缺路**：Blood Mage『Sunder the Flesh』`CritChanceBase` OVERRIDE 15（ModParser.lua:5801 / CalcOffence.lua:3667-3676）替换 gem 底材 7 | CritChance 0.45x → **0.96x ✓** | `2a40269` |
| monk twister | 纯聚合：树 notable『Struck Through』"Attacks have +1%..."（`^attacks have ` 前缀类 ModParser.lua:1266）此前 Unsupported | CritChance 0.94x → **1.00x ✓✓** | `2a40269` |
| druid ember-fusillade | 必暴链：Oracle『Forced Outcome』(tree 55135) "Inevitable Critical Hits" → flag（ModParser.lua:3393，消费 CalcOffence.lua:3712-3739 已有） | CritChance 0.15x → **1.00x ✓**、CritMult 2.06x → 1.03x ✓ | `326b4e4` |
| huntress twister | effective 抬升 = **BifurcateCrit**（非 lucky；CalcOffence.lua:3707-3710 + 爆伤 :3796-3811），源 = Garukhan's Resolve support 隐式 stat `attacks_roll_crits_twice`（sup_dex.lua:2421 → SkillStatMap.lua:1011-1013）——statmap flag 白名单 + SkillType tag + 隐式 stat overlay 通道三段接通 | CritChance 0.56x → 0.92x、CritMult 0.89x → 0.98x ✓ | `8012014` |
| huntress twister | 聚合②：油涂 `{enchant}Allocates Vulgar Methods`（ModParser.lua:5809 → CalcSetup.lua:1322-1331 notableMap）授予 30 INC | CritChance 0.92x → **1.00x ✓✓**、CritMult → 1.00x ✓✓ | `a1db2df` |

聚合面：offensive parity @5% 28/80 → **40/80**、defensive 374/450 → 377/450
（油涂节点的非 crit 词条连带）；TotalDPS：witch 0.09→0.15x、druid 0.12→0.15x、
huntress 0.27→0.44x、monk 0.22→0.23x——余差均为 pre-crit per-hit 段
（added/gain-as 大档线 + Speed 0.90x），非暴击线范围。

## 6. M4-J IncDamage 聚合差线——结果（2026-06-13）

§1-§3 登记的 pre-crit per-hit「IncDamage 聚合差」经 oracle 逐 mod 对拍
（vendor `skillModList:Tabulate("INC", cfg, "Damage")` 全来源行 vs PoBR ModDb
同名族聚合），按缺口类别 4 个独立 commit 修复：

| 缺口类别 | 定位（vendor 行号） | 受益 build | commit |
|---|---|---|---|
| **PerStat 资源分母接线 bug**（W-A3 登记项坐实）：编排 6c 用 `base_sum`（BASE-only）回填 cfg.multipliers 的 Mana/Life，vendor PerStat 读 actor **output**（ModStore.lua:440-460 GetStat，floor(stat/div)） | `3% increased Spell Damage per 100 maximum Mana`（Tree:19044）：vendor 234=3×floor(7889/100)，PoBR 旧值 93=3×floor(3100/100) | ember-fusillade（234✓ 逐位）、coiling-bolts（105✓ 逐位） | `4a936fc` |
| **parser 短语缺路**：per-ailment-type 整行（ModParser.lua:3798-3804，The Taming +42）、companion 后缀（:1803）、arcane surge 后缀（:1817）+ gain-FLAG form（:92/:4197/:1902）、One/Two-Handed Weapons 位（:1016/:1018）、ProjectileSpeed 名解锁 | 此前全部硬 ParseError 结构性丢弃 | twister、ember-fusillade | `8cd1f03` |
| **编排条件桥**：`Condition:ArcaneSurge` flag → `AffectedByArcaneSurge`（CalcDefence.lua:1580-1582）；`CompanionInPresence` 默认条件（ConfigOptions.lua:1012-1014 defaultState=true，按 SkillType.CreatesCompanion 在场） | ember Tree:27388→16940（+30）；twister WildProtector→37769（+10） | 同上 | `0f70253` |
| **隐式 stat→statmap flag→消费 三段**：`projectile_speed_additive_modifiers_also_apply_to_projectile_damage`（sup_dex.lua:4353 / SkillStatMap.lua:888 / CalcOffence.lua:840-845）→ INC ProjectileSpeed 复制为 Damage INC(Projectile) | twister 树投速小点 +31（vendor +23，差 8 属 options 变体线） | twister | `c894891` |

**IncDamage 聚合收敛**（vendor `Sum("INC", cfg, "Damage")` 等价口径）：
- ember-fusillade：~203 → **409 / vendor 409 逐位 ✓**（TotalDPS 69969→87040，0.17→0.21x，余差=Speed 4.875 vs 5.4 + per-hit gain-as 大档线）
- coiling-bolts：~296 → **301 / vendor 316**（余差 = 13724 暂缓 15 + witch 变体 +4——见下）
- huntress twister：~240 → **333 / vendor 350**（余差 = 34168 暂缓 25 + 1420 暂缓 15 − 变体线 ~23；TotalDPS 35943→46499，0.43→0.56x）

**暂缓登记（vendor 证实正确、但接入触发既有过记出 parity 带；代码内同步注释）**：
- ~~`attack/spell area damage`（ModParser.lua:721-722，deadeye 树 41 INC、twister 1420 +15）：接入令 deadeye TotalDPS 1.02x→1.11x 出带——根因 grenade **Speed 段 1.95x 过记**（0.32 vs 0.16，冷却线 §3），冷却线修复后启用~~ → **M4-K 已解锁**（j3 冷却整链修复后接入，5 build 纯收敛：deadeye TotalDPS 0.52x→0.57x、twister 0.53x→0.55x、gemling 0.27x→0.28x、smith-of-kitava 0.57x→0.69x、titan 0.61x→0.72x，零倒退）；
- `CritInPast8Sec` 后缀族（ModParser.lua:1904-1906，twister 34168 +25 / coiling+DD 13724 +15）：接入令 detonate-dead **panel** 口径 1.09x→1.13x 出 10% 带（panel 无敌方减伤本就过记；effective 口径实为 0.81→0.84 收敛）——effective 减伤线收敛 DD 后启用。

**确认非缺口**：witch 的 Bonded `20% increased Projectile Damage`（gloves enchant）vendor 同样不计（无「Gain the benefits of Bonded modifiers」激活源，oracle 钉值 flag=false），PoBR 行为一致。

**剩余登记（暴击线尾差，单一根因）**：
- **切换类节点 class 变体（isSwitchable options）未建模**——tree.lua 节点的
  `options.<Class>` 子表会按职业整体替换 stats：witch 51335『Affliction
  Enforcer』→ Witch 变体 64801『Jagged Shards』（+20 INC crit for spells，
  vendor 有、PoBR 缺 → witch 0.96x 的余差）；druid 6898『Relentless
  Vindicator』→ Druid 变体 7197『Guardian of the Wilds』（无 crit，PoBR 误用
  基础版 +10 INC → druid pre-effective 15.19 vs 14.49 过聚合，喂进必暴
  roll-down 使 CritMult 1.74 vs 1.69）。属树数据 schema（PassiveNodeDef 无
  options 字段）+ 适配器改造，归数据线独立做。
