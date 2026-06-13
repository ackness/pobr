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
- ~~`CritInPast8Sec` 后缀族（ModParser.lua:1904-1906，twister 34168 +25 / coiling+DD 13724 +15）：接入令 detonate-dead **panel** 口径 1.09x→1.13x 出 10% 带（panel 无敌方减伤本就过记；effective 口径实为 0.81→0.84 收敛）——effective 减伤线收敛 DD 后启用~~ → **M4-K 已解锁**（effective 口径 8 build 纯收敛：DD TotalDPS 0.80x→0.83x、coiling 0.81x→0.84x、twister 0.55x→0.58x、dot 列 twister 0.96x 新入列 3→4；panel 口径 DD 1.12x 出带按预案登记已审查例外、PANEL_OFF_HIT10 41→40，见 ninja_parity.rs 基线注释）；

**确认非缺口**：witch 的 Bonded `20% increased Projectile Damage`（gloves enchant）vendor 同样不计（无「Gain the benefits of Bonded modifiers」激活源，oracle 钉值 flag=false），PoBR 行为一致。

## 7. M4-K 异常 DoT 量级残差线——结果与归属表（2026-06-13）

j1 揭示的两个 dot 目标经 oracle 逐因子分解定位并修复（2 个行为 commit），
剩余残差逐项归属到非 dot 自身的线：

### 7.1 druid-oracle-comet TotalDotDPS 1.17x 高估（基线例外 1）——根因闭合

oracle 钉值 vendor 链：`IgniteDPS 182.74 = FireStoredCritAvg(13993.42) × 0.2 ×
ailmentStacks(0.1088) × effMult(0.6)`。PoBR/vendor 逐因子比：

| 因子 | PoBR | vendor | 比 | 归属 |
|---|---|---|---|---|
| **stacks 速率源** | 1.62（fill 本地链） | 0.6377（Speed） | **2.54x 过记 ← 主因** | **本波修复**：`effective_action_rate` 改读 offence 合并 `action_rate`（= vendor `globalOutput.Speed`，:5046-5053 stacks 速率源；fill 本地 `calc_skill_use_time` 链缺 `apply_total_time`（TotalCastTime）与 typed bucket/MORE，法术丢宝石施法时间） |
| Stored crit 量级（chance 同源连带） | 10860.37 | 13993.42 | 0.776²（chance+magnitude 双进） | 暴击线：CritMultiplier 4.07 vs 5.24（Malice buff 爆伤载荷未入聚合，§5 尾差族） |
| duration | 4.0 | 4.3478（=4/0.92） | 0.92 | curse 线：Temporal Chains（Blasphemy）→ enemy `BuffExpireFaster MORE -8`（act_int.lua:21308 statmap）→ vendor `debuffDurationMult = 1/max(0.25, mod(enemyDB,"BuffExpireFaster"))`（CalcOffence.lua:1833-1835/:5040）。`translate_curse_mod_name` 已显式列为待消费名 |
| effMult | 0.5 | 0.6 | 0.833 | 副技能 debuff 线：Frost Bomb 对敌 `−10 全元素抗`（oracle resistMods `Skill:FrostBombPlayer`）——PoBR 无「其它主动技能对敌施 debuff」通道 |

修后 1.17x → **0.45x**（高估消除；0.776² × 0.92 × 0.969(速度残差) × 0.833
= 0.45 逐因子闭合 ✓）。commit `1d9fdda`（速率信号）。

### 7.2 huntress-ritualist-bow-shot poison 全缺——链路打通（0.00x → 0.09x）

