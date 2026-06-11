# M3-T1 A5 双跑 diff 报告：旧 parse_config vs RawConfigInputs + config_interpreter

> 产出：`crates/pobr-build/tests/config_dualrun.rs::dual_run_old_subset_of_new`
> （`cargo test -p pobr-build --test config_dualrun -- --nocapture`）。
> 数据源：ninja 18-build（`examples/demo-bd-test/builds/*/code.txt`）+
> 5 个 `crates/pobr-build/tests/fixtures/config_*.xml`。
> 日期：2026-06-12；catalog = `data/4.5.0.3.4/overlay/config_options.json`
> （559 条目，verified:false = 54）。

## 0. 结论与现网状态

- **旧 ⊆ 新成立**：旧路径全部产出项（conditions / multipliers / global_texts /
  enemyIsBoss / resistancePenalty 标量）均被新路径覆盖，**不可解释项 = 0**
  （测试 hard assert）。
- **交集逐值相等**：同名 conditions / multipliers、quest 奖励数值、两个标量
  包装（`enemy_tier_from_config` / `campaign_progress_from_config`）逐值断言通过。
- **现网行为逐值不变**：`parse_build` 仍消费旧 `parse_config`（冻结为
  `parse_config_legacy` doc(hidden) 双跑参照）；新路径
  （`parse_config_inputs` + `interpret` + `ConfigCatalog`）已可用但无业务消费方。
  ninja_parity / golden 全绿（= 逐值不变）。
- **本波不开新增覆盖**：下述 §2 各类按蓝图 D3 属行为 commit，待主线显式审查
  后逐类打开（每类独立 commit + baseline 显式审查 + ConfigOptions.lua 行号）。

## 1. 覆盖判定口径（旧产出项的三层覆盖）

1. **同名同值**：新 `ConfigOutcome.conditions/multipliers` 回填表直接命中，逐值相等。
2. **mod 化覆盖**：vendor 忠实形态把旧的全局表条目落成带门控 tag
   （`Condition:Combat` / `Condition:Effective`）或 enemy actor 化 / vendor
   原拼写命名的 Modifier——载荷值逐值相等，但生效路径改变（属行为语义，门控未开）。
3. **handler 缺口**：catalog 条目带 `handler_id` 且未注册——原始输入已被
   `RawConfigInputs` 无损捕获（标量回显可查），解释产出待后续 handler 批次。

## 2. 分类汇总（17 类，93 项；跨 23 个数据源聚合去重）

### 2.1 交集逐值相等（现网等值，无行为风险）

| 类 | 条数 | 样本 |
|----|------|------|
| conditions 同名同值 | 1 | TargetingBrandedEnemy（defaultState=true 实体化） |
| multipliers 同名同值 | 0* | （18-build XML 中旧路径命中的 multiplier 均落「同名同值」或 mod 化；本轮无冲突值） |
| quest 奖励逐值相等（parser == 声明式 effects） | 5 | Spirit ×3 / ManaRegen / StunThreshold |
| quest 行 parser 不支持（旧路径同落 Unsupported 通道） | 6 | `+1 Charm Slot`、`30% increased Charm Effect Duration` 等 |
| 标量逐值相等：enemyIsBoss | 1 | None |
| 标量逐值相等：resistancePenalty | 1 | Act4（-30） |

### 2.2 命名/形态口径差异（值相等，待行为 commit 统一）

| 类 | 条数 | 明细 |
|----|------|------|
| enemy 条件 actor 化（`EnemyX` → enemy 桶 `Condition:X`+Effective tag） | 4 | EnemyBleeding / EnemyChilled / EnemyIgnited / EnemyShocked |
| 条件 mod 化（Combat/HaveCompanion 等门控 tag，依赖 D5 mode_combat） | 6 | AttackedRecently / BeenHitRecently / ChampionIntimidate / CompanionInPresence / CritRecently / UsedWarcryRecently |
| 条件 vendor 命名口径 | 1 | EnemyCriticalWeakness → `Condition:ApplyCriticalWeakness` |
| quest 命名口径差异（parser 名 ≠ vendor 名，类型+值相等） | 12 | ColdResistance→ColdResist、MaximumLife→Life、Strength→Str、Intelligence→Int、Dexterity→Dex 等（M6 parser 规则统一口径时一并裁决） |

