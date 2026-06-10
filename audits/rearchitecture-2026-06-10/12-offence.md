# 进攻计算（伤害/暴击/速度/DPS）

> 重构审计 · 2026-06-10 · 领域 12
> 范围：击中伤害、暴击、攻速/施法速度、命中、技能 DoT、DPS 末端组装
> 前置：已读 `audits/pob2-parity-2026-06-09/FINDINGS.md`，本文只记录其未覆盖的结构性缺口（FINDINGS 覆盖 MORE round / ModFlags 子集语义 / 穿透注入 / traced 路径等，与本文的双 pass / Double Damage / 技能 DoT / dpsMultiplier 等不重叠）。

---

## PoB2 代码结构（结构地图）

PoB2 进攻域由两个文件构成。

### Modules/CalcOffence.lua（6235 行）

单一入口 `calcs.offence(env, actor, activeSkill)`（L377），按章节顺序执行：

| 行段 | 章节 | 要点 |
|------|------|------|
| L68–156 | 局部纯函数 | `calcConvertedDamage` / `calcGainedDamage` / `calcDamage`：基于 `activeSkill.conversionTable` 的逐类型伤害聚合（typeFlags 仅含最终类型，PoE2 无转换源 double-dip） |
| L404–1116 | armour break / AoE / 射程 / 投射物 | 含 Molten Strike 三级半径/断点、武器射程 |
| L1116–1255 | 弩 bolt/ammo | reloadTime / boltCount / ChanceToNotConsumeAmmo / InstantReloadChance |
| L1255–1695 | skill type stats | 图腾（放置速度/图腾生命抗性）、陷阱（TrapThrowingSpeed/冷却）、地雷（MineLayingSpeed/引爆）、brand 等按技能类型的吞吐与衍生数值；L3127 `quantityMultiplier = Sum(BASE,"QuantityMultiplier")` |
| L2031–2233 | 技能消耗 + canDeal | L2229 `canDeal[type] = not Flag("DealNo"..type)` 门控 |
| L2233–2449 | 转换表 + passList | 两阶段转换表构建（skill 先于 global、源内归一）；攻击技能拆 Main Hand / Off Hand 双 pass（L2369–2440，source=weaponData1/2、unarmed 用 `data.unarmedWeaponData[classId]`、setOffHand* 技能走 weapon2） |
| L2453–2538 | combineStat | 以 AVERAGE/ADD/DPS/CRIT/CHANCE/CHANCE_AILMENT 等模式合并双手输出（DPS 模式 = MH+OH 相加、非 doubleHitsWhenDualWielding 时除 2） |
| L2543–2694 | 命中链 | Accuracy（base×inc×more，m_floor）→ 距离衰减惩罚（`Multiplier:enemyDistance`、`data.misc.AccuracyFalloffStart/End`、`MaxAccuracyRangePenalty`，L2562–2577）→ `AccuracyVsEnemy`（L2556–2574）→ 法术必中/`CannotBeEvaded` → `calcs.hitChance(evasion, accuracy) × mod("HitChance")`（L2615–2617）→ `HitChanceCanExceed100`（L2619–2624，超额写 `Multiplier:ExcessHitChance`）→ 敌方格挡 `max(min(BlockChance,100) - reduceEnemyBlock, 0)`、`CannotBlockAttacks`（L2666–2669）→ `HitChance = AccuracyHitChance × (1-block/100)`（L2671） |
| L2694–3527 | 攻速/施法速度 | triggerTime/triggerRate 分支、`skillData.attackSpeedMultiplier`（L2721–2723）、`castTimeOverridesAttackTime`（L2724–2726）、`SkillAttackTime` mod（L2727–2728）、`Sum(BASE,"Speed")` flat 秒（L2728/2730）、trauma 自持叠层（L2738–2821）、核心公式 `Speed = 1/(baseTime/round((1+inc/100)×more,2) + TotalAttackTime + TotalCastTime)`（L2827）、selfCast 才乘 ActionSpeedMod（L2831–2835）、totem 换 TotemActionSpeed（L2846–2852）、冷却 cap `min(Speed, Repeats/cooldown)`（L2855–2859）、服务器帧 cap（L2864–2865）、弩 reload 折算 FiringRate→Speed（L2867+） |
| L3527–3620 | Empowered/Ancestrally Boosted | 攻击 uptime、Explosive Arrow 引信 |
| L3620–3842 | 暴击 | 暴击率/爆伤（cap、命中降级、Lucky、Bifurcate、Inevitable、敌方 SelfCrit*） |
| L3842–3861 | Double/Triple Damage | `TripleDamageEffect=2×chance`、Triple 抵扣 Double 的去重、Intimidate uptime 注入、`ScaledDamageEffect ×= (1+DD+TD)`（L3861）；L3863 `skillData.dpsMultiplier ×= calcLib.mod("DPS")` |
| L3868–3902 | Culling / ReservationDPS | `CullingStrike<Rarity>Threshold` gameConstants + ReservationDPS 乘子 |
| L3902–4328 | 击中伤害主体 | 逐类型 base（source[min/max] + added×addedMult，×baseMultiplier，敌方 `Self<Type>Min/Max` 计入）→ conversionTable 重组 → **pass=1 暴击 / pass=2 非暴击双循环**（L3978–3980 `cfg.skillCond["CriticalStrike"]=(pass==1)`，暴击 pass 重新聚合伤害且 allMult 乘 CritMultiplier，L4030–4031）→ `allMult = ScaledDamageEffect × FistOfWar × Ancestral × Warcry`（L4023–4030）→ LuckyHits 平均（`min/3+2max/3` 按几率与中点混合，L4034–4044）→ 存 pre-resist `StoredHitAvg/StoredCritAvg` 供异常 → 敌方抗性/减伤 → 分 pass 累积 leech（L3970+） |
| L4394–4452 | DPS 组装 | `AverageHit = hitAvg×(1-c) + critAvg×c`；`TotalDPS = AverageDamage × (HitSpeed or Speed) × skillData.dpsMultiplier × quantityMultiplier`（L4407） |
| L4554–4705 | 双手 combineStat 大表 | L4673/4676 breakdown 印证两因子；L4705 leech 速率 |
| L4766–5831 | 异常/debuff | bleed/poison/ignite/非伤害异常/击退/眩晕/impale（L5680）/Decay（L5759） |
| L5831–6093 | 通用技能 DoT | `dotCfg`（加 ModFlag.Dot、按 `dotIsArea/dotIsSpell/dotIsAttack/dotIsHit` 剥 flag，L5832–5855）；逐类型 `baseVal = skillData[type.."Dot"]`，`total = baseVal×(1+inc/100)×more×(1+DotMultiplier/100)×aura×effMult`（L5905–5909）；`DotCanStack` 时 `TotalDot = min(instance×speed×Duration×dpsMultiplier×quantityMultiplier, DotDpsCap)`（L5931，trap/mine 按 keywordFlags 换用各自投掷速率 L5926–5930） |
| L6093–6235 | 合并 DPS | `WithDotDPS` / `With<Ailment>DPS`、ImpaleDPS、MirageDPS、`TotalDotDPS = Σ(dot+poison+caustic+ignite+burning+bleed+corrupting+decay)` clamp DotDpsCap、CullingDPS、ReservationDPS |

