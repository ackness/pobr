# 物品与角色系统 (Item & Character Systems)

本文档梳理 PoE2（0.5.0）的**物品/角色"系统功能面"**机制：稀有度与数量、物品等级、需求、插槽 / 符文 / 灵魂核心、品质、角色等级与点数来源、珠宝、瓶子 / 护符。

> 与既有文档**互补、不重复**：
> - 前后缀 / 修饰词等级 / 制作流程见 [crafting.md](./crafting.md)；通货球体见 [currency.md](./currency.md)；
> - 三大属性派生（力量→生命、敏捷→精准、智力→魔力）见 [attributes.md](./attributes.md)；
> - 宝石标签 / 类型 / Spirit / 转化宝石见 [gems.md](./gems.md) 与 [meta-gems.md](./meta-gems.md)；
> - 充能（Charges）获取 / 上限 / 持续、偷取 / 再生、瓶子充能恢复见 [recovery-charges-buffs.md](./recovery-charges-buffs.md)；
> - 战役永久奖励（属性点、抗性奖励、Boon 重选）见 [campaign-rewards.md](./campaign-rewards.md)。
>
> 本文只补它们没覆盖的**系统功能面**，并交叉引用。末尾「PoB2 数据核对」给出真实变量 / 常量名，是 pobr 的回归基准。

---

## 一、物品稀有度与数量 (Item Rarity / Quantity, Magic Find)

### 1.1 物品稀有度（与 crafting.md 交叉引用，不重复显性词条上限）

四档稀有度：普通 (Normal, 灰) / 魔法 (Magic, 蓝) / 稀有 (Rare, 黄) / 传奇 (Unique, 棕)[^poe2wiki-rare]。显性词条上限见 [crafting.md](./crafting.md)（魔法 2 / 稀有 6 / 传奇不可改）。**通货、瓶子、护符、符文等没有这四档稀有度**，只有内部掉落权重。

### 1.2 增加物品稀有度 / 数量 (IIR / IIQ = Magic Find)

- **IIR (Increased Item Rarity / Rarity of Items found)**：同时影响①掉落物的稀有度，②"未鉴定层级 (unidentified tier)"系统下鉴定时能滚到的词条好坏[^poe2wiki-rare]。
- **IIQ (Increased Item Quantity)**：增加掉落物**数量**。
- **MF (Magic Find)** = IIR + IIQ + 组队加成的统称。

> **关键计算特性（pobr 不直接建模，但需理解作用范围）**：
> - **不同来源的 IIR 相乘**：装备 vs 区域修饰词 vs 怪物 IIR 倍率三者**乘法叠加**，且部分来源有边际递减。
> - **作用于"可有稀有度的物品"**——主要影响装备掉落与未鉴定层级；对通货 / 黄金的影响因机制而异。0.5.0 起高 IIR 反而让黄金 / 通货 / 品质通货 / 瓶子 / 护符 / 符文掉落**变少**（鼓励刷稀有通货）[^poe2wiki-rare]。
> - **10 档稀有度层级系统**：掉落时抽一档，IIR 提高抽到高档的概率，效果是"偏态分布"而非线性。

### 1.3 PoE2 0.5.0 现状与变化

- IIR 词条上限**下调**：前缀 / 后缀最高档从 32%/30% 降到 **25%/25%**[^poe2wiki-rare]。
- 怪物词条提供的 IIR / IIQ **提高**（每条词条稀有度加成翻倍、+10% 数量 / 条）[^poe2wiki-rare]。
- 引入 **Monster Rarity** 新统计（影响魔法 / 稀有怪出现率与稀有怪额外词条数）；Waystone 词条改为提供 Monster Effectiveness / Pack Size / Item Rarity / Monster Rarity / Waystone Drop Chance，且与地图集天赋树**乘法叠加**[^poe2wiki-050]。
- 理论上 2×`Ventor's Gamble` + `Ingenuity` 可堆到 −100% IIR（0% 稀有度），但似乎仍会掉魔法物品；不影响隐藏的保证掉落[^poe2wiki-rare]。

---

## 二、物品等级 (Item Level)

物品等级 (item level, iLvl) 由**发现该物品的区域等级**决定，与"装备需求等级 (required level)"是两个独立概念[^orbis-ilvl]。

