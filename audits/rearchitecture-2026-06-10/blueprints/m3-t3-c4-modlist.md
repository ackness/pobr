# M3-T3 C4-2 敌方向词条迁移清单

> 蓝图 §6.4 C4 的范围控制产物：**先列清单再动手**。每条附 PoB2 一手依据
> （vendor = `vendor/PathOfBuilding-PoE2`，commit `2df5a74`，与 `vendor/.pob2-version.txt` 一致；
> 行号均为实读核对）。语料 = ninja_parity 18-build（`examples/demo-bd-test/builds/`），
> 采集方法：`POBR_DBG_DROPPED=1 cargo test -p pobr-build --test ninja_parity`（结构性
> ParseError 被 `filter_parseable` 丢弃的词条）+ `POBR_DBG_UNSUPPORTED=1`（软 Unsupported）。

## 0. 范围勘察结论

**(a) mod_parser 现有「Enemy 前缀 ModName」中语义为「写敌方 db」的子集：空集。**

逐项核对 `crates/pobr-core/src/mod_parser.rs` 全部 `Enemy*` 出现点：

| 出现点 | 形态 | 判定 |
|--------|------|------|
| `EnemyIgnited/Burning/Chilled/Frozen/Shocked/Bleeding/Poisoned`（`strip_tag_once`，` against <X> enemies` 后缀族） | 玩家侧 mod 的 `Condition` tag 变量名 | 非敌方 db 写入，不迁移 |
| `against rare or unique enemies` 等稀有度后缀 | 同上（`RareOrUnique` 条件 tag） | 非敌方 db 写入，不迁移 |
| `cannot evade enemy attacks` → `CannotEvade` flag | 玩家自身防御 flag（玩家不可闪避） | 非敌方 db 写入，不迁移 |
| 注释中的 `EnemyCritEffect`/`EnemyFreezeBuildup` | 仅消费侧命名说明，parser 不产出 | 无动作 |

`Enemy<X>Damage` / `EnemyFirePen` 等 Enemy 前缀 ModName 由 `calc/setup_env.rs` 注入
（EHP placeholder），不经 mod_parser，不在 C4 范围。

**(b) 语料中 Unsupported/Dropped 的敌方向词条**：见 §1（迁移）与 §2（登记待审）。

## 1. 迁移安全子集（C4-2 落地）

判定标准：现状为**结构性 ParseError**（`filter_parseable` 整行丢弃，零 modifier 注入、
零消费）→ 迁移后解析为 `Modifier{ name:"EnemyModifier", mod_type:List,
value:NestedMods([inner]) }` 落 player db。在 C4-3 转发落地前 `EnemyModifier` 名无任何
聚合消费（`mod_db.list` 对 NestedMods 返回空），故本子集 = 「现状 Err → 新增覆盖」，
ninja_parity 逐值不变。

inner mod 一律附 `Condition:Effective` tag（pobr 敌侧 debuff 统一口径，对照
`calc/setup_env.rs::push_enemy_effective_number` 与 vendor ConfigOptions.lua 敌况条目的
`{ type = "Condition", var = "Effective" }` 门控，如 :1682/:1737/:1878）——保证面板口径
（`mode_effective == false`，ninja_parity 现行口径）转发后仍逐值不变。

敌况条件变量名遵循 pobr 既有约定（vendor enemy-actor 条件 `<X>` ↔ build config
`conditionEnemy<X>` ↔ pobr cfg `Enemy<X>`，见 mod_parser.rs ` against <X> enemies` 族
注释）：vendor `Cursed` → pobr `EnemyCursed`；`EnemyInPresence` 为 vendor 玩家侧条件原名
（CalcPerform.lua:449 由 PresenceRadius 派生，pobr 暂无派生器 → 默认 false，词条惰性）。

### 1.1 通用前缀规则（递归解析剩余文本）

| pobr 前缀 | 行为 | vendor 依据（ModParser.lua preSkillNameList） |
|-----------|------|------------------------------------------------|
| `enemies you curse take ` | inner 名加 `Taken` 后缀 + `Condition:EnemyCursed` | :1367 `{ tag = Condition Cursed, applyToEnemy = true, modSuffix = "Taken" }` |
| `enemies you curse ` | `Condition:EnemyCursed` | :1368 `{ tag = Condition Cursed, applyToEnemy = true }` |
| `nearby enemies take ` | inner 名加 `Taken` 后缀 | :1371 `{ modSuffix = "Taken", applyToEnemy = true }` |
| `nearby enemies have ` | 无附加条件 | :1372 `{ applyToEnemy = true }` |
| `nearby enemies deal ` | 无附加条件 | :1373 `{ applyToEnemy = true }` |
| `enemies in your presence `（含 have/gain/deal 变体） | `Condition:EnemyInPresence` | :1416-1417 `{ applyToEnemy = true, tag = ActorCondition enemy EnemyInPresence }` |

