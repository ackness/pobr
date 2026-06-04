# 计算机制原语

---

## 1. 结论

PoBR 的计算核心不能只实现 `base + inc + more`。PoE/PoE2 的核心机制大量依赖上限、下限、可提升最大值、分阶段伤害、技能等级系数、异常状态实例和服务器帧限制。

因此 `pobr-core` 需要优先建立以下原语：

1. `StatBoundary`：统一处理 final/total/overcap/missing/max/floor。
2. `SkillUseTime`：统一处理 attack/cast/skill/action speed、固定 total time penalty 和服务器帧限制。
3. `DamageComponent`：按 source、damage type、hit/dot/ailment 分桶。
4. `SkillLevelEffect`：保存技能等级带来的基础伤害、damage of base、use time、crit、quality effect。
5. `AilmentInstance` / `DebuffInstance`：表示 bleed、poison、ignite、corrupted blood 等可堆叠或取最高值的持续效果。

---

## 2. 搜索到的关键机制

### 2.1 Skill Speed 与最小使用时间

PoE2 的 `Skill Speed` 是通用技能速度。它和具体类型速度叠加，例如 Attack Speed、Cast Speed、Warcry Speed、Totem Placement Speed。`Skill Speed` 与 `Action Speed` 是不同乘区。

当前应建模为：

```text
use_speed = 1 / base_use_time
use_speed_after_skill_speed = use_speed * (1 + total_skill_speed / 100)
final_use_time = (1 / use_speed_after_skill_speed) + total_use_time_penalty
```

等价：

```text
final_use_time = base_use_time / (1 + total_skill_speed / 100) + use_time_penalty
```

其中 `+# seconds to Total Attack/Cast Time` 类惩罚在速度修正后追加，不能被速度 modifier 缩放。

非吟唱动作还存在服务器帧限制：一次动作最小约 33ms，即约 30.3 actions/s。计算核心应同时输出：

```text
tooltip_action_rate = 1 / final_use_time
effective_action_rate = min(tooltip_action_rate, server_tick_rate)   // 非 channelling
```

早期可以先把 `server_tick_rate = 1 / 0.033` 放入 `GameConstants`，并在 breakdown 中标记是否被 server frame cap 影响。

### 2.2 抗性上限、下限与 overcap

抗性必须同时保留 uncapped/total 与 final：

```text
uncapped = base + sum(Base) * inc/more modifiers if applicable
max = min(hard_max_resist_cap, base_max + sum(MaxResist Base))
final = clamp(truncate(uncapped), resist_floor, truncate(max))
overcap = max(0, uncapped - max)
missing = max(0, max - final)
```

PoE2 默认最大元素/混沌抗性是 75%，最大抗性硬上限是 90%。PoB-PoE2 在初始化阶段注入 `FireResistMax` / `ColdResistMax` / `LightningResistMax` / `ChaosResistMax`，并把元素抗性进度惩罚作为 `FireResist` / `ColdResist` / `LightningResist` 的 `BASE` modifier。

计算核心需要把 `MaxFireResistance` / `MaxColdResistance` / `MaxLightningResistance` / `MaxChaosResistance` 和 `MaxElementalResistance` 作为独立 stat，而不是把最大值硬编码进 resistance 函数。

### 2.3 伤害类型与 damage bucket

伤害不能只保存单个 `PhysicalDamage` 数值。至少需要这些维度：

```rust
pub enum DamageSource {
    Attack,
    Spell,
    Secondary,
    Thorns,
    Ailment,
    Debuff,
}

pub enum DamageKind {
    Hit,
    Dot,
}

pub struct DamageComponent {
    pub source: DamageSource,
    pub kind: DamageKind,
    pub damage_type: DamageType,
    pub min: f64,
    pub max: f64,
    pub tags: SkillTypes,
}
```

核心阶段建议：

1. base damage：武器、技能宝石、flat added damage。
2. skill coefficient：attack damage of base、base spell damage、damage effectiveness。
3. conversion：damage conversion。
4. gain as extra：gain % of X as extra Y。
5. increased/reduced：按 source、type、skill tag 匹配。
6. more/less：按 modifier 语义独立连乘。
7. mitigation：抗性、护甲、penetration、ignore/inverted resistance。
8. hit result：crit、lucky/unlucky、double damage、taken modifiers。
9. derived ailment：从 pre-mitigation 或指定 hit component 生成 ailment/debuff instance。

### 2.4 流血

PoE2 当前流血是物理持续伤害异常：

```text
base_bleed_dps = pre_mitigation_physical_hit * 0.15
base_duration = 5s
base_total = pre_mitigation_physical_hit * 0.75
```

要点：

- 流血默认由物理 hit 贡献 magnitude。
- 需要显式流血几率或“inflicts bleeding”来源。
- 伤害 modifier 已经通过造成 hit 的 pre-mitigation damage 体现，流血本身不再直接吃“你造成的伤害”类 modifier。
- monster 身上的流血在目标移动或 aggravated 时造成额外 100% 伤害。
- 同类流血默认不叠加，持续时间独立，只取当前最高伤害实例。
- 0.5.0 官方 patch 明确：玩家身上的流血不再因为玩家移动而提高伤害；玩家造成给怪物的流血不受此改动影响。

### 2.5 腐化之血

Corrupted Blood 是 physical DoT debuff，但不是 bleeding。

要点：