- iLvl 决定①该物品能出现哪些词条、②词条能达到的**最高等级 (tier)**——前后缀 / 等级范围细节见 [crafting.md](./crafting.md)，此处不重复。
- 低 iLvl 物品**无法"幸运地"**滚出高等级词条；这是确定性约束，不是概率[^orbis-ilvl]。
- 同一词条，不同 iLvl 物品能滚到的最高 tier 不同——故低 iLvl 物品有时反而更易滚到想要的中 tier（候选 tier 池更小）[^alphagamer-tiers]。

### 重要 iLvl 断点（0.5.0，gearing 参考）[^crafting-breakpoints][^mobalytics-gearing]

| 断点 | iLvl | 解锁 |
|------|------|------|
| Tier 1 装备区 | 65 | 30% 移速靴、+2 技能类前缀起步 |
| 元素抗性 +41~45% | 82 | 最高单档抗性 |
| 混沌抗性最高（胸甲） | 81 | — |
| 最高移速 | 82 | 第二高移速需 70 |
| +3 to skills / 多数最高 flat 伤害 | ~75 | flat 元素 / 物理 |

> **未鉴定层级 (Tier 1-5) 系统**：掉落的稀有 / 魔法物品可带 "(Tier N)" 标记（5 最好），鉴定后标记消失但效果保留——它**剔除低于某 iLvl 的词条**，等效提高高 tier 命中率[^orbis-ilvl][^alphagamer-tiers]。

---

## 三、需求 (Requirements)

PoE2 装备 / 宝石可有**属性需求**（力量 / 敏捷 / 智力）和**等级需求**。

- **属性需求**：由底材 + 词条决定，需角色对应属性 ≥ 需求才能装备 / 使用。宝石需求在 PoB 数据中以 `reqStr/reqDex/reqInt`（0~100 的比例值，再按宝石等级缩放）表示[^pob-gems]。
- **等级需求**：角色等级 ≥ 物品 / 宝石需求等级。瓶子 / 护符也有等级需求（如 `Thawing Charm` req level 12）[^pob-flask]。
- **未满足需求的后果**：**物品无法装备 / 宝石无法使用**（PoE2 不像 PoE1 老版本带数值惩罚，而是直接禁用）。
- **需求转换**：`Giant's Blood` 等 Keystone "单手持双手武器，武器需求翻倍"；灵魂核心 `Soul Core of Atmohua/Cholotl/Zantipi` 提供 "Convert 20% of requirements to Str/Dex/Int" 把需求在属性间转移[^game8-sockets][^thegamer-runes]。

---

## 四、插槽系统：符文 / 灵魂核心 / 增强物 (Sockets / Runes / Soul Cores / Augments)

> **与 PoE1 的根本差异**：PoE2 装备**没有连线 (links) / 颜色插槽**。技能宝石独立存在、辅助宝石直接挂在技能上（见 [gems.md](./gems.md)），靠 **Spirit** 限制持续效果数量。装备插槽专门用来塞**增强物 (Augments)**——符文 / 灵魂核心 / 护身符 (Talismans) / Idols[^mobalytics-runes][^game8-sockets]。

### 4.1 插槽数量上限

| 部位 | 默认最大插槽 |
|------|------------|
| 胸甲、双手近战武器 | 2 |
| 单手近战武器、手套、头盔、靴子 | 1 |
| 法杖 / 短杖（caster weapons）| 0.5.0 起也可有符文插槽 |
| 箭袋、护身物 (Foci)、首饰（戒指 / 项链 / 腰带）| 0（不可插）|

- 全身最多约 **7 个常规插槽**（武器 2 + 胸甲 2 + 手套 / 靴子 / 头盔各 1）[^game8-sockets]。
- **加插槽**：`Artificer's Orb`（10 个 Artificer's Shard 合成）给物品**加一个插槽**；`Vaal Orb` 腐化可**突破** 1/2 上限多加一个（有损坏风险）[^mobalytics-runes][^thegamer-runes]。

### 4.2 增强物的核心特性：效果随插入部位变化