applyToEnemy 包装语义 = ModParser.lua:6733-6748（inner mod 包成
`mod("EnemyModifier", "LIST", { mod = ... })`；prefix 的 `tag` 经 :6637 起的
「Combine flags and tags」段并入 inner tagList）。

### 1.2 语料命中条目（迁移后产物）

| 语料词条（18-build 频次） | inner 产物 | vendor 依据 |
|---------------------------|-----------|-------------|
| `Enemies you Curse take 6% increased Damage`（×3，rune 附魔） | `DamageTaken INC 6 {EnemyCursed, Effective}` | ModParser.lua:1367（前缀）；递归体 `6% increased damage` 走通用 form |
| `Enemies you Curse are Hindered, with 15% reduced Movement Speed`（×2） | `Condition:Hindered FLAG {EnemyCursed, Effective}`（移速数字是 Hinder 定义的展示文本，vendor 不另产 mod） | ModParser.lua:1368 + flagTypes :6290 `["hindered,? with (%d+)%% reduced movement speed"] = "Condition:Hindered"`；ModCache.lua:5055 全行缓存逐字段核对 |
| `Enemies in your Presence are Slowed by 20%`（×1） | `ActionSpeed INC -20 {EnemyInPresence, Effective}` | ModParser.lua:2862（vendor 把 EnemyInPresence 条件挂**外层**；pobr 挂 inner——单 cfg 模型下聚合时点等价，且避免「条件在 env_finalize 阶段 2 之后才置真」的转发时点截断，见 env_finalize.rs 模块文档）；ModCache.lua:5031 |
| `Enemies in your Presence have 10% reduced Cooldown Recovery Rate`（×1） | `CooldownRecovery INC -10 {EnemyInPresence, Effective}`（inner 专用名解析，**不**进 parse_name 通用表——玩家侧同名词条另有消费链，见 §2-R1） | ModParser.lua:1417；ModCache.lua:5037 |
| `Enemies in your Presence Gain 7% of Damage as Extra Chaos Damage`（×1） | `DamageGainAsChaos BASE 7 {EnemyInPresence, Effective}` | ModParser.lua:1417（递归体走 parse_conversion_or_gain）；ModCache.lua:5024（12% 变体缓存：`DamageGainAsChaos BASE 12 {ActorCondition enemy EnemyInPresence}`） |
| `Enemies in your Presence are Intimidated`（×1） | `Condition:Intimidated FLAG {EnemyInPresence, Effective}` | ModParser.lua:1416 + flagTypes :6283 `["intimidated"]`；ModCache.lua:5030 |
| `Enemies in your Presence are Hindered`（×1，物品扫描计） | `Condition:Hindered FLAG {EnemyInPresence, Effective}` | ModParser.lua:4293（专条）；ModCache.lua:5028 |

消费现状（迁移当下）：`DamageTaken`/`<X>DamageTaken` 在 enemy db 被
`offence.rs::enemy_damage_multiplier` 消费（C4-3 转发后、`EnemyCursed`+`Effective` 双真
才生效）；`ActionSpeed`/`CooldownRecovery`/`DamageGainAsChaos`/`Condition:*` flag 在
enemy db 当前**无消费者**（vendor 消费位：敌方出手/CalcSetup.lua:66-69 Intimidate 基础
mod 组等），零行为、留通道。

## 2. 登记待审子集（**不**迁移，候选独立行为 commit）