### Modules/CalcActiveSkill.lua（1049+ 行）

- `mergeSkillInstanceMods`（L82）：statSet→mod，statMap 数据驱动。
- `createActiveSkill`（L144）：宝石+辅助→activeEffect。
- `getWeaponFlags`（L274–309）：`data.weaponTypeInfo[type]` → ModFlag 武器位 + Weapon/Weapon1H/Weapon2H/WeaponMelee/WeaponRanged；处理 `countsAsAll1H` / `asThoughUsing` / `cannotUseGemTag`。
- `buildActiveSkillModList`（L381）：多 part 技能选择（L417–439）、盾牌攻击、weapon1Flags/weapon2Flags 装配与 disable 原因（L447–512）、stat-map skillFlag、totem 基础 stat、Empower。

ModFlag 位表在 Data/Global.lua L222–259：Attack/Spell/Hit/Dot/Cast/Thorns/Melee/Area/Projectile/Ailment/MeleeHit/Weapon + 15 种武器类型位（Axe/Bow/Claw/Dagger/Mace/Staff/Sword/Wand/Unarmed/Fishing/Crossbow/Flail/Spear/Warstaff/Talisman）+ 4 个类别位（WeaponMelee/WeaponRanged/Weapon1H/Weapon2H）。

---

## pobr 实现现状

pobr 进攻域 ≈ `pobr-core/src/calc/{offence.rs(1242 行), damage.rs(716), crit.rs(429), skill_use_time.rs(187), skill_mechanics.rs(734)}` + `ailment.rs`/`perform.rs`/`trigger.rs`，build 层装配在 `pobr-build/src/calc_orchestrator.rs`。

**已覆盖且质量较高：**

1. 暴击管线 `crit.rs` 逐字对齐 L3681–3838（cap/命中降级/CritChanceLucky/Bifurcate/Inevitable 几何级数/NoCritMultiplier/敌方 SelfCrit*），含 traced 版。
2. 伤害转换链 `damage.rs`：两阶段（skill→global）折叠为单矩阵、源内归一、gain-as-extra 基于转换后量、Min/Max\<Type\>Damage 独立 MORE、AddedDamage 效率、ElementalDamage 共享组、PoE2 无 double-dip 口径（经 headless oracle 验证）。
3. 速度链 `offence.rs:231-253`：速度 bucket（AttackSpeed/CastSpeed/SkillSpeed）+ ActionSpeed 独立乘区 + TotalCastTime/TotalAttackTime 加法分母 + 冷却 cap×Repeats + 服务器帧 cap。
4. 命中链骨架：法术必中/CannotBeEvaded/敌方格挡/精准公式（`offence.rs:254-273`）。
5. 敌方减伤 `enemy_damage_multiplier`：DamageTaken 链、抗性+穿透（minPen=0）、护甲 DR+Overwhelm。
6. baseMultiplier（damage_multiplier）与 attack_speed_multiplier 已从 catalog JSON 进入 orchestrator。
7. 空手（`unarmed_contribution`，硬编码 2–N 物理/1.65 攻速/5% 暴击）与非武器攻击（Shield Wall setOffHand*，`non_weapon_attack_contribution`）已建模。
8. 武器局部词条（局部物理增伤/flat/局部攻速）已隔离为武器基底独立乘区。
9. source-level 归因 traced 全链路。

