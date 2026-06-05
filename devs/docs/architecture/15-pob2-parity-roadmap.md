# PoB2 替换路线图（2026-06-06，7-agent 评估）

> 用本地 vendor PoB2 Modules 对照 PoBR 综合。引擎不是瓶颈，数据装配+编排是。

# PoBR → 替换 PoB2 分阶段执行路线

> 综合 7 维 parity 评估 · 2026-06-06 · 路径已用真实仓库校正（评估 JSON 中的 `posebr-` 前缀均为 `pobr-` 笔误）

---

## 0. 现状总评（诚实版）

**一句话**：PoBR 已经把 PoB2 的「**Modifier 聚合 + 归因 + 计算骨架**」这条难走的路走通了，但「**装备/技能/天赋的真实数值喂进计算引擎**」这条看似简单的路只走了一半——结果是引擎能算，却**算不准任何一个真实 build**。

### 能做（已落地，是真实资产）
- **Modifier 全链路**：文本解析 → ModDb 聚合（`(base+Σbase)*(1+Σinc/100)*Π(1+more/100)`）→ source-level 归因（direct/marginal/interaction，PoB 没有的增量）。
- **计算骨架**：offence（life/mana/抗性/暴击/命中/DPS）、defence（armour/evasion/ES/block/EHP）、机制阶段（抗性边界/技能时间/伤害向量/异常/预留·恢复·格挡·抑制）。
- **Build Code 兼容**：真实 PoB2 ninja code 已验证编解码。
- **数据管线基座**：base_items（4873 个）、mods、stats、i18n 已入库（pinned `4.5.0.3.4`）。

### 不能做（阻塞真实 build 对齐的硬缺口）
| 缺口 | 后果 | 阻塞 |
|---|---|---|
| **武器基底伤害从未装配进 actor**（只解析词条，`item.rs`/`add_item` 零 weapon 引用，`WeaponBaseStats` schema 存在但无人调用） | 任何攻击 build 的 base hit = 0，DPS 失真 | attack |
| **SkillStatMap 只覆盖伤害族**（speed/aoe/duration/accuracy/cost 全缺，测试第168行明确断言 AoE 返回 None） | 攻速/法速不进 DPS、AoE/时长不生效 | both |
| **perform 编排仅 ~30% 成熟**（无 condition flags / buff 状态 / flask / exposure） | Fortify/Onslaught/Rage/暴露/药剂全部不计 | both |
| **无 buff/aura/curse 集合** | 光环、诅咒、增益不参与 | both |
| **防御池模拟缺失**（无 Ward/Aegis/Guard/MoM/reducePoolsByDamage） | EHP 是线性近似，重甲/法力护盾 build 严重偏差 | attack(def) |
| **游戏数据层缺失**（无 unique / minion / cluster 库；patch 落后于 live `4.5.1.1.3`） | 召唤/独占装/星团 build 无法构建 | both（外部阻塞） |

### 离替换 PoB2 的整体距离
- **攻击 build 核心 DPS 对齐**：约 **3 个关键 PR** 距离（武器基底 + 速度族 + condition flags）。这是性价比最高的一段路。
- **法术 build 核心 DPS 对齐**：紧随其后（法速族 + buff 状态 + 光环集合）。
- **防御/EHP 高保真**：最长的一段（reducePoolsByDamage 是 L-effort，依赖整套池模型）。
- **全 build 覆盖**：受**外部数据下载**阻塞（unique XL-effort、minion L-effort、patch 同步是 upstream 依赖）。

**结论**：当前不能替换 PoB2 做任何真实规划，但**距离「一个裸装攻击 build DPS 对齐 95%」非常近**。引擎不是瓶颈，**数据装配与编排是瓶颈**。

---

## 1. 关键路径（最短任务链）

### 链 A — 典型攻击 build 核心 DPS 对齐（按依赖严格排序）