| # | 词条族（语料频次） | 不迁移原因 | 候选去向 |
|---|--------------------|-----------|----------|
| R1 | `N% increased Cooldown Recovery Rate`（玩家侧裸形 ×8+×5+…、`for Grenade Skills` ×7、`Bonded:` ×6+×2） | ~~非敌方向；新增解析会改 ninja_parity 进攻值~~ **已落地**（行为 commit，本行勘误）：parse_name 增 ModParser.lua:660-662 三别名 → `CooldownRecovery`；敌方向通道内专名特例随之删除（通用递归覆盖）。**实测 18-build parity 逐值零 diff**——原登记「会改进攻值」不成立：(a) 裸形载体 build 的主技能均无固有冷却（`CooldownRecovery` INC 仅经 `apply_cooldown_cap`/trigger ICDR 消费）；(b) cd-capped 榴弹 build 走 calc_orchestrator 旧近似（pre-truncate + `CooldownBypass`，末端 cap 被绕过，见该处 TODO）；(c) `Bonded:` 形带 `CanUseBondedModifiers` 条件默认不激活。**残余仍 Err**：`for Grenade Skills` ×7（vendor modTagList :1073 `SkillType.Grenade` tag——pobr `SkillTypes` u64 位掩码容不下 Grenade=159，需 tag 维度扩容）、`Warcry ...` ×1（KeywordFlag.Warcry 未落）、`Minions have ... for Command Skills` ×4（minion actor 维度）、`Spells have N% increased ...` ×1（`spells have` 谓词前缀未剥）、`Grenade Skills have +1 Cooldown Use` ×2（`AdditionalCooldownUses`，ModParser.lua:663-664） | ~~独立行为 commit~~ 已落地；残余项随各自维度（SkillType tag 扩容 / minion / warcry keyword）waves 收 |
| R2 | `Damage of Enemies Hitting you is Unlucky`（×2）/ `Enemy Critical Hit Chance against you is Unlucky`（×2）/ `20% chance for Damage of Enemies Hitting you to be Unlucky`（×1） | vendor 走 `against you` applyToEnemy+actorEnemy（ModParser.lua:1376）+ luck 模型；pobr 无 EHP 进伤 luck 消费链 | T4/EHP 后续波次 |
| R3 | `[Attack] Damage Penetrates 15% of Enemy Elemental Resistances`（×2）/ `Damage Penetrates 8% ...`（×1）；实测语料另有无 `of` 单元素形 `Damage Penetrates 18% Cold Resistance`（×2）/ `8%/10% Lightning`（×2）/ `8% Fire`（×1）/ `12% Elemental`（×1）——原登记 ×3 偏小，本行勘误 | ~~新增解析会改进攻 parity 值~~ **已落地**（行为 commit，本行勘误）：`parse_penetration` 覆盖 `[attack ]damage penetrates N% [of ][enemy ]<X> resistance(s)` 全形（PEN form ModParser.lua:96-98 + penTypes :6215-6221 + :6466-6472；oracle ModCache.lua:4549/:4893/:4874）→ `<X>Penetration` BASE N（attack 前缀 → ATTACK flag）。**实测 18-build parity 逐值零 diff**——原登记「会改进攻值」在当前面板口径下不成立：敌抗/穿透交互整体吃 `mode_effective` 门控（offence.rs `enemy_damage_multiplier` 仅 effective 口径调用），而 ninja_parity 现行 `mode_effective=false`；mods 已实测入 db（POBR_DBG_STAT 验证，来源 PassiveNode），D5 MAIN 口径切 effective 后即生效（方向：降敌抗 → DPS 向 golden 上修）。**残余仍 Err**：`... while Shapeshifted` 等带条件尾缀变体（ModCache.lua:4877） | ~~独立行为 commit~~ 已落地；条件尾缀变体随 tag 后缀波次收 |
| R4 | `Enemies near your Totems take N% increased Physical and Fire Damage`（语料未命中） | vendor ModParser.lua:2834-2837 双条 EnemyModifier；图腾条件链未落地 | 图腾波次 |
| R5 | `Nearby Enemies have -N% to all Resistances`（语料未命中） | vendor ModParser.lua:4283 产 `ElementalResist`+`ChaosResist`（enemy db 命名），与玩家侧 `*Resistance` 命名不同，需要 enemy 侧名表；递归路径会产错名。**M3-W3 复核（跳过登记）**：非纯名字归一——vendor 共享名 `ElementalResist` 在 pobr **无消费者**（enemy db 抗性按分元素 `Fire/Cold/LightningResist` 聚合，setup_env/offence 实读），落 vendor 名 = 静默失效、展开为 pobr 分元素名集 = 引入与 vendor 不同的 enemy 名表，正面撞 quest/parser 命名口径裁决（m3-t1-dualrun-report §2.2 行 4，与 M6 一并裁决）。有裁决耦合 → 本波次不动 | C4 后续小批（待 M6 命名口径裁决后带 enemy 名映射落地） |
| R6 | `Enemies in your Presence have Fire Exposure`（语料未命中） | vendor ModParser.lua:4294 `FireExposure BASE 20`；pobr `reduce_enemy_exposure` 在编排层、`perform`/env_finalize **之前**运行——转发产物赶不上归约时点，迁移即静默失效 | 曝光归约时点迁入 env_finalize 后再收 |
| R7 | `Enemies taunted by you cannot evade attacks`（语料未命中） | vendor ModParser.lua:2824 inner `CannotEvade FLAG {Condition:Taunted}`；嘲讽条件链未落地 | 嘲讽波次 |

## 3. C4-3 转发实现要点（env_finalize 阶段 2）

- 对照 vendor `CalcPerform.lua:486-500 applyEnemyModifiers`（实读）：
  `actor.modDB:Tabulate(nil, nil, "EnemyModifier")` 逐条取 `value.mod`，
  `source = mod.source or value.mod.source`（inner 缺来源时回退**外层** mod 的来源，
  :495），`cache[mod]` 按 **mod 实例身份**跳过已转发条目（跨多次调用幂等，:496-498；
  调用点 :762 / :1107-1111）。
- pobr 等价：player(+minions) db 过滤 `name == "EnemyModifier" && mod_type == List &&
  matches(cfg)` 的外层条目，展开 `NestedMods` inner；inner 的 `source`/`origin` 为空时
  回退外层（保留原 SourceId 归因穿透）；幂等去重用「enemy db 现存 modifier 指纹多重集」
  （`HashMap<指纹, 计数>`，指纹 = 回退后 inner 的 `Debug` 序列化）——值相等的多份来源
  各自保留（vendor 实例缓存同语义），重复调用不重复注入。