符文 / 灵魂核心的效果**取决于插入的是武器、护甲还是头盔等**——这是 pobr 建模的关键。PoB-PoE2 `Data/ModRunes.lua` 按 `weapon` / `armour` / `body armour` / `helmet` / `boots` 等键分别给出修饰词[^pob-runes]：

```lua
["Soul Core of Tacati"] = {
  ["weapon"] = { type="SoulCore", "15% chance to Poison on Hit with this weapon", ... },  -- 局部
  ["armour"] = { type="SoulCore", "+11% to Chaos Resistance", ... },                       -- 全局
}
```

- **符文 (Runes)**：到处可掉，效果简单（如 Glacial Rune：武器 +6~10 冰伤 / 护甲 +12% 冰抗）。有 Lesser / 普通 / Greater 三档。0.5.0 起符文获得**法杖 / 短杖专属词条**（Desert/Glacial/Storm/Iron/Body/Mind/Rebirth/Inspiration/Stone/Vision 等）[^game8-sockets][^poe2wiki-augments]。
- **灵魂核心 (Soul Cores)**：仅 **Trials of Chaos** 奖励，效果更强 / 更独特（如 `Soul Core of Ticaba`：武器 +12% 爆伤 / 护甲"对你的击中减 10% 爆伤"，见 [critical-hits.md](./critical-hits.md)）。可有 Spirit / IIR 等高级词条。
- **作为附魔显示**：增强物效果以**附魔 (enchantment)** 形式出现在物品上（见 [crafting.md](./crafting.md) 附魔段）[^poe2wiki-augments]。

### 4.3 局部 vs 全局（与 crafting.md 局部 / 全局段呼应）

灵魂核心在**武器**里的词条通常是**局部 (local)**——只作用于该武器的击中（如 "with this weapon"、武器局部爆伤）；在**护甲**里通常是**全局**（抗性、最大生命等）。这与 [crafting.md](./crafting.md) 的 local/global 区分一致，pobr 需保留这层语义。

### 4.4 0.5.0 变化

- **"Rune Sockets" 改名 "Augment Sockets"**；可插物从 "Socketables" 改名 "**Augments**"[^poe2wiki-augments]。
- **可覆盖**：符文 / 灵魂核心插入后不可取回，但可用另一个 Augment **覆盖**（旧的销毁）[^poe2wiki-augments][^mobalytics-runes]。
- **Ancient Augments**（如 Atziri's Temple 的 Abyssal eyes、特殊灵魂核心）：**全角色共享上限 1 个**[^poe2wiki-augments]。

---

## 五、品质 (Quality)

品质对不同物品类型给**小幅加成**，效果与上限各异[^poe2wiki-quality]。

### 5.1 各类型品质效果（每 1% 品质）

| 物品类型 | 品质通货 | 每 1% 品质效果 | 作用方式 |
|---------|---------|---------------|---------|
| 近战武器 (Martial) | Blacksmith's Whetstone | **1% more 局部物理伤害** | 只放大**底材物理**，不放大附加 / 转化的非物理伤害 |
| 法杖 / 短杖 / 权杖 | Arcanist's Etcher | **+1% 内置技能品质** | 武器本身不吃品质，品质转给**内置技能** |
| 护甲（装备）| Armourer's Scrap | **1% more 局部防御** | 放大底材 armour/evasion/ES |
| 戒指 / 项链 | Catalyst（催化剂）| **1% increased 修饰词强度** | 按催化剂标签选定词条 |
| 瓶子 (Flask) | Glassblower's Bauble | **1% more 局部生命 / 魔力恢复** | — |
| 护符 (Charm) | （腐化等）| **1% increased 局部持续时间** | — |
| 技能宝石 | Gemcutter's Prism | 因技能而异 | 见 §六 |

> 关键："more 局部物理伤害 / 防御"作用于**底材数值**，在局部 increased 之前并入，是 pobr 武器 / 护甲底材缩放管线的一环。

### 5.2 品质上限