- 每个目标最多 10 层。
- 新层会刷新已有层持续时间。
- 不受 bleeding 相关 stat 影响。
- 来源公式不同，例如怪物按怪物等级，部分技能/支持按生命、力量、击杀目标最大生命等来源给出固定公式。

因此它不能用 `AilmentKind::Bleeding` 的路径复用，只能共享 `DebuffInstance`、duration、stacking、physical dot mitigation 等底层结构。

### 2.6 技能宝石等级与伤害系数

技能 gem 数据需要独立进入计算，而不是写死在公式里。每个 skill level 至少要保存：

```rust
pub struct SkillLevelEffect {
    pub level: u8,
    pub base_damage: Vec<DamageRange>,
    pub attack_damage_of_base: Option<f64>,
    pub attack_speed_of_base: Option<f64>,
    pub added_damage_effectiveness: Option<f64>,
    pub cast_time: Option<f64>,
    pub attack_time: Option<f64>,
    pub crit_chance: Option<f64>,
    pub cooldown: Option<f64>,
    pub stored_uses: Option<u8>,
    pub cost: SkillCost,
    pub quality_stats: Vec<Modifier>,
}
```

PoE2 attack skill 常见显示为 `Attack Damage: X% of base` 和 `Attack Speed: X% of base`。Spell 通常由技能自身等级表提供 base damage、cast time 和 crit chance。

0.5.0 还引入了“Gem Quality grants Socketed Skills an additional effect”的设计要求，因此 `quality_stats` 必须是可扩展 modifier 列表，不能只保存一个固定百分比。

---

## 3. `pobr-core` 目标接口

### 3.1 StatBoundary

```rust
pub struct StatBoundary {
    pub stat: StatId,
    pub total: f64,
    pub final_value: f64,
    pub floor: Option<f64>,
    pub max: Option<f64>,
    pub hard_cap: Option<f64>,
    pub overcap: f64,
    pub missing: f64,
}

pub fn bounded_stat(
    db: &ModDb,
    cfg: &CalcConfig,
    stat: StatId,
    boundary: BoundarySpec,
) -> StatBoundary;
```

第一批测试：

- 抗性默认 `max = 75`。
- `+max resistance` 提升 final 上限。
- hard cap 90 阻止最大抗性继续提高。
- uncapped 超过 max 时输出 overcap。
- curse/exposure 类减抗影响 total，再被 max clamp。

### 3.2 SkillUseTime

```rust
pub struct SkillUseTime {
    pub base_use_time: f64,
    pub total_skill_speed: f64,
    pub total_action_speed: f64,
    pub total_use_time_penalty: f64,
    pub tooltip_use_time: f64,
    pub tooltip_rate: f64,
    pub effective_rate: f64,
    pub capped_by_server_tick: bool,
}
```

第一批测试：

- cast speed 与 skill speed 同一 additive speed bucket。
- action speed 是独立乘区。
- `+0.4s to Total Attack Time` 在速度后追加。
- 非 channelling skill 的 effective rate 不超过 30.3/s。

### 3.3 Damage 与 Ailment

```rust
pub struct HitDamageResult {
    pub pre_mitigation: Vec<DamageComponent>,
    pub post_mitigation: Vec<DamageComponent>,
    pub total_hit_avg: f64,
    pub ailments: Vec<AilmentInstance>,
    pub debuffs: Vec<DebuffInstance>,
}
```

第一批测试：

- 物理 hit 可以生成 bleed magnitude。
- 火/冰/电/混沌 damage type 分别匹配各自 inc/more/resistance。
- `ElementalDamage` 同时匹配火/冰/电，不匹配混沌。
- Corrupted Blood 与 Bleeding 使用不同 stat namespace。
- DoT cap、stack cap、highest-instance-wins 作为独立测试。

---

## 4. 开发顺序

1. 先实现 `GameConstants` 与 `bounded_stat`，重构当前 resistance。
2. 实现 `SkillUseTime`，替换当前 `base_action_rate * speed` 的简化公式。
3. 实现 `DamageComponent`，把当前单一物理 hit 改成 damage vector。
4. 接入 fire/cold/lightning/chaos hit damage 与对应 resistance mitigation。
5. 添加 `SkillLevelEffect` fixture，先做 2-3 个技能的等级表。
6. 实现 bleed 的最小路径：hit -> bleed instance -> dot dps。
7. 实现 corrupted blood 的最小路径：fixed formula -> stack capped debuff。
8. 用 PoB/PoE2 fixture 做 golden regression。

---

## 5. 资料来源

- PoE2 Wiki: `Skill Speed`，说明 skill speed 与 action speed 区分、use time 公式、33ms server frame 限制。
- PoE2 Wiki: `Resistance`，说明默认最大抗性 75%、最大抗性硬上限 90%、抗性减伤公式。
- PoE2 Wiki: `Bleeding`，说明 bleed 基础 magnitude、持续时间、aggravated/moving、最高实例规则。
- PoE2 Wiki: `Corrupted Blood`，说明 corrupted blood 不是 bleeding、最多 10 层、刷新持续时间。
- PoE2 0.5.0 official patch notes，确认玩家移动不再提高玩家身上 bleed damage、部分 Cast Speed 词条更新为 Skill Speed、Gem Quality 额外效果。
- PathOfBuildingCommunity/PathOfBuilding-PoE2 `CalcSetup.lua` / `CalcDefence.lua` / `Data.lua`，确认 PoE2 计算常量、抗性进度惩罚和 resistance clamp 输出模型。
