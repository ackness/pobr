# PoB Calc 模块深度分析

> 原始文件：`CalcOffence.lua`, `CalcDefence.lua`, `CalcActiveSkill.lua`

---

## 1. 伤害计算（CalcOffence.lua）

### 1.1 主入口

```lua
function calcs.offence(env, actor, activeSkill)
```

| 参数 | 说明 |
|------|------|
| `env` | 计算环境 |
| `actor` | 当前 Actor（通常是 player） |
| `activeSkill` | 要计算的主动技能 |

结果写入 `actor.output`。

### 1.2 核心伤害函数

```lua
local function calcDamage(activeSkill, output, cfg, breakdown, damageType, typeFlags, convDst)
    -- 返回: min, max
```

| 参数 | 说明 |
|------|------|
| `activeSkill` | 当前技能 |
| `output` | 输出表（写入目标） |
| `cfg` | 计算配置 |
| `breakdown` | 分解表（供 UI 展示） |
| `damageType` | 当前伤害类型（"Physical"/"Fire"/"Cold"/"Lightning"/"Chaos"） |
| `typeFlags` | 伤害类型标志组合 |
| `convDst` | 转换目标类型（可选） |

### 1.3 calcDamage 内部步骤

1. **类型标志合并**：将当前 `damageType` 的标志合并到 `typeFlags`
2. **伤害转换**：遍历 `dmgTypeList`（物理、闪电、冰霜、火焰、混沌），递归调用 `calcDamage` 计算被转换/获得的伤害（`addMin`, `addMax`）
3. **基础伤害检查**：如果 `baseMin` 和 `baseMax` 都为 0，直接返回转换伤害
4. **Modifier 聚合**：
   - 获取与 `typeFlags` 匹配的 `modNames`
   - `inc = Sum("INC", cfg, modNames...)`
   - `more = More(cfg, modNames...)`
   - 特定最小/最大值的 more/less：`genericMoreMinDamage` / `genericMoreMaxDamage` / `moreMinDamage` / `moreMaxDamage`
5. **最终伤害**：
   - `min = (baseMin + addMin) * (1 + inc) * more * ...`
   - `max = (baseMax + addMax) * (1 + inc) * more * ...`
6. **细分记录**：如果 `breakdown` 存在，记录每一步的中间值

### 1.4 输出字段

| 字段 | 说明 |
|------|------|
| `totalHitMin` / `totalHitMax` / `totalHitAvg` | 总击中伤害 |
| `totalDot` | 总持续伤害 |
| `totalDotInstance` | DoT 实例伤害 |
| `speed` | 攻击/施法速度 |
| `critChance` | 暴击率 |
| `critMultiplier` | 暴击倍率 |
| `hitChance` | 命中率 |
| `DPS` | 每秒伤害 |
| `bleedChance` / `igniteChance` / `shockChance` | 异常状态触发几率 |
| `doubleDamageChance` | 双倍伤害几率 |

### 1.5 Rust 映射

```rust
// pobr-core/src/calc/offence.rs
pub fn calc_offence(
    env: &Env,
    actor: &mut Actor,
    active_skill: &ActiveSkill,
) -> Result<OffenceOutput, CalcError>;

fn calc_damage(
    active_skill: &ActiveSkill,
    output: &mut OutputTable,
    cfg: &CalcConfig,
    breakdown: Option<&mut BreakdownTable>,
    damage_type: DamageType,
    type_flags: TypeFlags,
    conv_dst: Option<DamageType>,
) -> (f64, f64);

pub struct OffenceOutput {
    pub total_hit_min: f64,
    pub total_hit_max: f64,
    pub total_hit_avg: f64,
    pub total_dot: f64,
    pub speed: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub hit_chance: f64,
    pub dps: f64,
    pub bleed_chance: f64,
    pub ignite_chance: f64,
    pub shock_chance: f64,
    pub double_damage_chance: f64,
}
```

---

## 2. 防御计算（CalcDefence.lua）

### 2.1 核心函数

```lua
function calcs.defence(env, actor)
```

| 辅助函数 | 签名 | 用途 |
|----------|------|------|
| `calcs.hitChance(evasion, accuracy)` | `(number, number) -> number` | 命中率（5%~100% 钳制） |
| `calcs.armourReduction(armour, raw)` | `(number, number) -> number` | 护甲伤害减免百分比 |
| `calcs.applyDmgTakenConversion(...)` | ... -> (table, number) | 伤害转换与承受伤害计算 |

### 2.2 输出字段

| 字段 | 说明 |
|------|------|
| `Armour` | 护甲值 |
| `Evasion` | 闪避值 |
| `EnergyShield` | 能量护盾 |
| `Life` / `Mana` | 生命/魔力 |
| `FireResist` / `ColdResist` / `LightningResist` / `ChaosResist` | 抗性 |
| `PhysicalMaximumHitTaken` | 物理最大承受伤害 |
| `BlockChance` / `SpellBlockChance` | 格挡/法术格挡 |
| `DodgeChance` / `SpellDodgeChance` | 躲避/法术躲避 |
| `SpellSuppressionChance` | 法术压制 |
| `EHP` | 有效生命值（估算） |