- **默认上限 20%**；任何物品**硬上限 30%**（"Maximum Quality cannot be raised above 30%"）[^poe2wiki-quality]。
- **例外**：`Breach Ring` 隐式把上限提到 **40%**[^poe2wiki-quality]。
- 高级掉落：iLvl ≥ 78 的普通 / 传奇装备可罕见地直接掉 >20%（最高 30%，带 "Exceptional" 前缀）。
- 品质通货**仍只能加到 20%**；超过 20% 需 `Vaal Infuser`（武器 / 护甲，capping 30%，有腐化风险）或腐化（瓶子 / 护符 / 宝石 ±10%，可达 23%）[^poe2wiki-quality]。
- **不能有品质**：箭袋、珠宝、腰带、辅助宝石[^poe2wiki-quality]。

---

## 六、宝石等级与品质曲线（与 gems.md 交叉引用补功能面）

宝石标签 / 类型 / Spirit / 转化宝石见 [gems.md](./gems.md)。此处补**等级 / 品质功能面**：

- **等级上限**：技能宝石自然最高 **level 20**（`naturalMaxLevel = 20`），腐化 +1 → **21**[^pob-gems][^poe2wiki-gem]。用 `Uncut Skill Gem` 雕刻 / 升级，雕出来的等级 = 未切割宝石的等级。
- **辅助宝石没有品质**（不能加）[^poe2wiki-quality]。
- **技能宝石品质效果因技能而异**（不是统一"+伤害"）；`Gemcutter's Prism` 每次固定 +5%[^poe2wiki-quality]。技能宝石掉落时**无品质**（未切割宝石无品质）。
- **0.5.0 额外品质**：所有宝石现在都有按住 **Alt 查看的额外品质属性**；`Gemling Legionnaire` 的 `Advanced Thaumaturgy` 改为"宝石品质赋予插槽技能额外效果"[^maxroll-050]（见 [gems.md](./gems.md) 0.5.0 段）。
- **全局宝石品质**（如 Body Armour "+2% Quality of all Skills"）**独立于 20% 默认上限**，>20% 部分仍继续叠加[^poe2wiki-quality]。
- **升华技能**：Ascendancy 授予的技能等级与辅助插槽**随角色等级自动缩放**[^poe2wiki-gem]。

---

## 七、角色系统：等级 / 点数 / 重置 (Character Progression)

### 7.1 等级与经验

- 每升 1 级获得 **1 个被动天赋点**；部分任务额外给点[^mobalytics-tree]。
- 角色等级派生生命 / 魔力 / 精准（见 [attributes.md](./attributes.md)：基础生命 28 + 12/级 + 2/Str；魔力 34 + 4/级 + 2/Int；精准 6/级 + 6/Dex）。1 级基础闪避 7、基础速度 37[^pob-misc]。
- 终局起始区域等级 **65**（`EndgameStartLevel = 65`）[^pob-misc]。

### 7.2 被动天赋点来源

| 来源 | 说明 |
|------|------|
| 升级 | 1 点 / 级 |
| 任务奖励 | 战役中若干任务给点 |
| **技能书 (Skill Books)** | 含可选 / 隐藏目标，全部约凑到 **123 总点数**[^game8-tree] |

被动节点类型：小节点 (small) / 显著 (notable) / Keystone（强力 + 代价）/ 属性 / 旅行节点（+5 任意属性，可重选）/ 珠宝插槽 (jewel socket)[^mobalytics-tree]。

### 7.3 武器组天赋点 (Weapon Set Passive Skill Points)

PoE2 角色默认有 **2 个武器组 (weapon sets)**（`base_number_of_weapon_sets = 2`）[^pob-misc]，可分别配不同武器 / Spirit，并有**独立的武器组天赋点**[^poe2wiki-weaponset]：

- 战役通过 `Book of Specialisation` 等给点，全程共 **24 点**，每组最多 24。
- **不额外增加总点数**——它把常规点数**转化**为武器组点（要分配武器组点，需有等量未分配的常规点）。
- Keystone 和珠宝插槽**不能**分到不同武器组。
- `Witchhunter` 的 `Weapon Master` 额外转化 100 常规点为武器组点。

### 7.4 觉醒点 (Ascendancy Points)

- 共 **8 点**，分 **4 组 × 2 点**，通过 Trial of the Sekhemas（4 层）或 Trial of Chaos（按 7/10/secret 门槛）获得[^poe2db-asc][^poe2wiki-trial]。
- 同一类 Trial 可重复刷满 8 点。
- 改升华需把觉醒点全退掉再到 Ascendancy Altar 重选；`AscendancyRespecCost = 5`[^pob-misc]。

