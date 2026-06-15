# M6 fork(a) 参考：PoB2 local-mod / 装备防御解析机制研究

> 触发：M6.3 切换实测回归（`druid-oracle-comet` Armour 1460→0），且四诊断通道
> （unsupported / Armour mods / Defences mods / dropped）engine 与 legacy **逐字节相同**——
> 回归非 parser 问题，而在 ingest 路径。owner 定方向 = fork(a)（数据化对齐 PoB2），
> 先研究 PoB2 怎么做。本文记录 PoB2 v0.18.0（vendor `2df5a74`）的权威机制。

## vendor 同步评估（2026-06-15）

- upstream = `github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2`（注意无连字符
  的 `Community`）；vendor 当前 = v0.18.0 / `2df5a74`；upstream `dev` HEAD = `a82a33b`
  （约晚 11 天）。
- ModParser.lua 差异 196 行、CalcDefence.lua 29 行，**多为正则微调**
  （`rating`→`r?a?t?i?n?g?` 可选化）+ ailment 免疫处理，**不涉及** local AEE / 基底
  armour 计算——对本回归无关。
- **结论：不 bump vendor**。理由：(1) 差异与 bug 无关；(2) bump 会让
  `regenerated_matches_committed_artifact` 门禁红（regen 钉死当前 vendor）；(3) 与
  `data/4.5.0.3.4/` + parity goldens（皆 v0.18.0 基线）脱钩——属 version-bump-drill
  专项，非随手同步。latest dev 已 clone 到 `/tmp/pob2-latest` 仅作参考。研究用当前
  vendor（与数据/golden 同版，最可靠）。

## PoB2 local-mod 判定：`calcLocal`（Classes/Item.lua:1655）

```lua
-- To be considered local, a modifier must be an exact flag match, and cannot have
-- any tags (e.g. conditions, multipliers). Only the InSlot tag is allowed.
local function calcLocal(modList, name, type, flags)
  ... while modList[i] do
    if mod.name==name and mod.type==type and mod.flags==flags and mod.keywordFlags==0
       and (not mod[1] or mod[1].type=="InSlot") then
      result += mod.value   -- (FLAG: or; MORE: *(100+v)/100)
      t_remove(modList, i)  -- ★ 命中即从 modList 移除（消费掉，不再全局生效）
    ...
```

**判定铁律**：mod 为 local ⟺ `name`+`type`+`flags` 精确匹配 ∧ `keywordFlags==0` ∧
**无任何 tag**（仅允许 `InSlot`）。命中的 local mod **从列表移除**，折进该槽位的 base。
带 `Condition`/`Multiplier`/`PerStat` 等 tag 的同名 mod **不算 local**，留在列表里走全局。

## PoB2 装备 armour 解析：`BuildModListForSlotNum`（Item.lua:1684, armour 分支 ~1792）

```lua
elseif self.base.armour then
  local armourBase            = calcLocal(modList,"Armour","BASE",0) + (self.base.armour.Armour or 0)
  local armourEvasionBase     = calcLocal(modList,"ArmourAndEvasion","BASE",0)
  local armourEnergyShieldBase= calcLocal(modList,"ArmourAndEnergyShield","BASE",0)
  local armourInc             = calcLocal(modList,"Armour","INC",0)
  local armourEvasionInc      = calcLocal(modList,"ArmourAndEvasion","INC",0)
  local armourEnergyShieldInc = calcLocal(modList,"ArmourAndEnergyShield","INC",0)
  local defencesInc           = calcLocal(modList,"Defences","INC",0)   -- ★ 本地 AEE
  local qualityScalar = self.quality   -- (AlternateQualityArmour 时置 0)
  armourData.Armour = round(
      (armourBase + armourEvasionBase + armourEnergyShieldBase)
    * (1 + (armourInc + armourEvasionInc + armourEnergyShieldInc + defencesInc)/100)
    * (1 + qualityScalar/100))
  -- Evasion / EnergyShield / Ward 同构（各自 base+混合 base，inc 含 defencesInc）
```

要点：
1. `"armour, evasion and energy shield"` → modName **`Defences`**（ModParser.lua:248）。
2. 该 AEE INC 若**无 tag** → 被 `calcLocal` 当 local 消费 → 进 `defencesInc` → 乘到
   **本槽位** armour/evasion/ES 三者的 base 上 → 出 `armourData.Armour`（已解析的槽位值）。
3. 带 `Condition(CanUseBondedModifiers)` 的 `Bonded:` AEE、带 `Global` 的 → **不被
   calcLocal 消费** → 留在 modList 走**全局** `Defences INC`（对全身 armour/eva/ES 生效）。
4. quality 作为**独立末乘区** `(1 + quality/100)`，不进 inc 池。
5. `armourData.Armour` 之后由 calc 引擎按槽位求和注入全局 armour（解析后的槽位值，
   **不带** slot tag 在全局聚合层）。

## 对照 PoBR 现状 + 本回归定位

- PoBR dump：`Armour Base 328 SlotName(bodyarmour) origin=base.Armour` —— 即我们把
  **槽位解析后的 armour（328）** 表示为带 `SlotName` tag 的 BASE mod（聚合层按槽位归并）。
  engine/legacy **此值逐字节相同**（328）。
