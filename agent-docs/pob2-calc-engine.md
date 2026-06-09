# PoB2 计算引擎实现解构（伤害组装 · 防御构筑 · 数据来源解析）

> 本文档以 vendor 内 PoE2 版 Path of Building 源码为一手依据，逐层解构其计算引擎：从 modifier 文本解析、ModStore 聚合，到环境装配（CalcSetup）、计算编排（CalcPerform），再到进攻核心（CalcOffence）与防御核心（CalcDefence）。所有公式、伪代码均标注 `文件:行号` 引用（路径相对 `vendor/PathOfBuilding-PoE2/src/`），便于和 PoBR Rust 实现对照。
>
> **与其它 agent-docs 的关系**：`damage-scaling.md` / `damage-defence-order.md` / `critical-hits.md` / `ailments.md` 等文档描述「游戏机制是什么」；本文档描述「PoB2 用什么源码、什么聚合顺序去实现它」。机制疑问以一手数据为准，本文档负责映射到可执行的计算管线。

## 总览

PoB2 计算引擎的本质是一个**确定性的 modifier 聚合 + 装配流水线**。所有装备、天赋、宝石、buff、配置面板最终都被解析成统一的 `Modifier` 对象塞进一个或多个 `ModStore`，随后由 `CalcSetup` 装配出计算环境 `env`，`CalcPerform` 做全局编排（buff/充能/条件/属性），最后 `CalcOffence`/`CalcDefence` 在每个 active skill 上下文里把 modifier 聚合成最终 DPS 与防御指标，写入 `env.player.output`。

### 端到端数据流

```
modifier 文本 (装备/天赋/宝石/配置)
        │  ModParser.lua   ── 文本 → {name, type(INC/MORE/BASE/FLAG/OVERRIDE/LIST...), value, flags, tags}
        ▼
   ModStore / ModList  ── 按 name 索引；Sum / More / Flag / Override / List 聚合
        │  Classes/ModStore.lua + Classes/ModList.lua
        ▼
   CalcSetup.initEnv   ── 装配 env：player/enemy/minion actor、各 slot 的 modDB、activeSkillList、conversionTable
        │  Modules/CalcSetup.lua
        ▼
   CalcPerform.perform ── 全局编排：keystone 合并、属性/条件、充能、buff/aura merge、reservation
        │  Modules/CalcPerform.lua
        ▼
   ┌──────────────────────────┬──────────────────────────┐
   │ CalcOffence.offence       │ CalcDefence (life/mana/   │
   │  伤害桶→转换→inc/more→     │  ES/armour/evasion/resist │
   │  命中/暴击/异常→DPS        │  /减伤/EHP)               │
   │  Modules/CalcOffence.lua   │ Modules/CalcDefence.lua   │
   └──────────────────────────┴──────────────────────────┘
        ▼
   env.player.output  (TotalDPS / Life / EHP / Resist ... + breakdown)
```

| 阶段 | 源码文件 | 关键入口 |
|------|---------|---------|
| 文本解析 | `Modules/ModParser.lua` | `formList`、`modNameList`、`modTagList`、`parseMod` |
| 聚合存储 | `Classes/ModStore.lua` / `Classes/ModList.lua` | `Sum`/`More`/`Flag`/`Override`/`List` + `EvalMod` |
| 环境装配 | `Modules/CalcSetup.lua` | `calcs.initEnv`、`buildModListForNodeList` |
| 全局编排 | `Modules/CalcPerform.lua` | `calcs.perform`、`doActorCharges`、`mergeBuff` |
| 进攻核心 | `Modules/CalcOffence.lua` | `calcs.offence`、`calcDamage`、`calcConvertedDamage` |
| 防御核心 | `Modules/CalcDefence.lua` | `calcs.hitChance`、`armourReductionF`、EHP |