### 7.5 重置 (Respec) 与属性点

- **随时可重置**被动，在 The Hooded One / Doryani 处花**黄金**；按已分配点数累进定价，PoB 用 `data.goldRespecPrices` 表（1→15、2→19、…、100→10129 金）[^pob-misc][^mobalytics-tree]。
- **属性点**：PoE2 **没有**独立分配的属性点，属性靠**被动树节点 + 装备 + 珠宝 + 升华 + 战役奖励**获得（见 [attributes.md](./attributes.md) §属性获取）。旅行节点的 +5 属性类型可随时重选。
- 地图集天赋树重置 `AtlasPassiveRespecCost = 5000`[^pob-misc]。

---

## 八、珠宝 (Jewels)

### 8.1 普通珠宝

放进**被动树的珠宝插槽**（jewel socket），提供词条加成。底材按属性区分（`Ruby` = str / `Emerald` = dex / `Sapphire` = int / `Diamond` = 三系），`type = "Jewel"`、`req = {}`（**无等级 / 属性需求**）[^pob-jewel]。珠宝**不能有品质**[^poe2wiki-quality]。0.5.0 新增 6 个 Sinister Jewel Socket 位置 + 用 Liquid Emotions / Catalysts 给珠宝制作额外词条[^poe2-tree-report][^poe2wiki-050]。

### 8.2 半径珠宝 / 时光珠宝 (Time-Lost / Timeless / Timelost)

PoB 数据里 `subType = "Radius"`（Time-Lost Ruby/Emerald/Sapphire/Diamond）与 `subType = "Timeless"`（Timeless Jewel）[^pob-jewel]：

- **Timeless Jewel**：放进**已分配**的珠宝插槽，**半径内被动节点被改造**（DropLevel 20，`AlwaysAllocate`，可腐化）[^poe2db-timeless]。半径来自 `PassiveTreeJewelDistanceMultiplier = 1.2`[^pob-misc]。
- **Time-Lost / Timelost Jewel**：0.5.0 用 **Ancient Emotions**（地图集树解锁）制作的半径珠宝，可镶 16 个不在普通树上的 Notable[^poe2wiki-050]。

### 8.3 关于"星团珠宝 (Cluster Jewels)"——重要校验

> **PoE2 实际游戏中没有 PoE1 式的星团珠宝**（套外圈插槽、自带子插槽生成天赋）。PoB-PoE2 仓库里仍有 `Data/ClusterJewels.lua`（Small/Medium/Large + minNodes/maxNodes/notableIndicies/socketIndicies），但那是**从 PoE1 继承的遗留数据 / 占位**，不代表 0.5.0 游戏现状[^pob-cluster]。pobr 实现珠宝域时**应以 Timeless / Time-Lost / 普通珠宝为准**，把 cluster 数据视为遗留、暂不实现，除非官方在后续版本引入。

---

## 九、瓶子与护符 (Flasks / Charms)

> 充能获取 / 上限 / 持续、偷取 / 再生见 [recovery-charges-buffs.md](./recovery-charges-buffs.md)；此处只补**瓶子 / 护符系统功能面与底材数值**。

### 9.1 瓶子 (Flasks)

PoE2 瓶子**只有生命 / 魔力瓶**（不像 PoE1 有大量功能 utility 瓶——utility 移到护符）。底材带 `flask = { life, duration, chargesUsed, chargesMax }`[^pob-flask]：

| 底材 | 恢复 (life) | 持续 | 单次消耗 | 充能上限 |
|------|-----------|------|---------|---------|
| Lesser Life Flask | 50 | 3s | 10 | 60 |
| Medium Life Flask | 90 | 5s | 10 | 65 |
| Greater Life Flask | 150 | 4s | 10 | 70 |
| Grand Life Flask | 260 | 5s | 10 | 75 |
| Giant / Colossal | 340+ | — | 10 | 75 |

- 瓶子**非消耗品**，靠**充能 (charges)** 使用；击杀回充（普通怪给 power 的一半、稀有 / 传奇 ×2，存档点 / 水井全满）[^poe2wiki-quality]。
- 品质 = 1% more 生命 / 魔力恢复 / 1% 品质（§5）。
- 瓶子可被蜕变 / 增幅加魔法词条（如即时恢复、增加充能、移除流血等 utility 词条），词条池见 `Data/ModFlask.lua`。