- 全局 `Defences INC`：engine/legacy 也逐字节相同（15 条件 + 30 全局）。
- 即「local AEE 折进槽位 base」「全局 Defences」两步两路一致，**却算出 Armour 1460 vs 0**。
- ⇒ 分歧在**上述四通道之外的某 ModName**（如 `Multiplier:QualityOnBody Armour`——
  PoB2 Item.lua:1722 `modList:NewMod("Multiplier:QualityOn"..slotName,...)`，若 engine
  ctx 下未产出，则 quality 末乘区错位；或 `ArmourAndEvasion`/`ArmourAndEnergyShield`
  混合 base 名下的差异），或 cfg condition/multiplier 状态差异。
- **下一步（独立 root-cause 任务）**：加 `POBR_DBG_ALLMODS` dump-all 模式 → 对
  druid-oracle-comet engine vs legacy 整玩家 ModDb **逐 mod 全集 diff** → 锁定那条
  只在一侧出现 / 值不同的 mod → 回溯其 ingest 产出点（`ingest_item_with_ctx` 在 engine
  ctx 下的本地 mod 折叠 / quality multiplier / 混合 base 处理）。

## fork(a) 实施含义（数据化对齐 PoB2）

PoBR 引擎要忠实复刻 PoB2，须保证 ingest 后的 mod 集**逐条**等于 legacy（= PoB2 语义）：
1. **local 判定**完全照 `calcLocal`：仅 `name+type+flags` 精确 + `keywordFlags==0` +
   无 tag（除 InSlot）才 local；命中即消费、不留全局。引擎产出若给本应 local 的 AEE
   挂了多余 tag（空 tag / 误加 Condition）→ local 判定失败 → 槽位 base 不缩放 + 全局
   多一条 → 双向偏差。**这是引擎 ctx 与 legacy ctx 在 `ingest_item` 里最可能的分叉点**。
2. **混合 base 名**（`ArmourAndEvasion`/`ArmourAndEnergyShield`/`EvasionAndEnergyShield`）
   + `Defences` 必须各自独立成名并都计入对应槽位 inc 池（见上式）。
3. **quality** 独立末乘区，经 `Multiplier:QualityOn<Slot>` 通道，勿并入 inc。
4. 验收 = 全 ingest 逐文本「ingest 后 ModDb 全集 diff」对 legacy 归零（不止 parsed/
   unsupported 状态），再翻 default。

## 全 ModDb diff 实测（POBR_DBG_ALLMODS，druid-oracle-comet，engine vs legacy）

加了 `POBR_DBG_ALLMODS=1` dump-all 仪表（session.all_mods + orchestrator）后对 druid
整玩家 ModDb 全集 diff：271 vs 271 条，**10 行有别**，全部非 armour 名，但揭示引擎
产出与 legacy 的真实语义分歧（parity 基线钉在 legacy 产出，故任何分歧——更全或更
错——都破回归门）：

1. **ailment 条件方向错（引擎 bug）**：
   - `Damage Inc 25`：engine `kw=0x40000 tags=[Condition{Ignited}]` vs
     legacy `kw=0 tags=[Condition{EnemyIgnited}]`；
   - `Damage Inc 35`：engine `Condition{Burning}` vs legacy `Condition{EnemyBurning}`。
   - 即「against Ignited/Burning **Enemies**」引擎错映射成**自身** Ignited/Burning + 多
     挂一个 keyword flag。**引擎 name_map/tag 数据把敌方 ailment 条件错配到自身侧**——
     fork(a) 须修这批 `Enemy<Ailment>` 条件映射（data 层）。
   - 注：CLI `parse-mod` 当前**不序列化 tags**，故 CLI 对照看不出此分歧（两侧都只印
     name/value）；唯有 `POBR_DBG_ALLMODS` dump（印 tags）可见。CLI tag 序列化缺失
     是独立小坑，登记。
2. **flask/charm 内层覆盖差**：engine 的 `FlaskBuff`/`CharmBuff` List `NestedMods`
   **已填**（Duration/FlaskRecovery/FlaskCharges/DamageTaken 等内层 mod），legacy
   **为空** `NestedMods([])`。即引擎把药剂/护符内层词条解析出来了、legacy 没有（M5
   遗留 gap）。引擎此处**更全**，但仍改变行为 → 破 parity 基线。

**Armour 1460→0 仍未由 mod 解释**：armour/defence 名下 mod 在两路逐字节相同（含
`Armour Base 328 SlotName`），上述 10 分歧均非 armour。⇒ Armour=0 是 **defence.rs
计算行为**（从 Base 328 输入算出 0），疑被 flask/charm 分歧间接触发（env_finalize
merge_flasks_charms）或独立的防御 calc 路径差异——须 **defence.rs armour 计算逐步
trace**（独立子任务）。

**fork(a) 工作清单（按此 druid 样本外推全 18 build）**：
- (A) 修 `Enemy<Ailment>` 条件映射数据（Ignited/Burning/Chilled/Shocked… 的「against
  X enemies」全族）；
- (B) 决策 flask/charm 内层：引擎更全是好事，但须么 legacy 也补齐（让 diff=0）、么把
  parity 基线迁到引擎产出（owner 决策——这等于承认引擎是新基准）；
- (C) defence.rs trace 定位 Armour→0 的计算路径分歧；
- (D) 全 18 build ModDb 全集 diff 归零后翻 default。诊断仪表：`POBR_DBG_ALLMODS`
  + `POBR_DBG_STAT` + `--features parser-engine`(engine) / `--no-default-features`(legacy)。