vendor 链 oracle 钉值：PoisonChance 100、PoisonStacksMax 4、MagnitudeEffect
3.04、Duration 3.2、PoisonDPS 131474（+ Bleed ≈ 11622 = TotalDot 143096）。
PoBR 三段结构性丢弃全部打通（commit `310f5b1`）：statmap 异常族白名单
（PoisonChance/BleedChance/AilmentMagnitude/<Ailment>Stacks/Enemy*Duration
归一/CanStack flag）+ `ailment_scoped_cfg`（vendor dotCfg keyword，:5005）+
叠层 flag 门/Override/MORE（:5021-5025）+ duration MORE 腿（:5037-5039）。
poison_dps 0→8779、bleed_dps 0→4430（Bleed III 连带）。

**余差 = mod_parser 短语缺路（本波禁动，移交 parser 线）**，oracle Tabulate
逐 mod 清单（全部 Tree/Item 来源）：

| 短语（vendor ModParser.lua 行号） | 词条 → vendor mod | 缺额 |
|---|---|---|
| `to poison on hit`（:836 名表）/ 装备『26% chance to Poison on Hit with Attacks』+ 树 8×5 | `PoisonChance BASE` | chance 60 → 100 |
| `magnitude of poison/ailments you inflict`（:785/:787-788 名表） | `AilmentMagnitude INC`（树 9 条 Σ130） | magnitude ×1 → ×3.04 |
| `poison duration`（:837 → `EnemyPoisonDuration`） | 树 7 条 + 弓 −25 | duration 2.0 → 3.2 |
| `targets can be affected by +N of your poisons at the same time`（:3895 → `PoisonCanStack` flag + `PoisonStacks BASE`） | 弓符文 +1、Tree:15986/63759 各 +1 | maxStacks 1 → 4 |
| `to inflict bleeding on hit`（:844 名表，树/装备侧） | `BleedChance BASE` | bleed 树侧补全 |

注：statmap 侧通道（Escalating Poison/Deadly Poison/Envenom 等 support 载荷）
已全部可走——本 build 主技能恰好不带这些 support，载荷全在树/装备（parser 域）。

### 7.3 残差归属表（dot 列全 18 build，修后实测）

dot 列命中 3/37 维持（修复消除的是**伪差/高估**，新增暴露的低估归属各自线）：

| build | dot 比（修前→修后） | 异常侧自身残差 | 击中侧/其它线残差 |
|---|---|---|---|
| druid-oracle-comet | 1.17x → 0.45x | 无（链路闭合 §7.1） | 暴击量级（Malice）、curse duration、Frost Bomb debuff |
| huntress-ritualist-bow-shot | 0.00x → 0.09x | 无（链路闭合 §7.2） | parser 短语 ×5 族（§7.2 表） |
| sorceress-varashta-comet | 0.62x → 0.32x | 无（速率过记伪高被修正） | 法术击中量级线（TotalDPS 0.19x 同源）+ 同 comet 的 curse/crit 族 |
| ranger-pathfinder-ice-shot | 0.28x → 0.27x | 无 | 击中量级线（TotalDPS 0.31x 同源，h 波 Stored 已对齐结构） |
| witch-abyssal-lich-DD | 0.68x → 0.67x | 无 | 尸爆基伤 effective 减伤线（§0 登记） |
| witch coiling-bolts | 0.79x → 0.62x | 无（旧 0.79 含速率伪高） | 击中量级（added/gain-as 大档线 §1） |
| sorceress-stormweaver-comet | 0.00x（不变） | **点燃链未触发**——vendor TotalDotDPS 1911 全为 ignite；M4-m oracle 对拍已闭环根因（见 §7.4），修复归 statmap Buff 域 | — |
| warrior-titan-shield-wall | 0.02x（不变） | vendor 5776 主体为 bleed（树侧 chance/magnitude 词条，parser 域） | — |
| 其余（grenade×2/frost-bomb/flicker/twister×2/kitava/ember） | 0.19x–0.77x | 无独立异常侧缺口 | 各自击中量级/速度线（off 列同源） |