**结构性差异：** pobr 是「单 pass、单武器、平均因子」模型——武器基底只取 Weapon1（`calc_orchestrator.rs` `weapon_contribution`，L1104 起；副手仅作为盾/DualWielding 条件源），伤害只算一遍非暴击分量再乘 crit.effect 平均因子（`offence.rs:291` `total_hit_avg = non_crit_hit_avg × crit_average_factor`），没有 PoB2 的 MH/OH 双 pass、暴击/非暴击双 pass、双倍/三倍伤害、LuckyHits、通用技能 DoT、totem/trap/mine 吞吐、弩 reload、culling/impale/合并 DPS 族。ModFlags 只有 5 位（`pobr-data/src/modifier.rs:38-42`），武器类型条件靠 mod_parser（L1016-1027）转 condition 字符串 + orchestrator 注入条件变量（UsingMace/UsingCrossbow/DualWielding 等）近似。ninja_parity 门禁显示进攻侧 24%@5%，与上述结构性缺口一致。

---

## 缺口清单

| # | 标题 | 严重度 | 类型 | PoB2 证据 | pobr 位置 | 说明 |
|---|------|--------|------|-----------|-----------|------|
| 1 | 无 Main Hand / Off Hand 双武器 pass 与 combineStat 合并 | 🔴 high | missing | CalcOffence.lua:2369-2449, 2453-2538, 4554-4705 | calc_orchestrator.rs:1104-1146 | 只对主手建模；per-hand 条件词条与双手合并语义全缺 |
| 2 | ModFlags 位集仅 5 位，缺武器类型/部位/Hit/Dot 全部维度 | 🔴 high | partial | Data/Global.lua:222-259 + CalcActiveSkill.lua:274-309 | pobr-data/src/modifier.rs:38-42 | 武器条件词条靠 condition 字符串近似，覆盖不全 |
| 3 | 暴击/非暴击双 pass 伤害重算缺失（只乘平均暴击因子） | 🔴 high | design | CalcOffence.lua:3978-3980, 4030-4031, 4047-4057, 4395 | offence.rs:289-295 | 带暴击条件的伤害词条全部失效 |
| 4 | Double/Triple Damage 与 ScaledDamageEffect 全乘区缺失 | 🔴 high | missing | CalcOffence.lua:3842-3861, 4023-4033 | 无（全局 grep 零命中） | 整条 allMult 乘区缺失 |
| 5 | 通用技能 DoT（skillData \<Type\>Dot）与合并 DPS 族缺失 | 🔴 high | missing | CalcOffence.lua:5831-5931, 5759, 6093-6234 | ailment.rs（仅异常 DoT）；catalog.rs 无 dot 字段 | DoT 主体技能 DPS 为 0 |
| 6 | TotalDPS 缺 dpsMultiplier 与 quantityMultiplier 因子 | 🔴 high | missing | CalcOffence.lua:3863, 3127-3130, 4407 | offence.rs:295 | 多投掷物/多图腾/带 dpsMultiplier 技能全部少算 |
| 7 | 命中链缺距离衰减、AccuracyVsEnemy、HitChance mod、reduceEnemyBlock | 🟡 medium | partial | CalcOffence.lua:2556-2671 | offence.rs:254-273 | 五个子机制缺失 + Accuracy 无 m_floor |
| 8 | DealNo\<Type\>/canDeal 伤害类型禁用门控缺失 | 🟡 medium | missing | CalcOffence.lua:2226-2230, 3989/4793/5451 | 无（grep 零命中） | Avatar of Fire 类 build 方向性偏高 |
| 9 | 攻速链缺 castTimeOverridesAttackTime/SkillAttackTime/Speed BASE/selfCast·totem action speed 分支/round(,2) | 🟡 medium | partial | CalcOffence.lua:2724-2852 | offence.rs:236-253 | 四处细粒度差异 |
| 10 | 弩 reload/bolt/ammo 吞吐模型缺失 | 🟡 medium | missing | CalcOffence.lua:1116, 2867+ | skill_stat_map.rs:98（仅排除项注释） | 所有弩 build DPS 系统性偏高 |
| 11 | 图腾/陷阱/地雷吞吐与 quantity 体系缺失 | 🟡 medium | missing | CalcOffence.lua:1255-1695, 5926-5930 | 无（grep 零命中） | 三类 build 原型 DPS 不可用 |
| 12 | LuckyHits 伤害掷骰平均缺失 | 🟡 medium | missing | CalcOffence.lua:4034-4044 | damage.rs:99-102 | avg() 写死中点，Lucky 词条无消费点 |
| 13 | Culling/Impale/Mirage 等合并 DPS 末端字段缺失 | 🟢 low | missing | CalcOffence.lua:3868-3877, 6122-6143, 6177-6213, 6233 | 无（仅注释级命中） | 面板 CombinedDPS 组成项缺失 |
| 14 | 多 part 技能（skillPart）选择机制缺失 | 🟢 low | missing | CalcActiveSkill.lua:417-439 | 无；catalog.rs GrantedEffectDef 无 parts 字段 | 多形态技能只能按默认形态算 |

---

## 缺口详述

### 1. 无 Main Hand / Off Hand 双武器 pass 与 combineStat 合并（🔴 high / missing）

- **PoB2**：CalcOffence.lua:2369-2449（passList 构建：weapon1/weapon2 各一 pass）+ 2453-2538（combineStat AVERAGE/ADD/DPS/CRIT/CHANCE/CHANCE_AILMENT；DPS 模式 = MH+OH 相加、非 doubleHitsWhenDualWielding 时再除 2）+ 4554-4705。
- **pobr**：`crates/pobr-build/src/calc_orchestrator.rs:1104-1146`（`weapon_contribution` 只读 `EquipmentSlot::Weapon1`；Weapon2 仅用于盾判定/DualWielding 条件与 `off_hand_defence`）。

