# PoB 核心模块深度分析（上）

> 历史说明：本文件最初基于 DeepWiki 对 `PathOfBuildingCommunity/PathOfBuilding`（PoE1）仓库的研究结果。PoBR 当前以 `PathOfBuildingCommunity/PathOfBuilding-PoE2` 为 parity 主参考；本文件中的 PoE1-only 字段（例如 bandit、pantheon）只能作为架构参考，不能直接作为 PoE2 Build 数据契约。后续需要按 PoB-PoE2 `Build.lua` / `ConfigOptions.lua` / `QuestRewards.lua` 重扫并拆分为新版文档。

---

## 1. Build 状态中心（Build.lua）

`Build.lua` 定义了 `buildMode` 对象，是整个应用的**单一状态中心**。

### 1.1 核心字段

| 字段 | 类型 | 含义 |
|------|------|------|
| `dbFileName` | string | 构建文件路径 |
| `buildName` | string | 构建名称 |
| `viewMode` | string | 当前视图（"TREE"/"ITEMS"/"SKILLS"/"CALCS"/"CONFIG"/"IMPORT"） |
| `characterLevel` | number | 角色等级 |
| `targetVersion` | string | 目标游戏版本 |
| `bandit` | string | 盗贼选择 |
| `pantheonMajorGod` / `pantheonMinorGod` | string | 万神殿选择 |
| `spec` | object | 天赋树规格（包含已分配节点） |
| `modFlag` | boolean | 是否有未应用的修改（触发重计算） |
| `unsaved` | boolean | 是否有未保存的更改 |

### 1.2 核心方法

| 方法 | 签名 | 职责 |
|------|------|------|
| `Init` | `Init(dbFileName, buildName, buildXML, convertBuild, importLink)` | 初始化构建，从 XML 或文件加载 |
| `Load` | `Load(xml, fileName)` | 从 XML 解析构建详情（等级、天赋、装备、技能等） |
| `LoadDB` | `LoadDB(buildXML, buildName)` | 从 XML 数据加载 |
| `LoadDBFile` | `LoadDBFile()` | 从文件加载 |
| `CloseBuild` | `CloseBuild()` | 关闭当前构建 |
| `CanExit` | `CanExit(mode)` | 检查是否可退出（未保存时弹提示） |
| `Shutdown` | `Shutdown()` | 退出时保存 |

### 1.3 Rust 映射

```rust
// pobr-build/src/build.rs
pub struct Build {
    pub db_file_name: Option<PathBuf>,
    pub build_name: String,
    pub view_mode: ViewMode,
    pub character_level: u8,
    pub target_version: GameVersion,
    pub bandit: BanditChoice,
    pub pantheon: Pantheon,
    pub spec: PassiveTreeSpec,
    pub items: Vec<Item>,
    pub socket_groups: Vec<SocketGroup>,
    pub config: BuildConfig,
    pub mod_flag: bool,
    pub unsaved: bool,
}
```

---

## 2. 计算环境（CalcSetup.lua）

`CalcSetup.lua` 的 `calcs.initEnv(build, mode, spec, rawOutput, override)` 创建 `env` 对象，它是**所有计算的唯一上下文**。

### 2.1 `env` 对象核心字段

| 字段 | 类型 | 含义 |
|------|------|------|
| `build` | Build ref | 对 Build 状态的引用 |
| `data` | Data ref | 全局游戏数据引用 |
| `configInput` | table | 用户在 ConfigTab 的输入 |
| `mode` | string | 计算模式（"CALCS", "MAIN", "CALCULATOR"） |
| `spec` | object | 天赋树规格 |
| `override` | table | 覆盖参数 |
| `classId` | string | 职业 ID |
| `modDB` | ModDB | 玩家的 Modifier 数据库 |
| `enemyDB` | ModDB | 敌人的 Modifier 数据库 |
| `itemModDB` | ModDB | 物品的 Modifier 数据库 |
| `enemyLevel` | number | 敌人等级 |
| `player` | Actor | 玩家 Actor |
| `enemy` | Actor | 敌人 Actor |
| `requirementsTableItems` / `requirementsTableGems` | table | 物品/宝石需求（力/敏/智） |
| `radiusJewelList` | table | 范围珠宝列表 |
| `extraRadiusNodeList` | table | 额外范围节点 |
| `grantedSkills` / `grantedSkillsNodes` / `grantedSkillsItems` | table | 来源：天赋树/节点/物品的赋予技能 |
| `flasks` / `tinctures` | table | 激活的药剂/酊剂 |
| `player.activeSkillList` | table | 玩家的主动技能列表 |
| `auxSkillList` | table | 辅助技能列表 |
| `player.weaponData1` / `weaponData2` | table | 主手/副手武器数据 |
| `mainSocketGroup` | number | 主技能宝石组索引 |

### 2.2 Actor 结构

`player` 和 `enemy` 遵循统一的 Actor 结构：

