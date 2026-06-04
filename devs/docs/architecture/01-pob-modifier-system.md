# PoB Modifier 系统深度分析

> 原始文件：`ModParser.lua`, `ModDB.lua`, `ModStore.lua`

---

## 1. Modifier 数据结构

Lua 中 Modifier 是一个表：

```lua
{
    name = "FireDamage",
    type = "INC",         -- INC / MORE / BASE / FLAG / OVERRIDE / etc.
    value = 10,
    source = "Item:Lightning Coil",
    flags = bit.bor(ModFlag.Attack, ModFlag.Melee),
    keywordFlags = 0,
    extraTags = {
        { type = "Condition", var = "Shocked" },
        { type = "Multiplier", var = "EnduranceCharge", limit = 3 },
    }
}
```

### 字段说明

| 字段 | 说明 |
|------|------|
| `name` | 统计名称，如 `"FireDamage"`, `"Life"`, `"AttackSpeed"` |
| `type` | 修饰符类型：`BASE`, `INC`, `MORE`, `FLAG`, `OVERRIDE`, `LIST` 等 |
| `value` | 数值（`FLAG` 类型时通常为布尔值） |
| `source` | 来源字符串（物品名、天赋节点名、技能名等） |
| `flags` | 位掩码，限制适用的上下文（`ModFlag.Attack`, `ModFlag.Spell` 等） |
| `keywordFlags` | 关键词标志位掩码（基于技能关键词） |
| `extraTags` | 额外条件/限制，如 `Condition`, `Multiplier`, `PerStat`, `SkillType` |

---

## 2. ModParser 解析流程

`ModParser.lua` 的 `modLib.parseMod(text)` 将游戏文本解析为 Modifier 对象。

### 解析步骤

以 `"+10% increased Fire Damage while Shocked"` 为例：

```
Step 1: PreFlags
  → 扫描 preFlagList，无匹配

Step 2: Skill Tag
  → 扫描 preSkillNameList，无匹配

Step 3: Mod Form (核心)
  → 匹配 formList["^(+%d+)%% increased"]
  → 提取 value = 10, type = "INC"
  → 剩余文本: "Fire Damage while Shocked"

Step 4: Mod Tags
  → 扫描 modTagList
  → 匹配 "while Shocked" → Condition tag

Step 5: Mod Name
  → 扫描 modNameList
  → "Fire Damage" → "FireDamage"

输出 Mod:
{
    name = "FireDamage",
    type = "INC",
    value = 10,
    flags = 0,
    extraTags = { { type = "Condition", var = "Shocked" } }
}
```

### 关键数据结构

| 结构 | Lua 形式 | 用途 |
|------|----------|------|
| `formList` | `{ [pattern] = ModType }` | 匹配修饰符开头，确定类型和值 |
| `modNameList` | `{ [text] = ModName or {ModName...} }` | 将文本映射到统计名称 |
| `modTagList` | `{ [pattern] = tag_spec }` | 匹配条件/限制标签 |
| `preFlagList` | `{ [pattern] = KeywordFlag }` | 前置标志（如 "Attacks"） |
| `preSkillNameList` | `{ [name] = true }` | 附魔技能名称 |

### 关键函数

```lua
-- 主解析入口
modLib.parseMod(text) → { mods, extra } or nil

-- 构造 Modifier
modLib.createMod(name, type, value, source, flags, keywordFlags, ...)
```

### 解析缓存

`ModParser.lua` 在开发模式下运行后会生成 `ModCache.lua`。运行期优先从缓存加载，只有缓存中不存在的 Modifier 才会调用 `ModParser` 实时解析。所有文本在解析前统一转小写。

---

## 3. ModStore / ModDB 接口

`ModStore.lua` 是基类，`ModDB.lua` 是具体实现。

### ModStoreClass 字段

- `parent`: 父 ModStore（实现层级继承）
- `actor`: 关联的 Actor
- `multipliers`: 乘数修饰符缓存
- `conditions`: 条件修饰符缓存

### ModStoreClass 方法

```lua
ScaleAddMod(mod, scale, replace)
-- 添加修饰符，根据 scale 缩放（unscalable 标志除外）
```

### ModDB 方法

```lua
AddMod(mod)                    -- 添加修饰符到数据库
Sum(type, cfg, ...)            -- 汇总匹配指定类型和配置的修饰符值
More(cfg, ...)                 -- 计算匹配修饰符值的乘积
Flag(cfg, ...)                 -- 检查是否存在匹配的 FLAG 类型修饰符
NewMod(name, type, value, source, ...)  -- 创建并添加新修饰符
```

### 查询参数 `cfg`

`cfg` 是一个配置表，包含当前计算上下文：

| 字段 | 说明 |
|------|------|
| `skillName` | 技能名称 |
| `skillTypes` | 技能类型集合 |
| `damageType` | 伤害类型 |
| `flags` | `ModFlag` 组合 |
| `keywordFlags` | 关键词标志 |
| `conditions` | 各种条件状态（`OnFullLife`, `CritRecently`, `Shocked` 等） |

ModDB 查询时，会遍历所有同名 Modifier，根据 `cfg` 中的条件判断是否匹配，然后聚合。

---

## 4. 性能特征

- **查询复杂度**：`Sum` / `More` / `Flag` 每次都要遍历同名 Modifier 列表，做条件过滤。
- **数据规模**：典型 Build 的 ModDB 包含 **3000~8000** 条 Modifier。
- **查询频率**：每次计算属性时可能触发数十次 `Sum` 调用。
- **Lua 瓶颈**：表查找 + 动态类型检查在热路径上开销巨大。

---

## 5. Rust 映射设计

```rust
// pobr-core/src/modifier/mod.rs
pub struct Modifier {
    pub name: ModName,
    pub mod_type: ModType,
    pub value: ModValue,
    pub source: ModSource,
    pub flags: ModFlags,
    pub keyword_flags: KeywordFlags,
    pub tags: Vec<ModTag>,
}

pub enum ModType {
    Base,
    Inc,
    More,
    Flag,
    Override,
    List,
}

pub enum ModTag {
    Condition { var: String },
    ActorCondition { actor: ActorType, var: String },
    Multiplier { var: String, limit: Option<i32>, global_limit: Option<i32> },
    PerStat { stat: String, div: Option<i32> },
    SkillType(SkillType),
    SkillName(String),
    GlobalEffect { effect_type: String, effect_name: String },
}
```

```rust
// pobr-core/src/mod_db/mod.rs
pub struct ModDb {
    mods: HashMap<ModName, Vec<Modifier>>,
    conditions: HashMap<String, bool>,
    multipliers: HashMap<String, i32>,
}

impl ModDb {
    pub fn add_mod(&mut self, mod: Modifier);
    pub fn sum(&self, mod_type: ModType, cfg: &CalcConfig, names: &[ModName]) -> f64;
    pub fn more(&self, cfg: &CalcConfig, names: &[ModName]) -> f64;
    pub fn flag(&self, cfg: &CalcConfig, name: ModName) -> bool;
}

pub struct CalcConfig {
    pub skill_name: Option<String>,
    pub skill_types: SkillTypes,
    pub damage_type: Option<DamageType>,
    pub flags: ModFlags,
    pub keyword_flags: KeywordFlags,
    pub conditions: HashMap<String, bool>,
}
```