PoB2 攻击技能对主手/副手各跑一遍完整管线（各自的 weaponData、weapon1Cfg/weapon2Cfg、独立 accuracy/crit/speed/damage、per-hand 条件 MainHandAttack/OffHandAttack），再按 stat 语义合并（DPS 模式 (MH+OH)/2、Speed 调和平均、CritChance 按 CRIT 模式、异常按 CHANCE_AILMENT 加权）。pobr 只对主手建模：当副手与主手数值相近时单手近似误差不大（因 PoB2 DPS 也除 2），但**副手词条/基底与主手不同的双持 build 误差不可控**（强副手被忽略→偏低，弱副手被忽略→偏高）、per-hand 条件词条全部失效、doubleHitsWhenDualWielding 类技能（双倍出手）系统性偏低一半、Speed/Crit/异常合并语义全缺。这是进攻 parity 24%@5% 在双持 build 上的结构性根因之一。

**修复方向**：在 orchestrator 层引入 passList 概念——对攻击技能按主/副手各构建一份 weapon contribution + per-hand 条件，调用两次 offence 管线，再实现 combineStat 的六种合并模式（合并语义可表驱动）。

### 2. ModFlags 位集仅 5 位（🔴 high / partial）

- **PoB2**：Data/Global.lua:222-259（ModFlag.Hit/Dot/Cast/Thorns/Ailment/MeleeHit/Weapon + Axe/Bow/Claw/Dagger/Mace/Staff/Sword/Wand/Unarmed/Fishing/Crossbow/Flail/Spear/Warstaff/Talisman + WeaponMelee/WeaponRanged/Weapon1H/Weapon2H）+ CalcActiveSkill.lua:274-309（getWeaponFlags 由 `data.weaponTypeInfo` 派生）。
- **pobr**：`crates/pobr-data/src/modifier.rs:38-42`（仅 ATTACK/SPELL/MELEE/PROJECTILE/AREA 五位）。

PoB2 的词条门控核心是 64 位 ModFlag：『with Maces』『with One Handed Melee Weapons』『with Two Handed Weapons』『Unarmed』等词条全靠武器类型/类别位匹配；getWeaponFlags 还处理 `countsAsAll1H`、`asThoughUsing`（视为持用某类武器）、`cannotUseGemTag`。pobr 仅 5 位，武器条件词条靠 mod_parser（mod_parser.rs:1016-1027 后缀→UsingMace/UsingCrossbow 等 condition）+ 伤害族专用 ModName（CrossbowDamage/MaceDamage）近似，覆盖不全且与 ModFlag 子集匹配语义（FINDINGS 01-02 已修的 `is_subset_of`）脱节。同时缺 Hit/Dot 位导致 dotCfg 剥 flag 机制（见缺口 5）无法表达。

**修复方向**：扩 ModFlags 为完整 64 位位集（武器 15 类型位 + 4 类别位 + Hit/Dot/Cast/Thorns/Ailment/MeleeHit/Weapon），武器类型→位的派生走 `weapon_type_info` 数据表（见数据切分建议）；condition 近似路径逐步退役。

### 3. 暴击/非暴击双 pass 伤害重算缺失（🔴 high / design）

- **PoB2**：CalcOffence.lua:3978-3980（`for pass=1,2; cfg.skillCond["CriticalStrike"]=(pass==1)`）+ 4030-4031（pass1 的 allMult 额外乘 CritMultiplier）+ 4047-4057（Stored\<Type\>CritAvg/HitAvg 分别存储）+ 4395（`AverageHit = hitAvg×(1-c)+critAvg×c`）。
- **pobr**：`crates/pobr-core/src/calc/offence.rs:289-295`（`total_hit_avg = non_crit_hit_avg × crit.effect`）。

PoB2 对暴击 pass 以 CriticalStrike 条件**重新聚合**全部伤害（『增伤 on Critical Hit』类条件词条只在 pass1 生效），且暴击与非暴击各自过一遍敌方抗性/减伤再按暴击率加权。pobr 把暴击折成单一 crit_effect 因子乘到非暴击平均上：凡带暴击条件的伤害词条全部失效，且暴击伤害的敌方减伤与非暴击共用同一口径。对暴击向 build 系统性偏差。FINDINGS 仅覆盖了 traced 路径分叉（05-05），未覆盖此双 pass 语义本身。

**修复方向**：伤害主体改为双 pass：以 `CriticalStrike` 条件分别聚合、分别过敌方减伤，末端 `hitAvg×(1-c)+critAvg×c` 加权。与缺口 1 的 MH/OH 双 pass 正交（PoB2 实为 2×2 嵌套），实现时一并设计 pass 矩阵。

### 4. Double/Triple Damage 与 ScaledDamageEffect 全乘区缺失（🔴 high / missing）

- **PoB2**：CalcOffence.lua:3842-3861（`TripleDamageEffect=2×chance`、OnCrit 变体按 CritChance 折算、Intimidate uptime 注入、Triple 抵扣 Double、`ScaledDamageEffect ×=(1+DD+TD)`）+ 4023-4033（`allMult = ScaledDamageEffect × FistOfWar × Ancestral × Warcry exerted`）。
- **pobr**：无（crates/apps/tools 全局 grep DoubleDamage/TripleDamage/ScaledDamageEffect 零命中）。

DoubleDamageChance/TripleDamageChance（含 OnCrit 变体、敌方 SelfDoubleDamageChance、Intimidate uptime 注入）构成独立乘区 allMult，连同 warcry exerted（OffensiveWarcryEffect）/Ancestral 一并乘到每类型击中伤害。pobr 完全没有这条乘区：带『chance to deal Double Damage』词条或 warcry 增强的 build 伤害直接少算该乘区。