| 字段 | 类型 | 含义 |
|------|------|------|
| `modDB` | ModDB | Actor 的 Modifier 数据库 |
| `level` | number | 等级 |
| `output` | table | 计算后的统计结果（写入目标） |
| `breakdown` | table | 详细分解数据（供 UI 展示） |
| `itemList` | table | （仅 player）装备物品列表 |
| `mainSkill` | ActiveSkill | （仅 player）当前选中的主动技能 |
| `enemy` | Actor ref | 反向引用到对手 Actor |

### 2.3 Rust 映射

```rust
// pobr-core/src/calc/env.rs
pub struct Env<'a> {
    pub build: &'a Build,
    pub data: &'a GameData,
    pub config_input: &'a BuildConfig,
    pub mode: CalcMode,
    pub spec: &'a PassiveTreeSpec,
    pub override_params: Option<OverrideParams>,
    pub class_id: ClassId,
    pub player: Actor,
    pub enemy: Actor,
    pub item_mod_db: ModDb,
    pub enemy_level: u8,
    pub requirements: Requirements,
    pub radius_jewels: Vec<RadiusJewel>,
    pub extra_radius_nodes: Vec<NodeId>,
    pub granted_skills: Vec<GrantedSkill>,
    pub flasks: Vec<Flask>,
    pub tinctures: Vec<Tincture>,
    pub active_skills: Vec<ActiveSkill>,
    pub aux_skills: Vec<AuxSkill>,
    pub main_socket_group: Option<usize>,
}

// pobr-core/src/calc/actor.rs
pub struct Actor {
    pub mod_db: ModDb,
    pub level: u8,
    pub output: OutputTable,
    pub breakdown: BreakdownTable,
    pub item_list: Vec<Item>,
    pub main_skill: Option<ActiveSkill>,
}
```

---

## 3. 计算编排（CalcPerform.lua）

### 3.1 主入口

```lua
function calcs.perform(env, skipEHP)
```

| 参数 | 类型 | 说明 |
|------|------|------|
| `env` | table | 由 `CalcSetup.initEnv` 初始化的计算环境 |
| `skipEHP` | boolean? | 是否跳过 EHP（有效生命值）估算 |

**无返回值**，结果写入 `env.player.output` 和 `env.enemy.output`。

### 3.2 执行阶段

```
1. 初始化天赋石修饰符
2. 初始化召唤物技能
3. 处理药剂效果 (flasks, tinctures)
4. doActorAttribsConditions(env, env.player)     -- 属性和条件
5. doActorLifeMana(env.player)                    -- 生命/魔力池
6. 保留计算 (life/mana reservation)
7. 增益/减益处理 (buffs/debuffs)
8. 充能计算 (charges: power/frenzy/endurance)
9. calcs.defence(env, env.player)                 -- 防御计算
10. if not skipEHP then calcs.buildDefenceEstimations(env, env.player) end
11. calcs.triggers(env, env.player)               -- 触发器计算
12. calcs.mirages(env)                            -- 幻影计算
13. calcs.offence(env, env.player, env.player.mainSkill)  -- 攻击计算
```

### 3.3 Rust 映射

```rust
// pobr-core/src/calc/perform.rs
pub fn perform(env: &mut Env, skip_ehp: bool) -> Result<(), CalcError> {
    init_jewel_mods(env)?;
    init_minion_skills(env)?;
    apply_flask_effects(env)?;
    
    calc_actor_attribs_conditions(env, &mut env.player)?;
    calc_actor_life_mana(&mut env.player)?;
    calc_reservations(env)?;
    apply_buffs_and_debuffs(env)?;
    calc_charges(env)?;
    
    defence::calc_defence(env, &mut env.player)?;
    if !skip_ehp {
        defence::build_defence_estimations(env, &mut env.player)?;
    }
    triggers::calc_triggers(env, &mut env.player)?;
    mirages::calc_mirages(env)?;
    offence::calc_offence(env, &mut env.player, env.player.main_skill.as_ref())?;
    
    Ok(())
}
```

---

## 4. 完整调用链

```
用户操作（换装备 / 点天赋 / 改配置）
    ↓
Build.lua: 更新状态，设置 modFlag = true
    ↓
Main.lua / CalcsTab: 触发重计算
    ↓
CalcSetup.lua: initEnv(build, mode, spec, ...) → env
    ↓
CalcPerform.lua: perform(env)
    ├── 阶段 1: 收集所有 Modifier 来源（天赋、装备、技能、药剂）
    │     → 填充 env.player.modDB / env.enemy.modDB
    ├── 阶段 2: 属性/条件/生命/魔力/保留/增益/充能
    ├── 阶段 3: calcs.defence(env, player) → player.output.{Armour,Evasion,Life,Resist,...}
    ├── 阶段 4: calcs.triggers(env, player)
    ├── 阶段 5: calcs.mirages(env)
    └── 阶段 6: calcs.offence(env, player, mainSkill) → player.output.{DPS,CritChance,totalHitAvg,...}
    ↓
BuildDisplayStats.lua / CalcBreakdown.lua: 格式化输出
    ↓
CalcsTab: 展示结果
```