**推荐阅读顺序**（也是本文档章节顺序）：①数据来源解析与 ModStore 聚合 → ②CalcSetup 装配 → ③CalcPerform 编排 → ④CalcOffence 伤害核心 → ⑤命中/暴击/异常/DPS 组装 → ⑥CalcDefence 防御与 EHP。前一层的输出是后一层的输入。

---

## 一、数据来源解析与 ModStore 聚合

这是整个引擎的地基：先把人类可读的 modifier 文本解析成结构化 `Modifier`，再用统一的聚合接口查询。

### 1.1 ModParser：文本 → Modifier

`ModParser.lua` 把一行词条文本拆成三部分：**form（数量形态）** + **modName（作用对象）** + **tag（生效条件/作用域）**。

**form 决定 mod 的 type**。`formList`（`Modules/ModParser.lua:62`）用正则匹配前缀，映射到聚合类型：

```lua
formList = {
  ["^(%d+)%% increased"] = "INC",   -- N% increased → INC
  ["^(%d+)%% faster"]    = "INC",
  ["^(%d+)%% reduced"]   = "RED",   -- RED 在求和时按负 INC 处理
  ["^(%d+)%% more"]      = "MORE",  -- N% more → MORE（独立乘区）
  ["^(%d+)%% less"]      = "LESS",  -- 负 MORE
  ["^([%+%-][%d%.]+)%%? to"] = "BASE", -- +N to ... → BASE（加法基底）
  ["^you gain ([%d%.]+)"]    = "GAIN",
  ...
}
```

- `INC`/`RED` → 加法区（`增益% 相加`）。
- `MORE`/`LESS` → 乘法区（`Π(1+v/100)` 连乘，每个独立乘区）。
- `BASE` → 基底加法（`+N to maximum Life` 这类）。
- `FLAG`/`OVERRIDE`/`LIST` → 布尔标志 / 覆盖值 / 嵌套 mod 列表（由 `modTagList` 与函数式解析产生）。

**modName** 由 `modNameList`（`:157`）把英文短语映射到稳定的内部 mod 名（如 `"fire damage"` → `FireDamage`）。**tag/flag** 由 `modFlagList`（`:964`）、`preFlagList`（`:1174`）、`modTagList`（`:1424`）提供，例如 `with axes or swords` → `ModFlagOr` 武器位掩码，`while on full life` → `Condition` tag，`per X charge` → `Multiplier` tag。

无法识别的文本不报错，而是被收集为「unsupported」——这正是 PoBR `mod_parser.rs` 的 `ParseStatus::Unsupported` 设计来源。

### 1.2 Modifier 的结构

每个 `Modifier` 至少携带：`name`、`type`、`value`、`flags`（位掩码：攻击/法术/武器类型/Hit/Dot 等）、`keywordFlags`、`source`（归因原文）、若干 `tag`（条件、倍率、按属性缩放）。tag 让一个 mod 的「有效数值」在查询时才确定。

### 1.3 ModStore：统一聚合接口

`ModStore.lua` 提供 5 个核心聚合原语，全部经 `Combine`（`:134`）分派，再下沉到 `ModList` 的 `*Internal` 实现（`Classes/ModList.lua`）：

| 接口 | 语义 | 实现 |
|------|------|------|
| `Sum(modType, cfg, ...)` | 同类相加（BASE/INC/RED） | `ModStore:150` → `ModList:SumInternal:97` |
| `More(cfg, ...)` | 独立乘区连乘 | `ModStore:179` → `ModList:MoreInternal:118` |
| `Flag(cfg, ...)` | 任一为真即 true | `ModStore:190` → `ModList:FlagInternal:152` |
| `Override(cfg, ...)` | 返回最后写入的覆盖值 | `ModStore:201` → `ModList:OverrideInternal:173` |
| `List(cfg, ...)` | 收集嵌套 mod 列表 | `ModStore:212` |

**Sum（加法区）** — `ModList:SumInternal`（`:97`）：

