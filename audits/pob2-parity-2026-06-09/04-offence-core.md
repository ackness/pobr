# 04 · 伤害核心（转换 / gain-as-extra / inc-more）

**Rust 模块**：`pobr-core::calc` — `damage.rs` + `offence.rs`
**对照**：PoB2 `CalcOffence.lua`（calcDamage / calcConvertedDamage / calcGainedDamage）
**agent-docs 交叉**：§四 CalcOffence 伤害核心（4.1 转换矩阵 / 4.2 calcDamage inc·more 应用）

## 总评

总体方向正确，与 PoB2 `CalcOffence.lua` 转换/gain 管线骨架高度一致：5 类型固定顺序、源内 >100% 归一、技能先于全局两阶段折叠、gain 基于「转换后（retained+converted-into）」量且不扣源/不参与归一、PoE2「inc/more 只按最终伤害类型」（无转换源 double-dip，对齐 `calcDamage(..., typeFlags=0)` + `damageStatsForTypes`）——核对无误。但存在两处造成数值偏差的真实缺口，及两处架构性/精度差异。

---

## 04-01 · 缺失 Min<Type>Damage / Max<Type>Damage 的分 min/max MORE 乘区 — HIGH

**PoB2 行为**：`calcDamage` 对 min 和 max 各自额外乘一个独立 MORE：`CalcOffence.lua:138-139` `moreMinDamage = More(cfg,"Min"..damageType.."Damage")` / `moreMaxDamage = More(cfg,"Max"..damageType.."Damage")`，最终 `CalcOffence.lua:153-154` `round(summedMin*inc*more*moreMinDamage+addMin)` 与 `summedMax*inc*more*moreMaxDamage`。这些词条真实存在于数据：`ModCache.lua` 有 `MaxLightningDamage MORE 10`、`MinPhysicalDamage MORE -35`、`MaxPhysicalDamage MORE 35`，`sup_str.lua` 也有 `MaxPhysicalDamage MORE`。语义是「更多/更少 最小/最大 某类型伤害」，仅缩放区间一端、改变平均与离散度。

**PoBR 现状**：`damage.rs` `scale_with_path`（`damage.rs:250-258`）对 min 与 max 用同一个 `scale=(1+inc/100)*more`，`aggregate_inc_more`（`damage.rs:265-306`）只聚合 `<Type>Damage`/`Damage`/`ElementalDamage`/类别名，从不读取 `Min<Type>Damage`/`Max<Type>Damage`。`mod_parser.rs` 也无对应解析（grep `Min(Physical|Fire|...)Damage` 全仓 0 命中）。
- `crates/pobr-core/src/calc/damage.rs:250`
- `crates/pobr-core/src/calc/damage.rs:265`
- `crates/pobr-core/src/calc/damage.rs:296`

**修复方案**：在 `mod_parser.rs` 增加「minimum/maximum <Type> Damage」的 more/less 解析，映射到 `Min<Type>Damage`/`Max<Type>Damage` MORE。`aggregate_inc_more` 改为返回 `(inc, more, more_min, more_max)` 或新增专门函数；`scale_with_path` 用 `comp.min*scale*more_min`、`comp.max*scale*more_max` 分别缩放。注意这些 MORE 用 final_type 的 type-scoped cfg 求值。

---

## 04-02 · gain-as-extra 源量在「base 被完全转换走」时错误回退到原始 base — MEDIUM

**PoB2 行为**：`calcGainedDamage`（`CalcOffence.lua:90-106`）gain 源量恒为 `baseMin = floor(output[other.MinBase]*conversionTable[other].mult)`（转换后留存）`+ convertedMin`（转换进 other 的量）。当某类型 base 被 100% 转换走（mult=0 且无转入），其 gain 源量为 0，不会再用原始 base。

**PoBR 现状**：`apply_conversion_chain`（`damage.rs:557-567`）`src = if conv_min[mid]!=0||conv_max[mid]!=0 { conv_min[mid] } else { base_min[mid] }`。当 `conv_min[mid]==0`（该类型被完全转换走、也无转入）时回退到 `base_min[mid]`（原始未转换 base），与 PoB2 的「源量=0」不一致，会高估 gain。仅在「100% 转换且对该类型仍有 gain-as-extra」的边角场景触发，但方向是错的（凭空多出 gain）。
- `crates/pobr-core/src/calc/damage.rs:557`
- `crates/pobr-core/src/calc/damage.rs:563`