### 2.3 Rust 映射

```rust
// pobr-core/src/calc/defence.rs
pub fn calc_defence(env: &Env, actor: &mut Actor) -> Result<DefenceOutput, CalcError>;

pub fn hit_chance(evasion: f64, accuracy: f64) -> f64;
pub fn armour_reduction(armour: f64, raw_damage: f64) -> f64;

pub struct DefenceOutput {
    pub armour: f64,
    pub evasion: f64,
    pub energy_shield: f64,
    pub life: f64,
    pub mana: f64,
    pub fire_resist: f64,
    pub cold_resist: f64,
    pub lightning_resist: f64,
    pub chaos_resist: f64,
    pub physical_max_hit_taken: f64,
    pub block_chance: f64,
    pub spell_block_chance: f64,
    pub dodge_chance: f64,
    pub spell_dodge_chance: f64,
    pub spell_suppression_chance: f64,
    pub ehp: f64,
}
```

---

## 3. 主动技能处理（CalcActiveSkill.lua）

### 3.1 创建主动技能

```lua
function calcs.createActiveSkill(activeEffect, supportList, actor, socketGroup, summonSkill)
```

| 参数 | 说明 |
|------|------|
| `activeEffect` | 主动技能效果对象（来自 Gem） |
| `supportList` | 支持宝石效果列表 |
| `actor` | 技能使用者 |
| `socketGroup` | 插槽组 |
| `summonSkill` | 召唤技能（如果适用） |

### 3.2 内部逻辑

1. **初始化 `activeSkill` 表**：
   ```lua
   activeSkill = {
       activeEffect = activeEffect,
       supportList = supportList,
       actor = actor,
       summonSkill = summonSkill,
       socketGroup = socketGroup,
       skillData = {},
       buffList = {},
       skillTypes = {},        -- 从 grantedEffect 复制
       minionSkillTypes = {},
       skillFlags = {},        -- 从 grantedEffect.baseFlags 复制
       skillModList = new ModList(parent=actor.modDB),
       baseSkillModList = new ModList(parent=actor.modDB),
       effectList = {},
   }
   ```

2. **处理支持宝石**：
   - 遍历 `supportList`
   - 兼容性检查：`calcLib.canGrantedEffectSupportActiveSkill(supportEffect, activeSkill)`
   - 如果兼容：将 `supportEffect.addSkillTypes` 合并到 `activeSkill.skillTypes`
   - 记录被拒绝的支持宝石索引

3. **合并技能修饰符**：
   - 遍历 `activeSkill.effectList`（包含主动宝石和所有支持宝石）
   - 对每个 `effect`：调用 `calcs.mergeSkillInstanceMods(env, skillModList, effect, extraStats)`
   - 处理 `manaMultiplier`（魔力倍率）、`manaReservationPercent`（保留百分比）

4. **添加等级/品质修饰符**：
   - 添加 `GemLevel` 和 `GemQuality` 修饰符
   - 应用支持宝石对这些属性的加成

### 3.3 合并技能实例修饰符

```lua
function calcs.mergeSkillInstanceMods(env, modList, skillEffect, extraStats)
```

- 根据 `skillEffect` 的等级和统计数据，计算并添加相应的 Modifier 到 `modList`
- 处理技能等级缩放（level scaling）
- 合并 `extraStats`（额外统计）

### 3.4 Rust 映射

```rust
// pobr-core/src/calc/active_skill.rs
pub fn create_active_skill(
    active_effect: &SkillEffect,
    support_list: &[SupportEffect],
    actor: &Actor,
    socket_group: &SocketGroup,
    summon_skill: Option<&ActiveSkill>,
) -> ActiveSkill;

pub fn merge_skill_instance_mods(
    env: &Env,
    mod_list: &mut ModList,
    skill_effect: &SkillEffect,
    extra_stats: &[(StatId, f64)],
);

pub struct ActiveSkill {
    pub active_effect: SkillEffect,
    pub support_list: Vec<SupportEffect>,
    pub actor: ActorRef,
    pub summon_skill: Option<Box<ActiveSkill>>,
    pub socket_group: SocketGroup,
    pub skill_data: SkillData,
    pub buff_list: Vec<Buff>,
    pub skill_types: SkillTypes,
    pub minion_skill_types: SkillTypes,
    pub skill_flags: SkillFlags,
    pub skill_mod_list: ModList,
    pub base_skill_mod_list: ModList,
    pub effect_list: Vec<SkillEffect>,
    pub mana_multiplier: f64,
    pub mana_reservation_percent: f64,
}
```