```lua
result = 0
for each modName in (...):
  for each mod in self:
    if mod.name == modName and mod.type == modType
       and (flags 匹配) and (keywordFlags 匹配) and (source 匹配):
      result += mod[1] and EvalMod(mod, cfg) or mod.value   -- 带 tag 的走 EvalMod
if self.parent: result += parent:SumInternal(...)           -- parent 链式累加
```

**More（乘法区）** — `ModList:MoreInternal`（`:118`）每个 mod 先按「最接近百分位」算，再连乘：

```lua
result = 1
for each modName in (...):
  modResult = 1
  for each mod (type == "MORE", 同样的 flag/source 匹配):
    modResult *= 1 + (EvalMod(mod,cfg) or mod.value)/100
  -- 默认 round 到 2 位；highPrecisionMods 用更高精度
  result *= modPrecision and floor(result*modResult*10^p)/10^p or round(modResult,2)
if self.parent: result *= parent:MoreInternal(...)
```

> **关键点**：`More` 是「逐 mod 取整后连乘」，而不是先把 value 求和。这与 PoBR `mod_db.rs::more` 的 `Π(1+v/100)` 必须逐项对齐才能 parity。

**标准属性管线**（PoB 通用形态，由上层组合上述原语得到）：

```
final = (base + Σ Sum("BASE")) × (1 + Σ Sum("INC")/100) × Π More
```

### 1.4 EvalMod、条件、倍率、归因

带 tag 的 mod 在聚合时调用 `ModStore:EvalMod`（`:325`）：它解析 `Condition`/`Multiplier`/`PerStat` 等 tag，决定该 mod 是否生效以及最终数值（如 `per X charge` 用当前充能数乘 value）。`GetCondition`（`:268`）、`GetMultiplier`（`:276`）、`GetStat`（`:280`）提供条件/倍率/属性的查询，三者都支持 parent 链回溯。

每个 mod 的 `source` 字段贯穿始终（`SumInternal` 里 `mod.source:match("[^:]+") == source` 做来源过滤），这正是 PoBR 做 source-level 归因 / TraceGraph 的天然挂点。

---

## 二、CalcSetup：环境与来源装配

`CalcSetup.lua` 的职责是把「一个完整的 build」展开成可计算的 `env`：哪些 actor、每个来源的 mod 塞进哪个 modDB、有哪些 active skill、转换表如何构成。

### 2.1 initEnv 与 actor / modDB 分层

`calcs.initEnv(build, mode, override, specEnv)`（`Modules/CalcSetup.lua:497`）是总入口。它构建多个 actor（`player` / `enemy` / `minion`），每个 actor 拥有独立的 modDB；并准备分层的 modDB：

- `env.modDB` — 玩家全局聚合 DB。
- `env.itemModDB` — 物品来源 mod（携带 `In<Slot>` 条件，见 `:1119`、`:1291`）。
- 各 active skill 的 `skillModList` — 技能局部 DB，parent 指向全局，从而链式继承。

`initModDB`（`:19`）写入基础条件：

```lua
modDB.conditions["Buffed"]    = env.mode_buffs
modDB.conditions["Combat"]    = env.mode_combat
modDB.conditions["Effective"] = env.mode_effective   -- 是否计入敌方反制（见 :101-103）
```

### 2.2 来源装配：天赋 / 物品 / 配置 → modDB

各来源经各自路径转成 mod 列表后 `AddList`/`AddMod` 进 modDB：

- **天赋节点**：`buildModListForNode`（`:126`）/ `buildModListForNodeList`（`:280`）把已分配节点的 mod 收集成 ModList，最后 `env.modDB:AddList(calcs.buildModListForNodeList(env, env.allocNodes, true))`（`:1352`）。
- **物品**：逐 slot 解析，写入 `env.itemModDB`，并附带 `<key>In<SlotName>` 这类局部条件（`:1291`），从而支持「仅左手生效」等约束。
- **配置面板**：`env.modDB:AddList(build.configTab.modList)`（`:687`），并把配置 flag 转成 `modDB.conditions[flag] = true`（`:704`）。
- **技能/宝石**：每个 active skill 收集 `skillData`（`:1972`）与自身 mod 进 `skillModList`。