**修复方向**：新增 ScaledDamageEffect 计算单元（Double/Triple 去重 + OnCrit 折算 + Intimidate uptime），作为 allMult 因子接入击中伤害；warcry exerted 因子可同期或后续补。

### 5. 通用技能 DoT 与合并 DPS 族缺失（🔴 high / missing）

- **PoB2**：CalcOffence.lua:5831-5931（dotCfg 构建 + 按 dotIsArea/dotIsSpell 剥 flag；`total = baseVal×(1+inc/100)×more×(1+DotMultiplier/100)×aura×effMult` L5905-5909；DotCanStack→`TotalDot=min(instance×speed×Duration×dpsMultiplier×quantityMultiplier, DotDpsCap)` L5931，trap/mine 换速率 L5926-5930）+ 5759（Decay）+ 6093-6234（WithDotDPS/`TotalDotDPS=Σ8 项` clamp DotDpsCap/CombinedDPS）。
- **pobr**：`crates/pobr-core/src/calc/ailment.rs`（仅 bleed/ignite/poison 异常 DoT）；`perform.rs:610-728`（仅异常 stacked DPS）；catalog.rs 无任何 \<Type\>Dot 基值/DotMultiplier 字段。

PoB2 的 DoT 有两路：异常（pobr 已建模）与技能自带持续伤害（毒雨、Decay、点燃地面/腐蚀地面、DotCanStack 类叠层 DoT）。后者依赖技能 statSet 的 \<Type\>Dot 基值 + DotMultiplier + dotCfg 专用 flag 集，pobr 完全缺失（全局 grep DotMultiplier/DotCanStack/TotalDotDPS 仅命中常量注释）——所有 DoT 主体技能 DPS 为 0。末端的 CombinedDPS/WithDotDPS/TotalDotDPS 聚合（含 8 项求和 + DotDpsCap）也无对应物，面板可比字段缺一族。

**修复方向**：分两步——(a) catalog schema 扩 dot 基值族 + dotIs* 旗标（数据侧，见切分建议），adapter 落数据；(b) calc 侧新增技能 DoT 模块（dotCfg flag 剥离依赖缺口 2 的 ModFlags 扩位）+ 末端合并 DPS 字段族。

### 6. TotalDPS 缺 dpsMultiplier 与 quantityMultiplier 因子（🔴 high / missing）

- **PoB2**：CalcOffence.lua:3863（`skillData.dpsMultiplier ×= calcLib.mod("DPS")`）+ 3127-3130（`quantityMultiplier = Sum(BASE,"QuantityMultiplier")`，>1 时写 output）+ 4407（`TotalDPS = AverageDamage × (HitSpeed or Speed) × dpsMultiplier × quantityMultiplier`）。
- **pobr**：`crates/pobr-core/src/calc/offence.rs:295`（`dps = total_hit_avg_for_dps × action_rate × hit_chance`，无二因子）；`ailment.rs:1012-1014` 与 `calc_orchestrator.rs:199` 仅有 defer 注释。

skillData.dpsMultiplier（技能数据字段，如多次打击/分裂箭按命中数折算）与 quantityMultiplier（图腾数/陷阱数/地雷数）是 PoB2 DPS 末端的两个独立乘子；『DPS』mod 名（`calcLib.mod "DPS"`）也只在此消费。pobr 的 DPS 公式止于 hit×rate×hitChance，多投掷物/多图腾/带 dpsMultiplier 数据的技能全部少算。grep 证实 pobr 全局仅注释提及、无实现。

**修复方向**：catalog 的 SkillLevelDef/SkillStatSetDef 补 dpsMultiplier 字段并入 orchestrator；ModDb 增 QuantityMultiplier/DPS 名消费点；offence DPS 末端乘入两因子。改动小、回报直接，可优先做。

### 7. 命中链缺距离衰减等五项（🟡 medium / partial）

- **PoB2**：CalcOffence.lua:2562-2577（enemyDistance×AccuracyFalloffStart/End×MaxAccuracyRangePenalty 距离惩罚，m_floor）+ 2556-2574（AccuracyVsEnemy 并列名独立聚合）+ 2615-2617（×`calcLib.mod("HitChance")`）+ 2619-2624（HitChanceCanExceed100→Multiplier:ExcessHitChance）+ 2666-2669（`block = max(min(BlockChance,100) - reduceEnemyBlock, 0)`；CannotBlockAttacks 且 isAttack → 0）。
- **pobr**：`offence.rs:254-273`（accuracy→hit_chance→enemy_block，仅此三步）。

缺失项：(1) 远程攻击的精准距离衰减（config enemyDistance 驱动，对弓/弩 build 命中显著影响）；(2) AccuracyVsEnemy 词条名；(3) HitChance inc/more 乘子；(4) 超 100% 命中转 ExcessHitChance 乘数机制；(5) 敌方格挡先 clamp 100 再减玩家 reduceEnemyBlock、CannotBlockAttacks 旗标（pobr 的 enemy_block 直接 clamp(0,1) 无 reduceEnemyBlock）。另外 PoB2 Accuracy 聚合用 m_floor（L2573），pobr 走 `scaled_numeric_stat` 无 floor。

**修复方向**：按五项逐个补齐；AccuracyFalloffStart/End/MaxAccuracyRangePenalty 走 misc 常量 JSON（见切分建议）。

### 8. DealNo\<Type\>/canDeal 门控缺失（🟡 medium / missing）

