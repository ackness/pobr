# 跨领域缺口汇总（2026-06-10 重构审计）

> 数据来源：[10–19 各领域文档](00-README.md#3-文档索引)的缺口清单汇总。严重级口径见 [00-README.md §2](00-README.md#2-方法)。

---

## 1. 总览表

| 领域 | 🔴 high | 🟡 medium | 🟢 low | 文档 |
|------|:---:|:---:|:---:|------|
| Modifier 系统（解析/存储/聚合） | 3 | 4 | 1 | [10-mod-system.md](10-mod-system.md) |
| 计算编排（Setup/Perform/环境） | 2 | 7 | 2 | [11-calc-orchestration.md](11-calc-orchestration.md) |
| 进攻计算（伤害/暴击/速度/DPS） | 6 | 6 | 2 | [12-offence.md](12-offence.md) |
| 防御计算（护甲/闪避/ES/减伤/EHP） | 5 | 7 | 4 | [13-defence.md](13-defence.md) |
| 触发/召唤物/异常状态 | 6 | 5 | 1 | [14-triggers-minions-ailments.md](14-triggers-minions-ailments.md) |
| 数据层与导出管线（数据/框架分离） | 5 | 5 | 3 | [15-data-pipeline.md](15-data-pipeline.md) |
| 物品系统 | 4 | 8 | 3 | [16-items.md](16-items.md) |
| 天赋树 | 3 | 5 | 4 | [17-passive-tree.md](17-passive-tree.md) |
| 技能与宝石 | 4 | 4 | 2 | [18-skills-gems.md](18-skills-gems.md) |
| 配置/Build/展示层 | 3 | 7 | 2 | [19-config-build-display.md](19-config-build-display.md) |
| **合计** | **41** | **58** | **24** | 共 123 条 |

## 2. 全部 🔴 high 缺口一句话清单（按领域分组）

### Modifier 系统（[10](10-mod-system.md)）
- ModParser 六张 pattern 表只移植了极小子集，且全部硬编码在 Rust 里——解析覆盖面与"数据/框架分离"双重缺口。
- specialModList（2085 个特殊词条模板 + data 驱动的 per-gem/keystone 派生模式）基本缺失。
- EvalMod 的 tag 求值类型：PoB2 20 种 vs pobr 5 种；缺 actor 引用、PerStat 读 output、globalLimit 等核心语义。

### 计算编排（[11](11-calc-orchestration.md)）
- buff/aura/curse 的 perform 编排阶段整体缺失（含 curse 系统）。
- doActorMisc 内建 buff 语义表缺失（Onslaught/Fortify/Adrenaline 等 flag→mod 展开）。

### 进攻计算（[12](12-offence.md)）
- 无 Main Hand / Off Hand 双武器 pass 与 combineStat 合并。
- ModFlags 位集仅 5 位，缺武器类型/部位/Hit/Dot 全部维度。
- 暴击/非暴击双 pass 伤害重算缺失（只乘平均暴击因子）。
- Double/Triple Damage 与 ScaledDamageEffect 全乘区缺失。
- 通用技能 DoT（skillData `<Type>Dot`）与合并 DPS 族缺失。
- TotalDPS 缺 dpsMultiplier 与 quantityMultiplier 因子。

### 防御计算（[13](13-defence.md)）
- 承伤 taken-as 转换管线（damageShiftTable）完全缺失。
- 击中扣池顺序管线 reducePoolsByDamage 缺失（allies→aegis→guard→ward→ES→MoM→loss-prevention→life）。
- MoM / EnergyShieldProtectsMana(EB) / per-type ES bypass 全缺。
- TotalEHP 口径不同：PoB2 是 numberOfHitsToDie×单击伤害，PoBR 是 lowest max-hit。
- 防御资源转换 resourceList 管线缺失（ConvertTo/GainAs/翻倍类 flag）。

### 触发/召唤物/异常状态（[14](14-triggers-minions-ailments.md)）
- 触发 configTable（61 项 per-skill/per-unique 触发配置）未建模，CoC/CWDT/unique 触发链路缺失。
- 触发源速率用基础 use_time，未用计算后的攻速/施速（PoB2 用缓存子计算 HitSpeed/Speed）。
- Minions/Spectres 数据未 JSON 化：pobr 仅 4 条手抄 Rust 常量，数据管线（adapter/gamedata）对 minion 零支持。
- 召唤物未接入 build 链路：calc_orchestrator 不识别召唤物宝石，真实 build 召唤物面板恒空。
- 召唤物技能（createMinionSkills）未建模：法术系召唤物 DPS 无来源，全部按虚拟武器白板攻击算。
- 非伤害异常消费侧闭环缺失：敌方施加循环（Override/叠层/Bonechill/Condition）+ Shock→enemy DamageTaken 不影响玩家 DPS。

### 数据层与导出管线（[15](15-data-pipeline.md)）
- stat_descriptions（statdesc）渲染链路完全缺失。
- SkillStatMap 被固化为框架内 Rust 启发式，954 条显式映射仅覆盖少数族。
- PoB2 Export 模板的人工策展层（#baseMod/#flags/#set + 每技能 statMap override）无系统性通道，已出现一次性手工补丁。
- Misc.lua 全局常数与怪物等级表硬编码为 Rust 常量。
- 宝石 qualityStats 数据缺失，宝石品质对计算无效。

### 物品系统（[16](16-items.md)）
- variant 词条无门控：多 variant unique 的所有变体词条全部注入。
- range 词条取值（itemLib.applyRange + ModScalability）完全缺失。
- 武器局部 mod 覆盖不全：元素 adds / 局部暴击 / LocalElementalDamage 泄漏为全局。
- BaseItemDef schema 缺关键字段：spirit / socketLimit / quality 上限 / BlockChance / MovementPenalty / ReloadTimeBase / subType / flask / charm。

### 天赋树（[17](17-passive-tree.md)）
- 武器组（WeaponSet1/2）天赋节点未解析、节点词条无 WeaponSet 条件——两套武器组天赋同时永久生效。
- 节点效果缩放管线整体缺失（PassiveSkillEffect / HasNoEffect / Jewel*PassiveSkillEffect / Time-Lost 珠宝主词条）。
- isSwitchable 节点按职业/飞升改写（ReplaceNode）完全缺失，数据与逻辑两侧皆无。

### 技能与宝石（[18](18-skills-gems.md)）
- 宝石品质（quality）链路缺失：数据表、Build 模型、XML 导入、orchestrator 接线四层皆空（core 层有未接线的归因 API）。
- support 适用性裁决未在 build 路径执行：无 exclude/后缀表达式/addSkillTypes 不动点，且 orchestrator 注入前根本不调 can_support。
- SkillStatMap 被实现为 Rust 后缀启发式而非数据表，且 adapter 端 is_mappable_stat 二次白名单过滤造成不可恢复的数据丢失。
- 多 statSet / additionalStatSet（PoE2 的 skill-part 等价物）未建模：每个效果只入库主 statSet。

### 配置/Build/展示层（[19](19-config-build-display.md)）
- ConfigOptions 带定制 apply 的条目语义未建模，仅靠命名前缀导入。
- customMods 自定义词条完全不导入。
- enemyIsBoss 及 Enemy Stats 数值覆盖项不从 build XML 读取。

## 3. 对计算正确性影响最大的 10 个缺口（排序判断）

> 排序口径：**(影响的 build 占比) × (单 build 内的数值偏差幅度) × (是否上游缺口——上游错则下游全错)**。这是审计方的主观排序，路线图（21-roadmap.md）排期时还需叠加实现成本与依赖关系。

| # | 缺口 | 领域 | 判断理由 |
|---|------|------|----------|
| 1 | ModParser pattern 表极小子集 + specialModList 缺失（Gap 1+2 合并看） | 10 | **最上游**：解析不出的词条对计算贡献恒为 0。这是覆盖率型缺口——任何真实 build 的词条命中率直接决定全部下游数值的可信度；同时它是"数据未与框架分离"的最大单点。 |
| 2 | ModFlags 位集仅 5 位（缺武器类型/部位/Hit/Dot 维度） | 12 | 同为上游：flags 是聚合查询的过滤维度，位集缺失意味着"用斧时""持盾时""仅 Hit"类条件词条要么全收要么全丢，**双向污染**（多算+漏算）几乎所有武器 build。也是 #3 双武器 pass 的前置。 |
| 3 | Main/Off Hand 双武器 pass 与 combineStat 缺失 | 12 | 双持/武器组是 PoE2 核心机制；没有 per-hand pass，双持 build 的 DPS 结构性错误，且 17 号领域的 WeaponSet 天赋缺口依赖同一套机制。 |
| 4 | buff/aura/curse perform 编排阶段整体缺失 | 11 | 光环/诅咒/内建 buff（Onslaught/Fortify 等）几乎出现在所有成型 build 里，缺失意味着面板系统性偏低，且偏差不是常数（与 build 强耦合），无法事后修正。 |
| 5 | 防御扣池管线缺失（taken-as + reducePoolsByDamage + MoM/EB/bypass，13 号 Gap 1–3 合并看） | 13 | 防御侧的"伤害→资源池"链路整体不存在，EHP/max-hit 对任何带 ES/MoM/护体转换的 build 都是错的；TotalEHP 口径差异（#9）也根植于此。 |
| 6 | support 适用性裁决不执行 + 多 statSet 未建模 | 18 | 不该生效的辅助宝石照常注入、技能 part 全部按主 statSet 算——DPS 的"输入选择"层面就错了，比乘区缺口更难被用户察觉。 |
| 7 | variant 词条无门控 + 武器局部 mod 泄漏为全局 | 16 | 物品是 build 数值的最大来源之一：variant 全注入直接多算成倍词条；局部元素 adds/暴击泄漏为全局会同时污染其他武器/技能的计算。属于"结果看似合理实则错"的危险类别。 |
| 8 | 暴击/非暴击双 pass 重算缺失 + Double/Triple Damage 乘区缺失 | 12 | 暴击 build 占比高；平均暴击因子近似在 ailment/特效（暴击附带异常、Lucky 等）路径上误差被放大，且 Double Damage 是常见 unique/天赋乘区，缺失即恒定低估。 |
| 9 | 召唤物全链路缺失（数据 JSON 化 + build 接线 + createMinionSkills） | 14 | 影响面限定于召唤 build，但对这类 build 是 0% 可算（面板恒空/白板攻击），属于"整个职业流派不可用"级别；排在第 9 仅因 build 占比而非偏差幅度。 |
| 10 | 宝石品质链路缺失 + qualityStats 数据缺失 | 18/15 | 几乎所有真实 build 宝石带品质；偏差方向单一（恒低估）且幅度有限（每颗 ~5-20%加成），但因为覆盖面是 100% 的 build，累积偏差不可忽略；同时是 ninja_parity 门禁收紧阈值前的必修项。 |

落选说明（high 但未进前 10）：触发 configTable（影响限触发 build，且上一轮已打通内建触发主链路）、节点效果缩放/ReplaceNode（影响限特定珠宝/职业组合）、statdesc 渲染链路（主要影响展示与数据管线再生能力，不直接改变计算数值）、ConfigOptions 定制 apply / customMods（用户可用 mod 文本绕过，偏可用性）。这些在 21-roadmap.md 中按依赖关系另行排期。