### 2.3 conversionTable 与转换装配

每个 active skill 在装配阶段获得一张 `conversionTable`（在 CalcOffence 早段构建，CalcSetup 准备其输入）。它把 `XDamageConvertToY` / `XDamageGainAsY` 解析成「从源类型到目标类型的乘数矩阵」，外加一个 `mult` 字段表示「源类型保留比例」。`PhysicalDamageGainAsRandom` 这类随机转换在 offence 早段被展开成对应的具体类型 mod（见 `CalcOffence.lua:1175` 起）。转换表是 §4 伤害核心的直接输入。

---

## 三、CalcPerform：全局编排（buff / 充能 / 条件）

`calcs.perform(env, skipEHP)`（`Modules/CalcPerform.lua:955`）在 offence/defence 之前跑一遍全局编排，把「跨技能、跨 actor 的状态」一次性解算出来，写进 modDB 和 output，供后续按 skill 计算时复用。

### 3.1 Keystone 与属性 / 条件

- `mergeKeystones`（`:66`）合并 keystone 提供的 mod 到玩家 DB。
- `doActorAttribsConditions`（`:137`）解算力量/敏捷/智力及由属性派生的条件。
- `doActorMisc`（`:503`）处理杂项 actor 状态。

### 3.2 充能（Charges）

`doActorCharges`（`:766`）是充能解算中心。它对每类充能求 Min/Max/Duration，并处理「最大充能互相绑定」的 keystone：

```lua
output.PowerChargesMax = Override("PowerChargesMax") or max(Sum("BASE","PowerChargesMax"), 0)   -- :772
-- 若 "MaximumFrenzyChargesIsMaximumPowerCharges"：把 FrenzyChargesMax 覆盖为 PowerChargesMax (:774)
output.FrenzyChargesMax    = Override("FrenzyChargesMax") or max(... , 0)                          -- :781
output.EnduranceChargesMax = ...                                                                  -- :790
```

PoE2 新增了多种特殊充能（Blood/Spirit/Brutal/Absorption/Affliction 等，`:797-804`），它们多数由「等于某类充能最小/最大值」的 flag 派生。解算出的充能数随后被 §1.4 的 `per X charge` Multiplier tag 在聚合时引用。

### 3.3 Buff / Aura 合并

`mergeBuff`（`:41`）是 buff 叠加去重的核心。它把同一 buff 的多个来源合并进目标 ModList，**对非 LIST 类型 mod 取较大值（不叠加同名 mod）**：

```lua
for mod in src:
  if mod.type ~= "LIST":
    在 dest 中找 compareModParams 相同的 destMod:
      if mod.value > destMod.value: 替换为 mod   -- 同名取大，避免重复叠
      match = true
  if not match: 插入 mod
```

这保证了「多个来源给同一 buff」时按最强者生效，而不是错误地相加。aura 自身 buff、Mark/Buff 的进攻型 gain-as-extra 也在这一阶段进入玩家 DB（对应 PoBR 近期 commit 的摄取逻辑）。

### 3.4 Reservation 与其它

CalcPerform 还解算预留（生命/魔力/spirit reservation）、恢复、条件最终态，并在 `skipEHP` 为否时触发防御侧的 EHP 预备。编排完成后，env 进入按 active skill 的进攻/防御计算阶段。

---

## 四、CalcOffence 伤害核心（转换 · gain-as-extra · inc/more）

`calcs.offence(env, actor, activeSkill)`（`Modules/CalcOffence.lua:377`）是单个 active skill 的伤害解算主函数。其核心三步：**算各类型基础伤害桶 → 应用转换/gain → 应用 inc/more 得最终伤害**。

### 4.1 伤害类型桶与转换矩阵