### 9.2 护符 (Charms)

PoE2 把 PoE1 的 utility 瓶职能搬到**护符**——装在腰带的护符槽，**满足触发条件时自动触发**防御效果。底材带 `charm = { duration, chargesUsed, chargesMax, buff }`、`type = "Charm"`、`quality = 20`[^pob-flask]：

| 护符 | 触发条件 | buff | 持续 | 单次消耗 | req level |
|------|---------|------|------|---------|-----------|
| Thawing Charm | 被冰缓 (Frozen) | Immune to Freeze | 3s | 40 | 12 |
| Staunching Charm | 开始流血 | Immune to Bleeding | 3s | 30 | 18 |
| Antidote Charm | 中毒 | Immune to Poison | 3s | 20 | 24 |
| Dousing Charm | 点燃 | Immune to Ignite | 3s | 30 | 32 |
| Grounding Charm | 感电 | Immune to Shock | 3s | 30 | 32 |
| Stone Charm | 被眩晕 | Cannot be Stunned | 3s | 20 | 8 |

- **护符槽上限 3**（`maximum_caltrops` 之外，wiki 明确 charm slot 上限 3）[^poe2wiki-quality]。
- 护符品质 = 1% increased 局部持续时间。
- 护符充能：击杀回充（普通怪给 power 的一半）。
- 护符词条池见 `Data/ModCharm.lua`。

---

## 对 pobr 实现的启示

对应未来的 **pobr-item**（raw item 解析 / 底材 / 增强物）、**pobr-build**（角色状态 / 点数编排）、**pobr-tree**（珠宝 / 半径）域：

### pobr-item

1. **底材数据模型**：`BaseItemDef` 需带 `quality`（默认上限 20、硬上限 30、Breach 例外 40）、`req {level, str, dex, int}`、`implicit`、`socketCount`（按部位上限 1/2，腐化可超）、以及 flask/charm 子结构（`life/duration/chargesUsed/chargesMax/buff`）。
2. **品质作为局部 more**：武器品质 → "1% more 局部物理"（只放大底材物理，不放大附加 / 转化非物理）；护甲品质 → "1% more 局部防御"。并入底材缩放管线，先于局部 increased。
3. **增强物（符文 / 灵魂核心 / 护身符）按插入部位选词条**：建模成 `Augment { effects: HashMap<SlotKind, Vec<Modifier>> }`，插入时按部位取对应组；区分局部（武器，"with this weapon"）vs 全局（护甲）。Ancient Augment 加全角色 ×1 限制。`SourceKind::Rune` / `SoulCore` 进 trace。
4. **需求语义**：未满足需求 = 物品禁用（非数值惩罚）；建模需求转换（`Giant's Blood` 翻倍、灵魂核心 "convert 20% requirements to X"）。
5. **iLvl ≠ reqLevel**：iLvl 只约束词条 / tier（属 crafting 域），装备校验用 reqLevel。

### pobr-build

6. **角色派生**：从职业起始属性 + 等级 + 属性派生生命 / 魔力 / 精准（见 attributes.md），别用全 0 默认。
7. **点数预算**：被动点（升级 + 任务，~123）、武器组点（24，转化常规点而非新增）、觉醒点（8 = 4×2）、地图集点；respec 成本用 `goldRespecPrices` 表 / `AscendancyRespecCost` / `AtlasPassiveRespecCost`。
8. **武器组 (weapon sets)**：2 组各自武器 / Spirit / 武器组天赋子树；计算需支持按 active set 切换快照（Keystone / 珠宝插槽不分组）。
9. **瓶子 / 护符**：瓶子只生命 / 魔力（utility 移到护符）；护符是条件触发 buff——建模为带触发条件的 buff 提供者，充能管线复用 recovery-charges-buffs.md。

### pobr-tree

10. **珠宝**：普通珠宝（无需求、按属性底材）放树插槽；Timeless / Time-Lost 是半径珠宝，需实现半径内节点改造（`PassiveTreeJewelDistanceMultiplier = 1.2`）。
11. **不实现 PoE1 cluster jewel**：`Data/ClusterJewels.lua` 是遗留数据；以普通 / 半径珠宝为准，cluster 暂不建模。