- **PoB2**：CalcOffence.lua:2226-2230（`canDeal[type] = not Flag("DealNo"..type,"DealNoDamage")`）+ 3989/4793/5451（hit/DoT/ailment 三处消费）。
- **pobr**：无（crates/apps 全局 grep DealNo/deal_no 零命中）。

Avatar of Fire 类『Deal no Physical Damage』关键石/词条依赖 canDeal 把对应类型 base 清零（且与转换交互：先转换后清零未转换残留）。pobr 无此门控，这类 build 会把本应清零的类型继续计入 DPS，方向性偏高。

**修复方向**：在 damage.rs 转换链之后增加 canDeal 清零步骤（注意顺序：转换先发生，残留未转换部分才被清零），hit/DoT/ailment 三处统一消费。

### 9. 攻速链四处细粒度差异（🟡 medium / partial）

- **PoB2**：CalcOffence.lua:2724-2730（castTimeOverridesAttackTime、SkillAttackTime mod、`baseTime=1/AttackRate + Sum(BASE,"Speed")`）+ 2827（分母 `round((1+inc/100)×more, 2)`）+ 2831-2835（仅 skillFlags.selfCast 乘 ActionSpeedMod）+ 2846-2852（totem 换 TotemActionSpeed）。
- **pobr**：`offence.rs:236-253`（action_speed_mod 无条件乘；apply_total_time 仅 TotalCastTime/TotalAttackTime；inc/more 不做 2 位舍入）。

逐项核实成立：(1) `Sum(BASE,"Speed")` flat 秒加进 baseTime（部分技能词条）pobr 未消费；(2) castTimeOverridesAttackTime/SkillAttackTime 数据字段未消费；(3) PoB2 速度乘子合并后 round 到 2 位小数（与 FINDINGS 01-01 的逐 mod round 不同层），pobr 不舍入会产生小数尾差；(4) ActionSpeedMod 仅 selfCast 技能施加且 totem 技能换用 TotemActionSpeed，pobr 对所有技能无条件乘 ActionSpeed（offence.rs:239-247 证实；FINDINGS 03-03 标 partly 但此 selfCast/totem 分支未列入）。

**修复方向**：(3)(4) 是纯逻辑修正可即修；(1)(2) 需 catalog/skill_stat_map 增加字段消费。

### 10. 弩 reload/bolt/ammo 吞吐模型缺失（🟡 medium / missing）

- **PoB2**：CalcOffence.lua:1116（Calculate ammo stats for bolt skills 章节）+ 2867+（skillData.reloadTime>0 时 FiringRate→EffectiveBoltCount/ChanceToNotConsumeAmmo/InstantReloadChance 折算实际 Speed）。
- **pobr**：`skill_stat_map.rs:98`（reload 仅作为数据层排除项注释）；全局 grep reload/bolt_count 无计算实现。

弩技能的真实出手速率 = 射击速率与装填时间的循环平均（受 boltCount、不消耗弹药几率、瞬间装填几率修正）。pobr 只有 CrossbowDamage 关键词增伤，速率仍按裸攻速算——所有弩 build 的 DPS 系统性偏高（漏掉 reload 停顿）。

**修复方向**：catalog 增加 reload_time/bolt_count 字段（数据侧），speed 链增加 reload 循环平均折算（逻辑侧）。

### 11. 图腾/陷阱/地雷吞吐与 quantity 体系缺失（🟡 medium / missing）

- **PoB2**：CalcOffence.lua:1255-1695（skill type stats：TrapThrowingSpeed/MineLayingSpeed/totem placement/brand）+ 5926-5930（DoT 速率按 Mine/Trap keywordFlag 换用对应投掷速率）。
- **pobr**：无（全局 grep TrapThrowingSpeed/MineLayingSpeed/totem_placement 零命中；仅 ailment.rs:1012 注释提及 defer）。

图腾/陷阱/地雷技能的速率不是攻速/施法速度，而是放置/投掷/铺设速度，数量走 quantityMultiplier；触发型 DoT 还要切换速率来源。pobr 无任何建模，这三类 build 原型的 DPS 不可用。与 FINDINGS 中 trigger 链路（03-01/03-02）互补，属不同缺口。

**修复方向**：与缺口 6 的 quantityMultiplier 一并设计：按 keywordFlag 切换速率 bucket（TrapThrowingSpeed/MineLayingSpeed/TotemPlacementSpeed），DPS 末端乘数量。

### 12. LuckyHits 伤害掷骰平均缺失（🟡 medium / missing）

- **PoB2**：CalcOffence.lua:4034-4044（LuckyHits/CritLucky/LightningNoCritLucky/ElementalLuckHits/\<Type\>LuckyHitsChance → `avgLucky = min/3 + 2max/3`，与中点平均按几率混合）。
- **pobr**：`damage.rs:99-102`（`DamageComponent::avg` 恒为 (min+max)/2）。

『Damage with Hits is Lucky』把每类型平均击中从 (min+max)/2 抬到 (min+2max)/3（约 +11% 对宽区间元素伤害）。pobr 的 avg() 写死中点，Lucky 词条解析后无消费点。注意与暴击几率 Lucky（crit.rs:158 已实现 CritChanceLucky）是两个不同机制，互不覆盖。

**修复方向**：DamageComponent 的平均计算接受 lucky 几率参数，按几率混合中点平均与 (min+2max)/3。

### 13. Culling/Impale/Mirage 合并 DPS 末端字段缺失（🟢 low / missing）