**修复方案**：去掉 base 回退：gain 源直接取 `conv_min[mid]`/`conv_max[mid]`（Rust 的 convert 矩阵对角线已含 retained mult，等价于 PoB2 `baseMin*mult+convertedMin`）。即便在无任何转换的纯 gain 场景，`convert[mid][mid]` 应为 1.0（fold 阶段 skill_mult=global_mult=1），`conv_min[mid]` 已等于 base，故无需 fallback。补一条单测：某类型 100% 转走 + 同源 gain-as-extra，断言不再凭空产出。

---

## 04-03 · AttackDamage/SpellDamage 用独立 ModName，而非 PoB2 的 Damage + ModFlag — LOW

**PoB2 行为**：PoB2 没有独立的 `AttackDamage`/`SpellDamage` 名：`increased attack damage` 解析为 ModName `Damage` 携带 `ModFlag.Attack`（L592 `NewMod("Damage","INC",...,ModFlag.Attack)`）。`calcDamage` 的 `damageStatsForTypes[typeFlags]` 只产出 `Damage`/`<Type>Damage`，Attack/Spell/Melee/Projectile/Area 过滤靠 `cfg.flags` 在 Sum/More 内部按 ModFlag 命中。

**PoBR 现状**：`mod_parser.rs:1109-1110` 把 `attack damage`→`AttackDamage`、`spell damage`→`SpellDamage`，`aggregate_inc_more`（`damage.rs:269-279`）再按 cfg.flags 选择性 push。只要解析与聚合两侧 ModName 一致，type-agnostic 的攻击/法术伤害结果正确。风险在带类型复合词条（如「increased Physical Attack Damage」）：PoB2=`PhysicalDamage`+ModFlag.Attack，会同时被 `PhysicalDamage` 桶与 Attack 过滤命中；若 Rust 单独映射成另一名或丢了 type 维度，则与 `PhysicalDamage` 聚合路径错配。
- `crates/pobr-core/src/calc/damage.rs:269`
- `crates/pobr-core/src/mod_parser.rs:1109`

**修复方案**：本身不是错误（内部自洽即可），但建议核对带类型+类别复合词条（Physical Attack Damage / Fire Spell Damage 等）解析是否落到 type 桶并携带正确 flag，避免与 `<Type>Damage` 聚合路径分叉。可加测试：`increased Physical Attack Damage` 应被攻击物理分量吃到、被法术/其它类型分量忽略。

---

## 04-04 · per-type base 用 round，PoB2 gain 源用 floor — LOW

**PoB2 行为**：PoB2 per-type base（`output[..MinBase]`）不取整（L3910-3911 原始浮点）；`calcConvertedDamage` 累加后 `round`（L83-84）；`calcGainedDamage` 的源 base `m_floor(...*mult)`（L95-96）；SummedMinBase `round`（L3945-3946）。即转换累加用 round、gain 源用 floor。

**PoBR 现状**：`damage.rs` `base_flat` 返回未取整值；`apply_conversion_chain` 在组装最终分量时对 `conv_min+gain_min` 统一 `round`（`damage.rs:587-588`），gain 源量未做 floor。少量 off-by-1 量级差异，对面板 DPS 通常 <0.1% 可忽略，但与 PoB2 golden 逐值核对时可能产生末位差。
- `crates/pobr-core/src/calc/damage.rs:587`
- `crates/pobr-core/src/calc/damage.rs:148`

**修复方案**：若追求与 PoB2 headless oracle 逐值 bit-parity，可在 gain 源量处对 `conv_min[mid]*mult` 这一步引入 floor 语义；否则保持 round 即可，归类为已知微小取整差异，建议在 oracle 容差内接受。

---

## 04-05 · base 附加伤害未计入敌人 Self<Type>Min/Max — INFO

**PoB2 行为**：`CalcOffence.lua:3907-3908` `addedMin = skillModList:Sum("BASE",cfg,damageTypeMin) + enemyDB:Sum("BASE",cfg,"Self"..damageTypeMin)`——附加伤害含敌人自带的 `Self<Type>Min/Max`（如某些「对敌人造成额外…」机制）。

**PoBR 现状**：`damage.rs` `base_flat`（`damage.rs:148-176`）只读玩家 db 的 `<Type>DamageMin/Max`，不读 enemy_db 的 `Self<Type>Min/Max`。属敌人侧增伤的边角机制，主流 build 不触发。
- `crates/pobr-core/src/calc/damage.rs:158`

**修复方案**：优先级低。若后续接入敌人侧 Self 附加伤害，需把 enemy_db 传入 `base_flat` 并叠加 `Self<Type>DamageMin/Max` BASE。当前可不动。