引擎对 6 种伤害类型（Physical/Fire/Cold/Lightning/Chaos，外加可能的 typeless）各维护一个 `<Type>MinBase`/`<Type>MaxBase` 桶。`activeSkill.conversionTable[src][dst]` 是转换乘数矩阵，`conversionTable[type].mult` 是该类型「转换后剩余」的比例。

**转换** — `calcConvertedDamage`（`:68`）：对目标类型 `damageType`，把所有源类型按矩阵乘数加进来：

```lua
convertedMin/Max = Σ over otherType:
    conversionTable[otherType][damageType] > 0 and
    output[otherType.."MinBase"/"MaxBase"] * convMult
```

**gain-as-extra** — `calcGainedDamage`（`:90` 起）：与转换不同，gain 是「额外获得」，不消耗源伤害。它取「源的 base + 该源转换出的部分」再乘 `gainTable[src][dst]`：

```lua
for otherType:
  baseMin = floor(output[otherType.."MinBase"] * conversionTable[otherType].mult)  -- 转换后保留的源
  if gainTable[otherType][damageType] > 0:
    convMin, convMax = calcConvertedDamage(..., otherType)
    gainedMin += (baseMin + convMin) * gainMult
```

> **PoE2 差异（重要）**：PoBR 近期修正确认——PoE2 仅按**最终伤害类型**聚合 increased，转换源不再做 increased double-dip（移除了 PoB-PoE1 沿用的转换源 increased 双蘸）。转换分量的 inc/more 用 `comp.damage_type` 而非源类型聚合。本文档据此标注，CalcOffence 的 `calcDamage` 也是按 `damageType`（最终类型）取 modNames。

### 4.2 calcDamage：inc / more 应用

`calcDamage(activeSkill, output, cfg, breakdown, damageType, typeFlags, convDst)`（`:110`）对单一最终类型应用加法与乘法区：

```lua
summedMin = output[damageType.."SummedMinBase"]    -- 已含 base+converted+gained
summedMax = output[damageType.."SummedMaxBase"]
if summedMin == 0 and summedMax == 0: return addMin, addMax   -- 无基底直接返回 (:121)

modNames = damageStatsForTypes[typeFlags]          -- 按 typeFlags 选出适用的 mod 名集合
inc  = 1 + skillModList:Sum("INC", cfg, unpack(modNames)) / 100
more = skillModList:More(cfg, unpack(modNames))
moreMinDamage = skillModList:More(cfg, "Min"..damageType.."Damage")  -- 仅作用最小值的 more
moreMaxDamage = skillModList:More(cfg, "Max"..damageType.."Damage")

return round(summedMin * inc * more * moreMinDamage + addMin),
       round(summedMax * inc * more * moreMaxDamage + addMax)
```

要点：

- `typeFlags` 通过 `bor(typeFlags, dmgTypeFlags.flags[damageType])`（`:113`）累积，使 `Fire`、`Elemental`、`Spell` 等多层标签都能命中对应 mod。
- `inc` 把所有 `INC` 求和后一次性 `(1+Σ/100)`；`more` 走 §1.3 的逐项连乘。
- `Min<Type>Damage`/`Max<Type>Damage` 是只偏置某一端的额外 more 乘区。

最终每个类型得到一对 `(min, max)`，加总为该 hit 的总伤害区间，进入 §5 的命中/暴击组装。

---

## 五、命中 / 暴击 / 异常 / DPS 组装

得到每类型 min/max 后，CalcOffence 继续把它折算成「期望每次命中伤害」，再乘命中率、攻速、暴击、异常，组装出 DPS。

### 5.1 命中率（Accuracy）

命中率公式在 `calcs.hitChance`（`Modules/CalcDefence.lua:32`，被 offence 复用）：

```lua
function calcs.hitChance(evasion, accuracy, uncapped)
  if accuracy < 0 then return 5 end
  rawChance = (accuracy * 1.25) / (accuracy + evasion * 0.3) * 100
  return uncapped and max(round(rawChance), 5)
                   or max(min(round(rawChance), 100), 5)   -- 命中率 cap 在 [5,100]
end
```