- **PoB2**：CalcOffence.lua:3868-3877（criticalCull/regularCull 按敌人稀有度 gameConstants 阈值 → CullMultiplier）+ 6122-6143（ImpaleDPS）+ 6177-6213（MirageDPS）+ 6233（`CullingDPS = CombinedDPS×(bestCull-1)`）。
- **pobr**：无（pobr-core/pobr-build grep culling/impale/mirage 仅 skill_stat_map.rs:394 与 calc_orchestrator.rs:597 的注释提及）。

斩杀（按敌人稀有度阈值 × 暴击斩杀的命中-暴击概率修正）、穿刺 DPS（物理 stored hit × ImpaleModifier）、镜影分身 DPS 是面板 CombinedDPS 的组成项。pobr 输出表无这些字段，对依赖它们的 build 面板低估；优先级低于上面缺口因受众较窄。

### 14. 多 part 技能（skillPart）选择机制缺失（🟢 low / missing）

- **PoB2**：CalcActiveSkill.lua:417-439（activeGrantedEffect.parts：按选中 part 改写 skillFlags 真/假）。
- **pobr**：无（crates/ 全局 grep skill_part/SkillPart 零命中；catalog.rs 的 GrantedEffectDef 无 parts 字段）。

一些技能（如先击中后爆炸的两段技能）在 PoB2 数据里有 parts 数组，UI/计算按 skillPart 切换 flag 集与数值口径。pobr 的 catalog schema 无 parts，XML 导入的 skillPart 字段被丢弃，这类技能只能按默认形态算。

---

## 数据 vs 逻辑切分建议

### 属于「数据」的部分（应 JSON 化，目前混在 PoB2 Lua 里）

1. **武器类型表 `data.weaponTypeInfo`**（Data/Global.lua 附近定义，CalcActiveSkill.lua:275 消费）：武器类型 → {flag 名, oneHand, melee, range}。纯查找表，应入 catalog 为 `weapon_type_info.json`。pobr 目前在 calc_orchestrator.rs:1030-1056 用 Rust match 硬编码近似了 melee/two_handed 判定——这是「数据写进了框架」的反模式，版本更新武器类型时要改代码。

2. **职业空手数据 `data.unarmedWeaponData[classId]`**（CalcOffence.lua:2383 消费）：每职业空手 phys/速度/暴击。pobr 的 `unarmed_contribution` 硬编码（2–N 物理/1.65 攻速/5% 暴击），应入 JSON。

3. **misc 游戏常量 `data.misc` / `data.gameConstants`**：ServerTickRate、AccuracyFalloffStart/End、MaxAccuracyRangePenalty、DotDpsCap、EnemyPhysicalDamageReductionCap、CullingStrike{Normal,Rare,Unique}Threshold 等。pobr 在 pobr-data/src/constants.rs 硬编码了一部分（DOT_DPS_CAP、SERVER_TICK_SECONDS）——这些随补丁变动，应进 `data/<version>/constants.json`（DataManifest 增一个 misc 常量域），框架只留默认值兜底。