**本波线内可继续项（登记）**：① ~~vendor `rateMod`（`<Ailment>Faster/Slower`，
:5036）PoBR 仅 finalize 段近似，statmap 名未开白~~ **M4-m 已闭环**（commit
`742a6c3`：三名开白 + ailment_rate_mod 补 INC 腿）；② `debuffDurationMult`
消费点在 ailment duration（curse 通道就绪后一并接，见 §7.1）；③ ~~stormweaver-
comet ignite 0 值对拍~~ **M4-m 对拍已闭环**（见 §7.4，修复归 statmap Buff 域）。

### 7.4 stormweaver-comet ignite 0 值——oracle 对拍结论（M4-m，2026-06-13）

vendor IgniteDPS 1910.98 的全链路已钉死（oracle dump + 逐 mod Tabulate）：

- **来源伤害**：`IgniteColdMin/Max 8673/13006` + `IgniteLightningMin/Max 433/648`
  ——ignite 来源**全为冷/电**（`IgniteFireMin/Max = 0`，build 无火伤）。跨类型
  通行证 = `ColdCanIgnite` + `LightningCanIgnite` FLAG，`Tabulate` 钉源 =
  `Skill:PinnacleOfPowerPlayer`：武器 Adonia's Ego（Siphoning Wand）`Grants
  Skill: Pinnacle of Power`（other.lua:12503，fromItem buff 技能）。其 statmap
  条目 `elemental_power_elemental_damage_+%_final_per_power_charge` 载荷 =
  `Damage MORE`（`Multiplier{RemovablePowerCharge, scalar=
  ConsumedPowerChargeEffect}`）+ 六枚 `<El>Can<Ailment>` flag，**全带
  `GlobalEffect/Buff` tag**——vendor 经 buff 循环写全局（CalcPerform buff 分支）。
- **几率派生**：`IgniteChanceOnHit 3.96 / OnCrit 13.85` = 阈值缩放
  `hitAvg/enemyThreshold × IgniteChanceMultiplier(20)`（CalcOffence.lua:5405-
  5432，无任何 flat IgniteChance 词条）——PoBR 同构机制已有
  （`ailment::ignite_traced` + `threshold_chance_traced`）。
- **PoBR 缺口（单点）**：`stored_source_at_roll`/`cross_type_source_hit` 的
  `<Type>Can<Ailment>` 旗标消费已就绪，缺的只是 **Pinnacle of Power buff 载荷
  注入**——归 statmap Buff 域（本波禁动）。注意两个子障碍：① 该条目首元素
  `Damage MORE` 带 `scalar` Multiplier（引擎 `collect_element` 现对 scalar 整条
  Unsupported——flag 元素会被连坐丢弃，需元素级拆分或 scalar 支持）；② 注入
  作用域 = 全局 buff（vendor GlobalEffect/Buff 通道，对位 PoBR buff_skill_specs
  注入面）。接通后该 build TotalDotDPS 0.00x 预期直接回 ~0.9x 带
  （effMult 0.66/duration 4s/StackPotential 0.37 等中间值 PoBR 机制均已同构）。

**M4-n 已闭环（2026-06-13）**：① 子障碍①经 `map_player_buff_stat` **逐元素
独立处置**（vendor merge 循环对 modOrGroup 本就逐元素，CalcActiveSkill.lua:96-117
无成组耦合；scalar `Damage MORE` 维持零注入 + 全败 Unsupported 上报不退化）；
② 子障碍②走既有 socket-group 扫描（PoB 把 `Grants Skill` 写成
`source="Item:…"` 组）——`buff_skill_specs` 非 aura/curse 分支补 Buff 载荷产线
（`player_buff_stat_modifiers` + 新 flag 通道 `<Type>Can<Ailment>` 族允收）→
`BuffSpec(kind=Buff)` → buff_pass Buff 分支入 player db。连带补 parser 名族
`flammability magnitude`/`chance to ignite` → `EnemyIgniteChance`（vendor
ModParser.lua:773-777；oracle Tabulate 钉源 Tree:38068『Elemental Ailment
Chance』24 INC——§7.4 原文「无任何 flat IgniteChance 词条」漏看了该 INC 腿）。
**收敛**：TotalDotDPS 0.00x → **0.83x**、CombinedDPS 0.83x → **0.90x ✓ 新命中**；
其余 17 build 全列零变动。余差单源 = 击中量级线（PoBR ignite 来源
cold 8053/12077 vs vendor 8673/13006 = 0.93x，与 TotalDPS 0.91x 同根因，
量级线已另册登记；几率经阈值缩放二次折入故 DPS ≈ 0.93²）。

**剩余登记（暴击线尾差，单一根因）**：
- **切换类节点 class 变体（isSwitchable options）未建模**——tree.lua 节点的
  `options.<Class>` 子表会按职业整体替换 stats：witch 51335『Affliction
  Enforcer』→ Witch 变体 64801『Jagged Shards』（+20 INC crit for spells，
  vendor 有、PoBR 缺 → witch 0.96x 的余差）；druid 6898『Relentless
  Vindicator』→ Druid 变体 7197『Guardian of the Wilds』（无 crit，PoBR 误用
  基础版 +10 INC → druid pre-effective 15.19 vs 14.49 过聚合，喂进必暴
  roll-down 使 CritMult 1.74 vs 1.69）。属树数据 schema（PassiveNodeDef 无
  options 字段）+ 适配器改造，归数据线独立做。

## 7. M4-K grenade per-hit 校正——结果（2026-06-13）

j3 例外注释预告的「per-hit 缺口 = 宝石等级 gating」路径，两段修复
（commit `5ebb1a0` 等级链 + `20e5bdb` 油涂品质）：

| 段 | 根因 | 修复 | oracle 双证 |
|---|---|---|---|
| 等级链 | Wave9 对 `skill_types[Grenade]` 暂关 `+N to Level of all <X> Skills`（当时 Speed ×1.95 双计会放大）；j3 修 Speed 后该 gating 反成主缺口 | 解除 gating（vendor applyGemMods 无 grenade 特例）。聚合路径本就正确：gemling 20+4=24（项链 +2 Projectile + 树 +2；Weapon 1 Swap 槽 +3 Attack vendor 不计、PoBR 无 swap 槽天然一致）、deadeye 21+6=27（武器 +3 Attack + 项链 +3 Projectile） | GemLevel 24 / 27 逐位 ✓ |
| Gemling 品质 +5 | **非升华节点**：j3 疑为 Gemling 升华，实为项链 enchant『Allocates Paragon』授予的油涂池 notable 20686（`+5% to Quality of all Skills` + `+5 to all Attributes`）。双段缺路：① GGG data.json 不含主图外油涂 notable（整池 27 个缺失）；② `gem_property_bonuses` 只扫 `tree.allocated_nodes`，授予节点漏扫 | ① `pobr-data-adapter --tree-anoints <tree.lua>` 回填通道（4844→4871 节点）；② GemProperty 扫描纳入 `granted_passive_defs`（与 append_granted_passives 共享解析，幂等去重） | quality 31 逐位 ✓ |

**收敛（golden 比值）**：
- deadeye：TotalDPS/CombinedDPS 0.52x → **0.82x**（Speed 1.00x ✓ 保持）
- gemling：TotalDPS/CombinedDPS 0.27x → **0.37x**；Speed 0.97→1.00x ✓ 新命中、Mana 0.94→0.95 ✓ 新命中
- 油涂池连带（全部朝 golden）：twister Speed 0.91→0.97x ✓ 新命中 + TotalDPS 0.26→0.27、stormweaver-comet 0.64→0.67、ember-fusillade 0.20→0.21
- 聚合：def@5% 377→379、core@5% 131→132、off@5% 41→42

**剩余登记（deadeye 未回 1.0x 带，per-hit ~0.82x）**：
- §6 暂缓项「attack/spell area damage」（ModParser.lua:721-722，deadeye 树 41 INC）
  的启用前提『grenade Speed 段冷却线修复后』**已满足**（Speed 1.00x ✓、双计已拆），
  归 parser 短语线启用（vendor IncDamage 371，该项即占 41）；
- 其余为伤害向量量级线（deadeye 链 = Convert Fire→Lightning 100% + gain-as
  45/20/5 大档，vendor mainHandOutput Lightning HitAverage 91407 占主导），
  待 per-hit 逐分量对拍（oracle dump 已留 /tmp 复现命令：`tools/pob2-oracle/run.sh
  examples/demo-bd-test/builds/ranger-deadeye-explosive-grenade/decoded.xml`）。
- dashboard `AverageDamage` 行恒等式 `dps/action_rate` 对 grenade build 含
  GrenadeActivateTwice ×1.5 端因子（golden 的 AverageDamage 不含），该行比值带
  结构性 1.5x 偏置（deadeye 显示 1.22x = 真实 per-hit 0.82x × 1.5）——读数注意，
  列口径是否拆端因子归 dashboard 线裁决。

baseline 按惯例本波不动（所有列 ≥ 基线），回升入带后随主线合并 bump。

## 8. M4-L 副技能 debuff/buff 注入面——结果（2026-06-13）

§7.1 归属表中「Frost Bomb debuff」与「CritMult（原误记 Malice 爆伤载荷）」两
因子经 oracle 逐 mod 对拍闭合（3 个独立行为 commit）：

| 因子 | 根因（oracle 钉值） | 修复 | commit |
|---|---|---|---|
| effMult 0.833（Frost Bomb −10/−12 全元素抗） | vendor buff 循环遍历**全部** activeSkillList（CalcPerform.lua:1847），Debuff 分支 :2219-2285 把 GlobalEffect Debuff 载荷（`active_skill_all_elemental_exposure_magnitude` → `<El>Exposure BASE 20`，SkillStatMap.lua:1721-1725）写 enemyDB，再经 "Apply exposures"（:3214-3247）折 `<El>Resist BASE -magnitude`（boss ExposureEffectOnSelf ×0.5）。PoBR 无非主技能对敌 debuff 通道 | statmap Debuff 域（`map_debuff_stat`，曝光族允收）+ buff_skill_specs Debuff 分支（全组扫描）+ 曝光归约收口 env_finalize 阶段 8 单点 | `e13ff0f` |
| CritMultiplier 4.07 vs 5.24 | **非 Malice 爆伤载荷**——vendor CritMult INC 聚合 PoBR 已逐位对齐（387）；缺口 = 必然暴击 less 腿输入的 pre-inevitable 暴击率 39.52 vs 69.92=(13+10)×3.04，+10 为 Critical Weakness 敌侧 SelfCritChance（ConfigOptions.lua:1892-1894）。enemy mod 已注入但其 `{Condition:ApplyCriticalWeakness}` tag 查 cfg 单条件空间恒 false（enemy FLAG 未落未前缀名）。Malice 仅是 vendor UI ifFlag（ConfigTab.lua:444 只控可见性，BuildModList :881-907 不查）| config_resolve 敌侧条件**未前缀桥**（仅对被敌侧数值 mod tag 引用的 var，防 Chilled 类玩家名污染） | `f941876` |
| h3 Potent Exposure（同根） | `exposure_effect_+%` → `<El>ExposureEffect INC`（SkillStatMap.lua:1731-1735）双段丢弃：主通道名单缺名（monk 主组）+ 非主组 support 无注入面（chronomancer Frost Bomb 副组，vendor 按来源技能作用域 :3193-3211/:3226-3231） | 主通道直通名单 += 三元素 ExposureEffect；编排 `exposure_support_modifiers` 扫含曝光载荷的非主组（主组跳过防双注入、名族过滤防局部词条全局泄漏） | `19ad9c2` |

**收敛**（effective 口径）：druid-oracle-comet CritMultiplier 5.24 逐位 ✓、
TotalDPS 0.66x→**0.97x（入带）**、TotalDotDPS 0.45x→0.89x（剩余 ≈0.92 curse
duration 因子 × 速度残差，归 curse 线）；monk-invoker-frost-bomb CritChance
100 ✓、TotalDPS 0.33x→0.45x；sorceress-chronomancer CritChance 18.19 逐位 ✓。
聚合：offensive 40→46/80 @5%、dot 3→4/37 @5%、defensive 377→379/450 @5%。

**登记（本波勘察暴露、未实现）**：
- 非主组主动技能的**玩家侧 Buff 数值载荷**（oracle buffList 实测 druid：
  Mysticism I `Damage INC 30`、Nature's Exchange `ColdMin/Max BASE`、Spell
  Totem aura `CritChance INC 50`）——`player_buff_stat_modifiers` 允收名单仅
  Accuracy，扩名单须逐消费方对照（Buff kind 消费段 buff_pass:328-355 已就绪）；
- 多曝光源且效果系数不同的 per-source 缩放（vendor :3226-3231 逐源独立、
  PoBR 扁平求和）——reduce_enemy_exposure doc 既有 TODO(parity) 维持。

## 9. M4-n crit/speed 尾差线——结果（2026-06-13）

m1 收敛表带边 build 的逐项 oracle 分解（5 个独立行为 commit）：

| build | 起点 | 根因（oracle 钉值） | 修复 | commit |
|---|---|---|---|---|
| ice-shot | CritMult 0.93x/TotalDPS 0.87x | Time-Lost 珠宝『N% increased Effect of Notable Passive Skills in Radius』未建模（ModParser.lua:6847 → JewelNotablePassiveSkillEffect；消费 CalcSetup.lua:246-275 对半径内 Notable modList 整体 ScaleAddList）：notable 自身词条 13407 25→30、56776 60→73 + 授予词条 10→12×6、12→14×4，全 vendor 截尾（ModStore.lua:45-80 `m_modf(round(v×scale,2))`）逐位吻合 | RadiusJewel.notable_effect_inc + radius_jewel_expansions 共享几何 + 授予文本内联缩放 + notable 自身差额副本；槽位加成（quiver/ring）副本同步改截尾口径（quiver INC 23.8→23） | `7763401` |
| varashta（腿1） | Speed 0.95x | 『13% increased Cast Speed during any Flask Effect』后缀变体缺失（vendor ModParser.lua:1840-1841 → Condition:UsingFlask），整条硬失败被丢弃 | strip_tag_once 后缀表 += `during (any) flask effect` 两变体 | `89f3c36` |
| varashta（腿2） | 同上 | Arcane Surge buff 载荷无注入点（vendor CalcDefence.lua:1580-1591：Speed INC 15×effect [Cast] + ManaRegen MORE 20×effect，effect=1+Σ(ArcaneSurgeEffect,BuffEffectOnSelf)/100；flag 解析与 AffectedBy 桥 PoBR 已有）。树 27388『Aspiring Genius』授予 flag，rune『40% increased effect of Arcane Surge on you』→ ×1.4=21 | overlay buff_definitions += ArcaneSurge 条目（数据驱动走既有 expand_misc_buffs）+ 名表 `effect of arcane surge on you`→ArcaneSurgeEffect（ModParser.lua:472）+ vendor_ref 守卫扩 CalcDefence 行段 | `8c8a70b` |
| titan | 1.07x 越带高估 | 3b Hulking Form 小点副本按 v×inc/100 直加，vendor ScaleAddList 截尾：『3% increased Attack/Skill Speed』×1.5=4.5 vendor 落 4——speed INC 44 vs 42、伤害 INC 同源过记 | 3b 改差额副本 trunc(round(v×scale,2))−v（与 Time-Lost 同口径 vendor_scale_mod_value） | `429bedd` |
| huntress-twister | Speed 0.93x/TotalDPS 0.87x | Tree:29306『15% increased Attack Speed while not on Low Mana』整条丢失：低魔条件后缀族缺失（vendor ModParser.lua:1761-1763 → Condition:LowMana[/neg]，默认 false → neg 生效） | strip_tag_once += 四变体（`while [you are] [not] on low mana`） | `5e8d9d5` |

**收敛（effective 口径）**：
- ice-shot：CritChance 0.97→0.99 ✓、CritMultiplier 0.93→0.99 ✓、TotalDPS
  0.87→0.96 ✓、TotalDotDPS 0.89→0.99 ✓、CombinedDPS 0.87→0.96 ✓（全列入带）
- varashta：Speed INC 138 逐位 ✓、rate 0.704142 逐位 ✓（0.95→1.00x）
- titan：Speed 1.14 逐位 ✓、TotalDPS 1.07→1.05 ✓、TotalDotDPS 1.09→1.02 ✓、
  CombinedDPS 1.07→1.04 ✓（全列入带）
- huntress-twister：Speed INC 106 逐位 ✓（0.93→1.00x）、TotalDotDPS
  0.91→0.99 ✓、TotalDPS 0.87→0.94（余差=量级线，见登记）
- 连带：gemling TotalDPS 0.88→0.96 ✓ + CombinedDPS 0.95 ✓（flask 条件 +
  Arcane Surge）、stormweaver TotalDPS 0.91→0.94 ✓、monk-twister
  0.73→0.80 + Speed 0.97→1.00 ✓
- 聚合：**off@5% 52→60（75.0%）、off@10% 62→68（85.0%）、dot@5% 9→15
  （40.5%）、dot@10% 12→17（45.9%）**；defensive 379/392 不变

**登记（oracle 已分解、根因复杂或另属线）**：
- **monk-twister CritMult 0.95x**：缺腿 = Tree:5802 `CritMultiplier INC 40
  [MultiplierThreshold:enemyDistance, Condition:WeaponSet2]`——敌人距离阈值
  Multiplier（vendor config `enemyDistance` 变量 + MultiplierThreshold tag
  机制 PoBR 未建）；其余 INC 逐位对齐（327+40=367 ✓）。
- **huntress-twister AverageDamage 0.94x**：速度线闭环后余差全在击中量级
  （per-type 分量待逐位，与 deadeye per-hit 登记同线）。
- **titan 带内余差 ~+4.7%**：chaos/fire 链分量过记（PoBR chaos stored
  2027-3166 vs vendor 1504-2263，pre-scale base 464 vs 345——gain-as/转换链
  分量口径，量级线）；INC 聚合已逐位（336.5 vs 336，0.5 为带内截尾残差）。
- **varashta TotalDPS 0.72x / TotalDotDPS 0.56x**：速度/暴击全对齐后余差全在
  击中量级线（Cast on Crit comet 链），另属 per-hit 分量对拍线。
- **stormweaver CombinedDPS 0.85x**：TotalDPS 已入带（0.94 ✓）；余差 =
  TotalDotDPS 0.00x（Pinnacle of Power buff 载荷，§7.4 已登记归 statmap
  Buff 域，本波禁动）。
- 范围珠宝效果的两个近似（当前样本无触发）：半径重叠时 vendor 同节点后写
  覆盖单一效果（PoBR 按授予珠宝独立）；『Effect of **Small** Passive Skills
  in Radius』变体（ModParser.lua:6842，JewelSmallPassiveSkillEffect）未接
  ——18-build 样本无此词条，出现时与 notable 通道同构接入。

baseline 按惯例本波不动（所有列 ≥ 基线），入带后随主线合并 bump。
## 8. M4-n varashta/comet 系残差包——三项闭环（2026-06-13）

m1 移交三项（focus implicit ModRange / Sigil+Conflux buff 载荷 / 非曝光缺口
勘察）全部 oracle 钉值闭环；varashta TotalDPS eff 0.69x→**0.95x**、
TotalDotDPS 0.53x→**0.97x ✓**、CombinedDPS 0.68x→0.95x、Mana 1.04x→1.00x ✓。
聚合 off@5% 52→53=66.2%（@10% 62→64=80.0%）、dot@5% 9→11（@10% 12→16）。

| 项 | 钉值结论 | 修复 | commit |
|---|---|---|---|
| ① focus 65 vs 33 | **ModRange 为误归因**——vendor rangeLineList=0、行文本无区间；真实机制 = Disciple of Varashta『Instruments of Power』节点 20701（tree.lua:57320-57334）『50% reduced bonuses gained from Equipped Focus』：ModParser.lua:4867 → EffectOfBonusesFromFocus INC -50，CalcSetup.lua:1209-1220 对 Focus 件全局数值词条 ×0.5（LIST/FLAG 经 MergeMod skipNonAdditive 保全值）。oracle modDB 双证：Damage 65→33、CritChance 47→24、Int 21→11、Mana 104→52 | slot_bonus_effect_scales 增 reduced 前缀 + equipped focus 目标（Weapon2 实为 Focus 基底门控）；消费点放开负 scale（全值+负副本 = ×(1+scale)） | `bc3f799` |
| ② Sigil/Conflux buff 载荷 | vendor buffList：Sigil `Damage MORE 17`（eff level 32 per-stage 17 × stage 1，limitVar=MaxStages 4）+ Conflux `<El>Damage MORE 73×(1/3)=24.33`（invert Multiplier，config Average 档 3）。§7.4 登记的 scalar 连坐子障碍**不阻塞本项**（两条目无 element.scalar） | ModTag::Multiplier 增 invert + translate_tag 允收 limitVar/invert；允收名单扩 Damage/<El>Damage/Multiplier:SigilOfPowerMaxStages + Spell flag 直译 + GlobalEffect unscalable 容忍；buff_skill_specs 玩家 Buff 分支（等级 = 宝石 + 适用 +N Level；同名 stat 先加法合并防 merge_buff 取强吞品质段）+ `Multiplier:` BASE 桥进 cfg.multipliers；config:elementalConfluxElement handler | `c2710a4` |
| ③ 非曝光缺口 | 分解：量级 panel 0.992 ✓（旧 0.19x 线已闭）/ 敌侧 ×1.5 同构 ✓（Rakiata 100% 反转敌抗 50→-50）/ 速度 0.95x（速度线）/ **伪高** = M4-H Effective 乘数桥对空 tag 条目空真，裸 config Multiplier（sigilOfPowerStages placeholder 1）被双计为 2 → Sigil MORE 17→34 | 桥判据加 `!tags.is_empty()`；连带 witch-lich-DD 1.11→0.98x ✓ 新命中、coiling-bolts 1.01→0.89x（旧 ✓ 为双计伪命中，真实低估暴露） | `ae844a2` |

**剩余登记（本波勘察暴露、未实现）**：
- **Uhtred's Exodus 系 support 等级授予**（`SupportedGemProperty` LIST +
  MultiplierThreshold SupportCount，sup_str.lua:7543-7579）：Conflux 实级
  21+3=24（63+q10=73）vs PoBR 21（60+10=71）——per-element MORE 23.33 vs
  24.33，varashta 残差 ~1%；
- **buff effect 缩放豁免（unscalable）**维度未建模——BuffEffect 乘区 ≠1 时
  `Multiplier:SigilOfPowerMaxStages` 会被错误缩放（本 build 乘区 1，无差异）；
- **coiling-bolts 量级线 0.89x**：双计移除后暴露的真实低估，归击中量级线；
- varashta Speed 0.95x：速度线（另一 agent，禁动）。