### 2.3 新增覆盖项（旧路径完全不产出；逐类行为 commit 候选）

| 类 | 条数 | 明细 |
|----|------|------|
| count 型 / implyCond / 非前缀 condition | 11 | Stationary、UsedSkillRecently、SkillCritRecently、CritInPast8Sec、BannerPlanted、Burning、UsedWarcryInPast8Seconds、AverageResourceGain、UseCurrentEnergyShield、FlameWallAddedDamage、averageRepeat |
| multiplier（count 条目数值化） | 5 | StationarySeconds、CurrentEnergyShield、CurrentManaPercentage、PurpleFlamesCount、SigilOfPowerStage |
| enemy 数值覆盖（BASE 直注 enemy 桶 + EnemyConfig 归因） | 6 | FireResist、LightningResist、SelfCritChance、Multiplier:ChillStacks/ShockStacks/ScorchStacks |
| customMods 行通道（StripEscapes + mod_parser，含不可解析行可见性） | 1 源 ×3 行 | fixture 验证：2 行可解析 + 1 行 Unsupported |
| 标量 default 实体化（XML 省略 → catalog defaultIndex） | 2 | enemyIsBoss=Pinnacle、resistancePenalty=-60(Endgame)（均与现网消费方回退值一致 → 实际打开为零 diff） |

### 2.4 已知缺口（解释产出待回补，原始输入已无损捕获）

| 类 | 条数 | 明细 |
|----|------|------|
| handler 缺口（命中 18-build 输入的未注册 handler） | 8 | ConcPath/FlickerStrike/VigilantStrike BypassCD、inDemonForm、multiplierNearbyEnemies、multiplierNearbyRareOrUniqueEnemies、quest Tribal Medicine、quest Seven Pillars |
| unhandled 全量（含 default 激活的未注册 handler） | 20 | 上述 8 + SecondsSinceInevitableCrit、bannerValour、elementalConfluxElement、enemySizePreset、touchedDebuffsCount、raiseSpectre/summonCompanion/summonElementalRelic Enable* 族 |
| tag 维度未接通（T5-E1 ActorCondition / actor Multiplier，保守跳过） | 3 | EnemyColdExposure / EnemyFireExposure / EnemyLightningExposure（catalog 效果带 `actor_condition` tag，解释器记 diagnostics 跳过） |

## 3. 遗留项（待行为 commit / 后续波次清单）

按蓝图 D3「每项独立 commit + PoB2 行号 + baseline 显式审查」执行：

1. **xml_build 切换主路径**：`parse_build` 改消费
   `parse_config_inputs` + interpreter（含 §2.2/§2.3 全部语义打开）；前置依赖
   D5 `mode_combat` 行为 commit（Combat 门控条件）与 T5-E1（Exposure 三条）。
2. **count condition / implyCond 打开**（§2.3 行 1-2；ConfigOptions.lua
   conditionStationary/UsedSkillRecently 族）。
3. **enemy 数值覆盖打开**（§2.3 行 3；敌方抗性/暴击覆盖直注 enemy db）。
4. **customMods 生效**（§2.3 行 4；build 层喂 `session.add_modifier_texts`，
   source=Custom，vendor ConfigOptions.lua:2278-2296）。
5. **quest / 条件命名口径统一**（§2.2；与 M6 parser 规则、M1 statmap 名空间
   一并裁决，避免双名共存）。
6. **第二批 config handlers**（§2.4；优先命中 18-build 的 8 个缺口）。
7. **T5-E1 后回补 ActorCondition/actor Multiplier 条目**（§2.4 行 3）。
8. **删旧码**：上述全部打开、报告复核干净后删除 `parse_config_legacy` /
   `ParsedConfig` 导出与旧 `parse_config`（独立 commit）。

## 4. 监控基线（A6）

- config handler 注册数 = 2（`config:enemyIsBoss` 包装、`config:presetBossSkills`
  stub）≤ 预算 54；registry 总数 < 100（`handlers.rs::handler_counts_within_budget`）。
- catalog `verified:false` = 54 / 559（`pobr-gamedata ruleset` 测试打印并断言 ≤54）。
- vendor 已知数据瑕疵：`conditionEnemyExitedPresenceRecently` 同 var 双条目
  （Exited/Entered 共用输入键，vendor 原样），`by_var` 按后写覆盖。