4. **逐技能 skillData 字段**（src/Data/Skills/*.lua 38MB 的主体）：castTime、baseMultiplier、radius/radiusSecondary/radiusTertiary、attackSpeedMultiplier、castTimeOverridesAttackTime、reloadTime/boltCount、`<Type>Dot` 基值、dotIsArea/dotIsSpell/dotIsAttack/dotIsHit、dpsMultiplier、doubleHitsWhenDualWielding、showAverage、parts[]、weaponTypes 限制、cannotBeEvaded 等布尔/数值开关。pobr 的 `SkillLevelDef`/`SkillStatSetDef`（catalog.rs）已覆盖 cooldown/attack_time/cost/attack_speed_multiplier/base_multiplier/crit_chance/伤害 stat，**尚缺**：parts（多形态）、weaponTypes（可用武器限制）、reload_time/bolt_count、dot 基值族、dotIs* 布尔、radius 三级、dpsMultiplier、castTimeOverridesAttackTime、doubleHitsWhenDualWielding。这些都是 GGG dat/PoB 数据字段，扩 schema + pobr-data-adapter 即可，不需要新逻辑。

5. **statMap（stat id → mod 映射，SkillStatMap.lua + 各技能局部覆盖）**：把 `total_cast_time_+_ms` 这类 stat 翻成 mod 的映射表。pobr 在 pobr-build/src/skill_stat_map.rs 用 Rust 硬编码了一份子集——**这是当前最大的一块「数据混在框架」**，PoB2 该表数千行且带每技能覆盖；建议把映射规则 JSON 化（stat id → {mod 名, 类型, flag, 除数}），框架只实现映射解释器，每技能覆盖作为数据条目。

6. **ModFlag/KeywordFlag 位表**（Data/Global.lua:222-292）：位值本身是框架枚举（pobr 已对齐 KeywordFlag 位值），但**武器类型 → 位**的派生依赖 weaponTypeInfo 数据，扩位后无新逻辑。

### 属于「逻辑」的部分（留在框架 Rust）

- 聚合管线（base×inc×more、MORE 逐 mod round）、转换链算法（两阶段折叠/归一）、暴击管线、命中公式、双 pass（MH/OH、crit/non-crit）与 combineStat 合并语义、DoT 公式骨架、DPS 末端组装（TotalDPS/CombinedDPS 公式形状）。这些在 PoB2 也是手写 Lua（CalcOffence/CalcActiveSkill 本体），与版本数据解耦，正是 pobr 框架该承载的部分。

- **灰色地带**：PoB2 的 `runSkillFunc("preDotFunc")` 与 SkillStatMap 中内嵌的 per-skill Lua 函数（如 Molten Strike 三级半径、Explosive Arrow 引信、trauma）是「代码即数据」——无法纯 JSON 化，建议在 Rust 框架建一个按 skill id 注册的 special-case registry（行为留框架、触发开关与参数走 JSON），并在 sync-pob-catalog 的 parity 检查里枚举哪些技能带 skillFunc 以防漏。

### PoB2 现状的混合点（复刻时需重构）

CalcOffence.lua 把 trauma/Explosive Arrow/Molten Strike 等具体技能逻辑直接写进通用管线（L2738-2821、L3614），并在管线中间随手 NewMod 注回 modDB（如 `Multiplier:SustainableTraumaStacks`、`Multiplier:ExcessHitChance` L2623）——pobr 复刻时应把这类「管线中途注入」收敛为显式 pre-pass，否则纯函数/确定性约定会被破坏。

### pobr 当前 JSON schema 缺口汇总（按优先级）

1. `GrantedEffectDef.weapon_types[]`（可用武器限制）
2. `SkillStatSetLevel` 的 dot 基值族与 dotIs* 旗标
3. `SkillLevelDef.reload_time_ms` / `bolt_count`
4. `GrantedEffectDef.parts[]`（多形态）
5. radius 三级字段
6. `weapon_type_info` 表
7. per-class unarmed 表
8. misc 常量域（AccuracyFalloff* / CullingThreshold 等）
9. statMap 规则表（最大单项，建议独立设计）

---

## 附录：核查说明

核查范围：全部 6 条 high + 4 条 medium 抽查（命中链/攻速链/DealNo/LuckyHits）+ 2 条 low 抽查（Culling/skillPart），共 12 条，每条都打开了 PoB2 Lua 原文与 pobr Rust 实现，并对 pobr 侧做了全局 grep（crates/apps/tools）防止「在别处实现」误判。先读了 `audits/pob2-parity-2026-06-09/FINDINGS.md` 确认无重复（FINDINGS 覆盖的是 MORE round/ModFlags 子集语义/穿透注入/traced 路径等，与本报告的双 pass/Double Damage/技能 DoT/dpsMultiplier 等结构性缺口不重叠）。

**修正 1 条**：Gap「无 MH/OH 双 pass」——PoB2 combineStat 的 DPS 模式实测为 `(MH+OH)`、非 doubleHitsWhenDualWielding 时再 `/2`（CalcOffence.lua:2534-2538），因此原 detail 断言「双持 build DPS 系统性偏低约一半」不成立——当双手武器相近时 MH-only ≈ (MH+OH)/2，误差反而小；真实影响改写为「副手与主手差异不可控的偏差 + per-hand 条件词条失效 + doubleHitsWhenDualWielding 技能偏低一半 + Speed/Crit/异常合并语义缺失」。severity 维持 high（结构性缺口本身查实：calc_orchestrator.rs `weapon_contribution` 确实只读 Weapon1，Weapon2 仅用于盾/DualWielding 条件）。pobr_ref 行号由 1102-1146 校正为 1104-1146；combineStat 行号由 2451 校正为 2453。

**查实保留 5 条 high**：(2) ModFlags：pobr-data/src/modifier.rs:38-42 确仅 5 位，Global.lua:222-259 确有 Hit/Dot/Cast/Thorns/Ailment/MeleeHit/Weapon + 15 武器类型位 + 4 类别位（报告原文漏了 Fishing/Talisman，已补全引用，不影响结论）；mod_parser.rs:1016-1027 证实 condition 近似路径存在。(3) crit 双 pass：CalcOffence.lua:3978-3980 `for pass=1,2; skillCond["CriticalStrike"]=(pass==1)` 原文确认，pass1 allMult 乘 CritMultiplier（L4030）；pobr offence.rs:291 确为 non_crit_hit_avg × crit.effect 单因子。(4) Double/Triple：L3842-3861 原文逐行确认（含 Triple 抵扣 Double、ScaledDamageEffect），pobr 全局 grep 零命中。(5) 技能 DoT：L5905-5931 公式原文确认（baseVal×inc×more×DotMultiplier×aura×effMult、DotCanStack、trap/mine 速率切换），pobr 仅 ailment DoT，catalog.rs 无 dot 字段，DotMultiplier/TotalDotDPS grep 仅命中常量注释。(6) dpsMultiplier/quantityMultiplier：L3863/3127-3130/4407 原文确认，pobr offence.rs:295 公式确无二因子、仅 defer 注释（ailment.rs:1012、calc_orchestrator.rs:199）。

**查实保留 4 条 medium**：命中链（L2556-2671 全部子项原文核对：距离衰减/AccuracyVsEnemy/hitChanceMod/ExcessHitChance/reduceEnemyBlock/CannotBlockAttacks，pobr offence.rs:254-273 确实只有三步；并核实 m_floor vs 无 floor 细节）；攻速链（L2724-2852 核对四个子项，pobr action_speed_mod 确为无条件乘、无 round(,2)）；DealNo（L2226-2230 原文确认，pobr grep 零命中）；LuckyHits（L4034-4044 原文确认，damage.rs avg() 确为中点；与 crit.rs CritChanceLucky 确为两个机制）。

**查实保留 2 条 low**：Culling/Impale/Mirage 与 skillPart：pobr grep 仅注释级命中、catalog 无 parts 字段，均成立。

无删除、无降级；唯一实质修正是 gap 1 的影响量化表述与若干行号精化（quantityMultiplier 3128→3127-3130、canDeal 2230→2226-2230、totem action speed 2845→2846-2852、damage.rs avg 100-103→99-102）。