offence 侧（`CalcOffence.lua:2612` 起）取敌方闪避 `enemyEvasion`，算 `output.AccuracyHitChance`；`HitChanceCanExceed100` flag 下保留 `AccuracyHitChanceUncapped` 用于超额命中转化（`:2619`）。最终：

```lua
output.HitChance = output.AccuracyHitChance * (1 - output.enemyBlockChance / 100)   -- :2671
```

> **PoE2 差异**：命中公式系数为 `1.25 / (acc + 0.3*eva)`，与 PoB-PoE1 的形态不同；敌方对玩家的命中走 `monsterHitChance`（`CalcDefence.lua:40`），公式不对称。

### 5.2 暴击（Crit）

暴击链在 `CalcOffence.lua:3685` 一带：

```lua
base = Sum("BASE","CritChance") (+敌方 SelfCritChance)        -- :3685
inc  = Sum("INC","CritChance")  (+敌方)
more = More("CritChance")
output.CritChance = (baseCrit + base) * (1 + inc/100) * more  -- 标准 (base)*(1+inc)*more
output.CritChance = min(CritChance, Override("CritChanceCap") or Sum("BASE","CritChanceCap"))  -- 暴击率 cap
if mode_effective: CritChance *= AccuracyHitChance / 100      -- 命中折减暴击 (:3700)
if CritChanceLucky: CritChance = (1 - (1 - p)^2) * 100         -- lucky 取两次较优 (:3705)
```

暴击倍率 `output.CritMultiplier` 由 `Sum("BASE","CritMultiplier")` 等组合得出；`InevitableCriticalHits`、`Bifurcate`、Maw of Mischief 之类的分级暴伤（100%/70%/40%/10% 的链式概率，`:3719-3724`）也在此处理。最终期望命中伤害约为：

```
avgHit ≈ damage_avg * (1 + critChance/100 * (critMultiplier/100 - 1)) * DamageEffectiveness
```

### 5.3 异常（Ailment）

伤害型异常（Bleed/Poison/Ignite）与非伤害型（Shock/Freeze/Scorch...）在 `:4904` 的 `calcAilmentDamage(ailment, sourceCritChance, sourceHitDmg, sourceCritDmg)` 统一计算 base magnitude：

```lua
baseVal = baseFromHit + baseFromCrit     -- :4914，按命中/暴击两部分加权
-- 每种 ailment 有 ailmentData[ailment]: { duration, max, min, precision, associatedType }
output[ailment.."Duration"] = ailmentData[ailment].duration * (1+incDur/100) * moreDur * debuffDurationMult  -- :5522
```

- Bleed/Poison 走 `:5147-5149` 的 min/avg/max 三档解算（`ailmentPercentBase` 为基准系数）。
- Shock/Freeze 等用 `calcAilmentDamage(...) * More("FreezeAsThoughDealing")`（`:5501`/`:5517`）把「视作造成多少伤害」折成幅度，再映射到等级/时长。
- 哪些伤害类型能触发哪种 ailment，由 `type.."Can"..damagingAilment` flag 控制（`:5453`，如 Avatar of Fire + Blistering Bond 让火伤可流血）。

### 5.4 攻击/施法速度与 DPS 总装

速度解算在 `:2697` 起：`output.Speed = 1 / output.Time`；channelling、totem、trigger、warcry、连射（FiringRate/Reload，`:2884`）各有分支，并受 `ActionSpeedMod` 独立乘区与服务器帧率 cap（`min(Speed, ServerTickRate * Repeats)`，`:2864`）约束。

最终 DPS 组装（概念式）：

```
TotalDPS = avgHit * HitChance/100 * Speed * skillData.dpsMultiplier
         + Σ ailment/dot DPS
output.CombinedDPS = TotalDPS + 各 DoT/二次效果
```