---

## 参考来源

[^poe2wiki-rare]: PoE2 Wiki — Rare / Item rarity / IIR-IIQ / Magic find. https://www.poe2wiki.net/wiki/Rare
[^poe2wiki-050]: PoE2 Wiki — Version 0.5.0. https://www.poe2wiki.net/wiki/Version_0.5.0
[^poe2wiki-quality]: PoE2 Wiki — Quality（各类型效果 / 上限 / 例外）. https://www.poe2wiki.net/wiki/Quality
[^poe2wiki-augments]: PoE2 Wiki — Augments / Socketables（0.5.0 改名、覆盖、Ancient Augments）. https://www.poe2wiki.net/wiki/Augments
[^poe2wiki-weaponset]: PoE2 Wiki — Weapon set passive skill points. https://www.poe2wiki.net/wiki/Weapon_set_passive_skill_points
[^poe2wiki-trial]: PoE2 Wiki — Ascension trials. https://www.poe2wiki.net/wiki/Trial
[^poe2wiki-gem]: PoE2 Wiki — Gem（等级上限 / 升华缩放）. https://www.poe2wiki.net/wiki/Gem
[^poe2db-asc]: PoE2DB — Ascendancy Points（4 组 × 2 = 8）. https://poe2db.tw/us/Ascendancy_Points
[^poe2db-timeless]: PoE2DB — Timeless Jewel. https://poe2db.tw/us/Timeless_Jewel
[^mobalytics-runes]: Mobalytics — Runes, Sockets & Soul Cores. https://mobalytics.gg/poe-2/guides/runes-sockets
[^mobalytics-tree]: Mobalytics — Passive Skill Tree. https://mobalytics.gg/poe-2/guides/passive-skill-tree
[^mobalytics-gearing]: Mobalytics — League Start Gearing Guide（iLvl 断点）. https://mobalytics.gg/poe-2/guides/league-start-gearing-guide
[^game8-sockets]: Game8 — Gear Sockets Explained. https://game8.co/games/Path-of-Exile-2/archives/489096
[^game8-tree]: Game8 — Passive Skill Tree（123 总点数）. https://game8.co/games/Path-of-Exile-2/archives/487065
[^thegamer-runes]: TheGamer — Runes / Soul Cores / Talismans / Sockets Guide. https://www.thegamer.com/path-of-exile-2-poe2-runes-soul-cores-talismans-sockets-effects-guide/
[^orbis-ilvl]: Orbis — How important is item level in PoE 2. https://orbispatches.com/gaming-faq/how-important-is-item-level-in-poe-2
[^alphagamer-tiers]: Alphagamer — Item Modifiers, Modifier Tiers and Item Tiers. https://legacy.alphagamer.net/path-of-exile-2-understanding-item-modifiers-modifier-tiers-and-item-tiers/
[^crafting-breakpoints]: 见本仓 [crafting.md](./crafting.md) §重要断点。
[^poe2-tree-report]: PoE2 Passive Tree 0.4→0.5 Change Report（Sinister Jewel Sockets）. https://poe2-05-tree.netlify.app/
[^maxroll-050]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^pob-misc]: PathOfBuilding-PoE2 — `src/Data/Misc.lua`（gameConstants / characterConstants / goldRespecPrices）. https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/dev/src/Data/Misc.lua
[^pob-gems]: PathOfBuilding-PoE2 — `src/Data/Gems.lua`（reqStr/reqDex/reqInt / naturalMaxLevel=20）.
[^pob-flask]: PathOfBuilding-PoE2 — `src/Data/Bases/flask.lua`（flask / charm 底材数值）.
[^pob-jewel]: PathOfBuilding-PoE2 — `src/Data/Bases/jewel.lua`（Jewel / Radius / Timeless subType）.
[^pob-runes]: PathOfBuilding-PoE2 — `src/Data/ModRunes.lua`（Rune / SoulCore 按部位词条）.
[^pob-cluster]: PathOfBuilding-PoE2 — `src/Data/ClusterJewels.lua`（PoE1 遗留数据，非 0.5.0 游戏现状）.