| # | 任务 | 文件:函数 | effort | 串/并 |
|---|---|---|---|---|
| A1 | **注入武器基底伤害** 为 `base_hit_min/max`（从 `Item.base_stats.weapon` 解包，应用品质标量，作 BASE modifier 注入 PhysicalMin/Max） | `pobr-core/src/item.rs:ingest_section` + `session.rs:add_item`(weapon path) | M | 串(根) |
| A2 | **虚拟攻速**：`1000/speed_ms → attack_rate`，注入 BASE 或 `ActorBaseStats.action_rate` | `pobr-core/src/item.rs:ingest_section`(weapon) | S | 串(依A1) |
| A3 | **品质标量仅作用物理**（quality × (1+physInc%)） | `pobr-core/src/item.rs`(weapon) | S | 串(依A1) |
| A4 | **速度族 SkillStatMap**：`attack_speed_+%` / `attack_and_cast_speed_+%` → Speed INC/MORE + Attack flag | `pobr-build/src/skill_stat_map.rs:map_skill_stat`(新增 `map_speed_percent`) | S | **并**(独立文件) |
| A5 | **condition flags**：`doActorAttribsConditions`（UsingAxe/DualWielding/Unarmed/UsingShield… 30+） | `pobr-core/src/calc/perform.rs`/`env.rs`(新 fn) | M | 串(calc 核心，门控 A6/buff) |
| A6 | **武器暴击基底**：crit_chance → BASE CritChance | `pobr-core/src/item.rs`(weapon) | S | 并(依A1) |
| A7 | **AoE 族**（`base_skill_area_of_effect_+%` → AreaOfEffect INC，读取点 `skill_mechanics.rs:calculate_aoe` 已就绪） | `pobr-build/src/skill_stat_map.rs`(新增 `map_aoe_percent`) | S | 并 |
| A8 | **黄金回归测试**：裸装单武器 build DPS vs PoB2 ≥95% | `pobr-core/tests/calc_session.rs`(新 `weapon_base_attack_dps`) | M | 串(终点，依 A1–A6) |

**链 A 总 effort：~3M + 4S + 测试**。完成即解锁「攻击 build 裸装 DPS 对齐」——**最高优先级，单条链可独立交付**。

### 链 B — 典型法术 build 核心 DPS 对齐（复用链 A 大部分）

| # | 任务 | 文件:函数 | effort | 依赖 |
|---|---|---|---|---|
| B1 | **法速族**：`cast_speed_+%` / `cast_speed_+%_granted_from_skill` → Speed INC/MORE + Cast flag | `pobr-build/src/skill_stat_map.rs:map_speed_percent` | S | 与 A4 同函数（合并做） |
| B2 | **时长族**：`skill_effect_duration_+%` → Duration INC/MORE（读取点 `ailment.rs:resolve_ailment_duration` 已就绪） | `pobr-build/src/skill_stat_map.rs`(新增 `map_duration_percent`) | S | 并 |
| B3 | **buff/aura/curse 集合**：Env 增加 buffs/debuffs/curses 容器 | `pobr-core/src/calc/env.rs` | S | 串(calc 核心，门控 B4) |
| B4 | **buff 状态解析**：Fortify/Onslaught/Rage/Adrenaline/Convergence…（设 condition flag + multiplier） | `pobr-core/src/calc/perform.rs`(新 `apply_buffs`) | L | 串(依 A5 + B3) |
| B5 | **法术回归测试**：典型法术 build DPS vs PoB2 ≥95% | `pobr-core/tests/calc_session.rs` | M | 串(依 A4/B1/B3/B4) |

> 法术 build 不依赖 A1–A3（武器基底），但**强依赖 A5(condition flags) + B3(buff 集合)**。法速族（B1）与攻速族（A4）是同一个 `map_speed_percent` 函数，应在同一 PR 一并完成。

### 关键路径瓶颈节点
- **A5 (condition flags)** 是整个 perform 编排的根门控：buff 解析、active skill limit、flask 选择性、item bonus 全部 `depends_on` 它。**优先打通 A5**。
- **B3 (buff 集合)** 是 buff/aura/curse/exposure 的数据结构前置。S-effort，应尽早做。

---

## 2. 分阶段路线

### Phase 1 — 攻击 build 裸装 DPS 对齐（地基）
**主题**：让引擎第一次能算对一个真实攻击 build 的 base hit。
**任务**：A1, A2, A3, A4, A6, A8（武器基底装配 + 速度族 + 暴击基底 + 回归测试）。
**能力跃迁**：从「base_hit=0 永远算不准」→「裸装单武器 build DPS 对齐 PoB2 ≥95%」。**第一个可信数值**。
**并行性**：A1–A3/A6（item.rs，串行链）与 A4（skill_stat_map.rs，独立）可双线并行。

### Phase 2 — perform 编排核心（攻击 build 加 scaling）
**主题**：让词条/天赋的增益真正按 PoB2 规则应用。
**任务**：A5(condition flags) → A2-limits(active skill limit) → exposure → item bonus scaling。
**能力跃迁**：从「裸装对齐」→「带词条/天赋的攻击 build 对齐」（DualWielding/Unarmed 条件 mod 生效，暴露降抗，物品 slot 加成）。
**串行**：全在 perform.rs/env.rs（calc 核心），必须串行，A5 是根。

### Phase 3 — 法术 build + buff/光环/诅咒
**主题**：覆盖第二大 build 类型。
**任务**：B1(法速) + B2(时长) + B3(buff 集合) + B4(buff 状态) + flask/charm 合并 + enemy debuff。
**能力跃迁**：从「只有攻击」→「攻击+法术双 build 类型 + Fortify/Onslaught/Rage/光环/药剂生效」。覆盖绝大多数 league-start build。
**部分并行**：B1/B2（skill_stat_map）独立于 B3/B4（perform，calc 核心）。