`output.CombinedDPS` 初始化于 `:400`，逐项累加 hit DPS、ailment DPS、burning/caustic ground 等，得到面板总 DPS。

---

## 六、CalcDefence 防御与 EHP

`CalcDefence.lua` 负责生命/魔力/护盾资源、护甲/闪避减伤、抗性、以及最终 EHP 与「按伤害消耗资源池」的模拟。

### 6.1 资源池：Life / Mana / Spirit / ES

`doActorLifeManaSpirit`（`:73`）解算各资源最大值。注意 PoE2 的「资源转护」机制（`:91`）：

```lua
conv = min(Sum("BASE", resName.."ConvertToEnergyShield" / "...ConvertToArmour" / "...ConvertToEvasion"), 100)
```

即一部分最大生命/魔力可被转成 ES/护甲/闪避，转换比 cap 在 100%。

### 6.2 护甲减伤

护甲减伤是非线性的，核心 `calcs.armourReductionF`（`:56`）：

```lua
function calcs.armourReductionF(armour, raw)
  if armour == 0 and raw == 0 then return 0 end
  if armour < 0 then armour = -armour ...  -- 护甲被击破到负值时仍按公式但取负减伤
  return armour / (armour + raw * data.misc.ArmourRatio) * 100
end
```

其中 `ArmourRatio = 10`（`Modules/Data.lua:193`）。

> **PoE2 vs PoE1 关键差异**：PoE2 护甲系数为 **`armour / (armour + 10*raw)`**（`ArmourRatio=10`），而 PoB-PoE1 公式为 `armour/(armour+5*raw)`（系数 5）。这是 PoBR `agent-docs/armour.md` 与 defence 实现必须区分的一点；同样的 raw hit 下 PoE2 护甲减伤约为 PoE1 的一半。

减伤还受 `<Type>DamageReductionMax` 上限 cap（`:394`），并与 flat damage reduction、敌方 overwhelm 叠加（`:429-431`）。

### 6.3 闪避与命中规避

玩家闪避通过敌方命中率体现：敌方对玩家命中走 `calcs.monsterHitChance`（`:40`）：

```lua
rawChance = (1 - (0.95 * evasion) / (evasion + 4 * accuracy)) * 100   -- 越多闪避越低命中
```

`deflectChance`（`:48`）处理 PoE2 新增的「偏转」（deflection），cap 由 `DeflectionChanceCap = 95`（`Data.lua:183`）约束。

### 6.4 抗性

抗性解算（`:817` 起）对每种元素：`base = Sum("BASE", elem.."Resist", ...)`，`inc = max(calcLib.mod(...), 0)`，最大抗性 `<elem>ResistMax = Override or min(MaxResistCap, Sum("BASE", elem.."ResistMax", "ElementalResistMax"))`，其中 `MaxResistCap = 90`（`Data.lua:181`）。引擎额外解算 `ResistTotal`（实际生效）、`ResistOverCap`（溢出，可被某些机制利用）、`Totem<elem>Resist`（图腾独立抗性）。`ElementalResistMaxIsHighestResistMax`（`:867`，Melding of the Flesh）这类 keystone 把所有元素最大抗性统一为最高者。

### 6.5 减伤合流与 takenHit

`applyDmgTakenConversion`（`:356`）与 `takenHitFromDamage`（`:422`）把一次原始伤害穿过完整防御链：抗性 → 护甲/ES/闪避折算的「有效护甲」减伤 → flat reduction → overwhelm → taken multiplier。有效护甲来自多源（`:388-391`）：

```lua
effArmour = Armour*pArmour/100*(1+ArmourDefense) + Evasion*pEva/100 + EnergyShield*pES/100
armourReduct = min(<type>DamageReductionMax, armourReductionF(effArmour, damage))
reductMult = (1 - max(min(<max>, armourReduct + reduction), 0)/100) * damageTakenMods   -- :396
```