### Phase 4 — 防御/EHP 高保真
**主题**：把 EHP 从线性近似升级为 PoB2 池模型。
**任务**：Ward → Aegis → Guard → MoM → `reducePoolsByDamage`(L) → recovery rate mod → ES recharge cap → defence estimations(max hit brackets)。
**能力跃迁**：从「简单线性吸收」→「按池优先级模拟伤害吸收 + max hit 分档」。法力护盾/重甲/守护类 build 防御对齐。
**串行**：全在 defence.rs/ehp.rs，`reducePoolsByDamage` 是核心枢纽。

### Phase 5 — 游戏数据层补全（外部阻塞）
**主题**：解锁需要专属数据的 build 类型。
**任务**：patch 同步 `4.5.1.1.3`(S, upstream) → base_items 武器基底校验(M) → ClusterJewels(M) → Minions(L) → Uniques(XL) → Spectres/Flask/Rune(M)。
**能力跃迁**：召唤 build / 独占装 build / 星团珠宝 build 可构建。
**并行**：数据适配器（pipeline + data-adapter）完全独立于 calc 核心，**可与 Phase 1–4 全程并行**。但每项**阻塞于数据下载/提取**（标注为外部阻塞）。

### Phase 6 — 高级机制（长尾）
**主题**：触发/分身/复杂召唤。
**任务**：triggers & mirages(XL, CWC/cooldown/mirage archer) + create_minion_skills(L) + 暴击条件型 stat + 异常 DoT flat。
**能力跃迁**：触发流/CoC/分身/spectre build 完整。这是替换 PoB2 的「最后 5%」。

---

## 3. 即刻可执行的前 3 项

> 全部**不阻塞、单 PR 可验证、互不冲突**（一个动 item.rs，一个动 skill_stat_map.rs，一个动 pipeline/config）。

### #1 — 武器基底伤害装配（链 A1–A3，最高性价比）
**做什么**：在 `pobr-core/src/item.rs:ingest_section` 的 weapon 分支，从 `Item.base_stats.weapon`（`WeaponBaseStats`，schema 已在 `pobr-data/src/catalog.rs:62`，目前**零调用**）解包 physical_min/max + attack_rate(`1000/speed_ms`) + crit_chance，应用品质标量（仅物理），注入为对应 BASE modifier。对照 `vendor/PathOfBuilding-PoE2/src/Classes/Item.lua:1732-1773`。
**验收标准**：
- 装备一把已知基底武器（如 dagger，physical 5–10, crit 6%, rate 1.5），空 build，`total_hit_avg` DPS 与 PoB2 GUI 同 build ≤2% 偏差。
- 同基底两把武器（0% vs 20% quality），物理伤害差 ~20%；元素伤害不受 quality 影响。
- 新测试 `weapon_base_attack_dps` 入 `pobr-core/tests/calc_session.rs` 并通过 CI gate（fmt+clippy+test）。

### #2 — 速度族 + AoE + 时长 SkillStatMap（链 A4/A7 + B1/B2，一次性扫掉 4 族）
**做什么**：在 `pobr-build/src/skill_stat_map.rs:map_skill_stat` 新增 `map_speed_percent`（攻速/法速/`attack_and_cast_speed`）、`map_aoe_percent`、`map_duration_percent` 分支。读取点 `calculate_aoe`/`resolve_ailment_duration` 已就绪。对照 `SkillStatMap.lua:551,1998,2001,559,636,651,746,762`。
**验收标准**：
- 现有测试第 168 行 `assert!(map_skill_stat("base_skill_area_of_effect_+%").is_none())` 翻转为返回 `Some(AreaOfEffect, INC)`，测试同步更新。
- 新增单测：4 族各 ≥1 个 stat key 映射到正确 ModName+ModType（speed→Speed+Attack/Cast flag、aoe→AreaOfEffect、duration→Duration）。
- 一个带 `attack_speed_+%` 词条的 build，DPS 随攻速线性变化（端到端验证速度进 DPS）。

### #3 — pipeline patch 同步到 live `4.5.1.1.3`（外部阻塞解锁，S-effort）
**做什么**：`pipeline/config.json:2` 从 `4.5.0.3.4` 更新到 live `4.5.1.1.3`，重跑数据管线（`download-index.mjs` + data-adapter），落到 `data/4.5.1.1.3/`。
**验收标准**：
- `data/4.5.1.1.3/` 生成，manifest 反序列化通过 `pobr-gamedata::GameData::new` 无 schema 错误。
- base_items/mods/stats 计数与新 patch 一致，i18n lint 通过。
- ⚠️ **外部阻塞标注**：若官方 `.dat` 导出或 upstream vendor 尚未推进到 `4.5.1.1.3`，此项卡在数据下载——**先验证数据源可得性**，不可得则降级为「校验现有 `4.5.0.3.4` 武器基底是否完整覆盖所有武器类型」（链 Phase 5 的 base_items 校验，M-effort，无外部依赖）。