### 6.6 EHP 与资源池模拟

EHP 通过 `reducePoolsByDamage`（`:461`）反向求解：给定各类型来袭伤害比例，模拟依次消耗 ES（含 bypass，`:583`）、Life、以及盟友 ES（Soul Link，`:484`）等资源池，求能承受的最大 hit / 总有效血量。护甲的非线性使「最大可承受单次伤害」需要解二次方程（`:3645` 一带的 `a*x² + b*x` 形式），这也是 EHP 计算复杂度的来源。

---

## 七、PoE2 vs PoE1 关键差异速查表

下表从各章提炼 PoBR 在 parity 时最易踩坑、与 PoB-PoE1 公式不同之处：

| 机制 | PoE1（旧 PoB 沿用） | PoE2（本引擎实现） | 源码 |
|------|------|------|------|
| 护甲减伤系数 | `armour/(armour+5*raw)`（Ratio=5） | `armour/(armour+10*raw)`（**Ratio=10**） | `Data.lua:193`、`CalcDefence.lua:56` |
| 命中率公式 | PoE1 形态 | `1.25*acc / (acc + 0.3*eva) * 100`，cap [5,100] | `CalcDefence.lua:32` |
| 敌方命中玩家 | — | `1 - 0.95*eva/(eva+4*acc)`（不对称） | `CalcDefence.lua:40` |
| 转换源 increased | 转换源 increased 双蘸（double-dip） | **仅按最终伤害类型聚合 increased**，无 double-dip | `CalcOffence.lua:110`（按 `damageType` 取 modNames） |
| 偏转 Deflection | 无 | 新增 `deflectChance`，cap 95 | `CalcDefence.lua:48`、`Data.lua:183` |
| 资源转护 | 有限 | Life/Mana 可 `ConvertToEnergyShield/Armour/Evasion`，cap 100% | `CalcDefence.lua:91` |
| 充能种类 | Power/Frenzy/Endurance 为主 | 新增 Blood/Spirit/Brutal/Absorption/Affliction 等派生充能 | `CalcPerform.lua:797-804` |
| 最大抗性 cap | 90 | 90（`MaxResistCap`），但叠 keystone 路径不同 | `Data.lua:181`、`CalcDefence.lua:867` |
| Spirit 资源 | 无 | 新增 spirit 预留资源池 | `CalcDefence.lua:73`、`CalcPerform` reservation |

> **聚合层不变量**（PoE1/PoE2 共用，PoBR 必须逐项对齐）：`More` 是逐 mod 取整后连乘（非先求和），`source` 前缀匹配贯穿所有聚合接口，`EvalMod` 在查询时才解 tag——这三点是 ModStore parity 的硬约束（`ModList.lua:97-200`）。

---

### 附：PoBR 实现映射速查

| PoB2 源码 | PoBR 对应模块 |
|-----------|--------------|
| `ModParser.lua` | `pobr-core/src/mod_parser.rs` |
| `ModStore.lua` / `ModList.lua` | `pobr-core/src/mod_db.rs`（`sum`/`more`/`flag`/`override_`/`list`/`*_traced`） |
| `CalcSetup.lua` | `pobr-core/src/calc/session.rs` + `item.rs`/`passive.rs`/`skill_source.rs` ingest |
| `CalcPerform.lua` | `pobr-core/src/calc/perform.rs` |
| `CalcOffence.lua` | `pobr-core/src/calc/offence.rs` + `damage.rs`/`ailment.rs`/`skill_use_time.rs` |
| `CalcDefence.lua` | `pobr-core/src/calc/defence.rs` + `stat_boundary.rs`/`ehp.rs`/`survivability.rs` |

source-level 归因（TraceGraph / AttributionReport）是 PoBR 相对 PoB2 的核心增量：PoB2 的 `mod.source` 仅做来源过滤，PoBR 借同一字段构建 DAG，回溯每个输出到 `SourceId`。