---

## 4. 并行流 vs 串行（calc 核心）划分

### 独立可并行流（不碰 calc 核心，可全程并行推进）
| 流 | 文件域 | 内容 | 阻塞源 |
|---|---|---|---|
| **数据装配流** | `item.rs`(weapon 解包) | 武器/护甲基底注入（A1–A3, A6） | 无（schema 已存在） |
| **SkillStatMap 流** | `pobr-build/skill_stat_map.rs` | 速度/AoE/时长/命中/消耗族映射（A4,A7,B1,B2） | 无（纯映射表） |
| **数据管线流** | `pipeline/`, `tools/pobr-data-adapter/` | patch 同步 / unique / minion / cluster / spectre 库 | **外部**（数据下载、upstream patch） |
| **i18n 流** | `pobr-i18n/`, 语言包 | 新 stat 显示文本 | 跟随新 ModName |

> 这四条流互不冲突，可由不同人/不同 PR 同时推进。SkillStatMap 流尤其「纯加法」、风险最低。

### 必须串行（calc 核心，共享 perform/env/defence 可变写入）
```
A5 condition flags (perform.rs/env.rs)  ← 根门控
   ├─→ active skill limit
   ├─→ B3 buff 集合 (env.rs) ─→ B4 buff 状态 (apply_buffs)
   │                              ├─→ enemy debuff
   │                              └─→ flask/charm 合并
   ├─→ exposure
   └─→ item bonus scaling

defence 池模型 (defence.rs) — 内部串行:
   Ward → Aegis → Guard → MoM → reducePoolsByDamage → ehp 集成
```
**理由（CLAUDE.md 约定）**：calc 函数对 `Env` 的可变写入集中在 `perform`，并行化只在只读快照阶段展开。perform 编排有严格 `depends_on` 链（condition flags → buff → flask），**不可乱序**。defence 池模型按吸收优先级（allies→aegis→guard→ward→ES/MoM→life）严格串行。

### 务实排序总原则
1. **先解锁最多真实 build 数值对齐的**：武器基底（#1）+ 速度族（#2）单独就让攻击 build 从「完全失真」跳到「裸装 95% 对齐」——投入产出比最高，**立即做**。
2. **calc 核心串行链尽早打通 A5**：它门控 perform 一半的后续任务。
3. **数据管线流与 calc 流全程并行**，但数据流**受外部下载阻塞**——尽早验证数据源可得性，不可得就先做无外部依赖的 base_items 校验。
4. **防御池模型（Phase 4）和高级机制（Phase 6）押后**：effort 大（L/XL）、覆盖的 build 类型相对窄，等核心 DPS 对齐稳定后再投入。

---

### 关键文件索引（绝对路径）
- `/Users/wuyong/codes/game/pobr/crates/pobr-core/src/item.rs` — `ingest_section`（武器基底注入点，#1）
- `/Users/wuyong/codes/game/pobr/crates/pobr-core/src/calc/session.rs:63` — `add_item`（weapon path）
- `/Users/wuyong/codes/game/pobr/crates/pobr-data/src/catalog.rs:62` — `WeaponBaseStats`（schema 已存在，**零调用**）
- `/Users/wuyong/codes/game/pobr/crates/pobr-build/src/skill_stat_map.rs:46` — `map_skill_stat`（速度/AoE/时长扩展点，#2；测试断言在第168行）
- `/Users/wuyong/codes/game/pobr/crates/pobr-core/src/calc/perform.rs` — condition flags / buff / exposure 编排根（A5）
- `/Users/wuyong/codes/game/pobr/crates/pobr-core/src/calc/env.rs` — buff/aura/curse 集合（B3）
- `/Users/wuyong/codes/game/pobr/crates/pobr-core/src/calc/defence.rs:84` — `DefenceOutput`（Ward/Aegis/Guard/MoM 池模型，Phase 4）
- `/Users/wuyong/codes/game/pobr/pipeline/config.json:2` — patch 版本（`4.5.0.3.4`→`4.5.1.1.3`，#3，外部阻塞）

> 注：评估 JSON 的 `offence-parity` 维度因 vendor 路径笔误（`posebuilding-poe2`）无法评估——实际 vendor 在 `/Users/wuyong/codes/game/pobr/vendor/PathOfBuilding-PoE2/`，offence 参考实现 `src/Modules/CalcOffence.lua` 可访问，该维度应重跑。
