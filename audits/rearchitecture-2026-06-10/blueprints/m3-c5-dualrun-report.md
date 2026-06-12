# M3-T3 C5 aura 双跑 diff 报告（D3 双跑点 2）

> 日期：2026-06-12；测试 = `crates/pobr-build/tests/aura_dualrun.rs`
> （`cargo nextest run -p pobr-build --features buff-pass-aura --test aura_dualrun --no-capture`）；
> 语料 = ninja 18-build（`examples/demo-bd-test/builds/`）；
> 比较口径 = `pobr_core::extract_display_values` 全部 Computed 展示字段逐位比较
> （ninja_parity 同编排选项：`mode_effective=false` 面板口径、Pinnacle）+
> curse 面板新增字段（`enemy_curse_limit` / `curse_slots`）单列。

## 0. 结论

- **18/18 build 全列逐值持平，0 build 有差异**——旧路径（`aura_buff_modifiers`
  静态直注 + `mode_buffs=false`）与新路径（feature `buff-pass-aura` 开 +
  `mode_buffs=true` + 关静态直注，aura 经 BuffSpec → buff_pass 乘区）在全部
  Computed 展示字段上**逐位相等**。
- 蓝图 D3 点 2 的两类口径都满足：
  - 无 AuraEffect 词条的 build **逐值持平**（要求满足）；
  - 「有 AuraEffect/BuffEffectOnSelf 词条的 build 差异即修复目标」——本语料
    **无该类词条载体**（见 §2.1），乘区恒 `(1+0/100)×1.0 = 1.0`，新路径退化为
    纯通道替换，无差异即正确。
- C5-2 切换为**零 diff 切换**（display 面板维度）+ **加法型新增**（curse 面板
  字段从中性 0/空 变为真值，§2.2）。baseline 预期不动（ninja_parity 比较列
  不含 curse 面板字段）。

## 1. 双跑设置

| 路径 | feature | `mode_buffs` | `aura_buff_modifiers` 静态直注 | BuffSpec → buff_pass |
|------|---------|--------------|-------------------------------|----------------------|
| 旧（现网） | 开（编译需要）| false | 注入 | 整段空转（mode_buffs gate） |
| 新（C5 目标） | 开 | true | 关闭 | aura 乘区 + curse priority/limit |

旧路径在 feature 开 + `mode_buffs=false` 下与 feature 关的默认产线逐值一致
（T3 已证：ninja_parity 两 feature 态全绿，commit 41f9871 message）。
双跑开关 = `DataOrchestratorOptions::buff_pass_aura`（C5-1 临时脚手架，
C5-2 切换后删除）。

## 2. 逐 build 结果

### 2.1 display 全列（Computed 展示字段）

| build | aura/curse 载体 | diff 列数 |
|-------|----------------|-----------|
| druid-oracle-comet | Blasphemy+ElementalWeakness（hex）、Malice 等 | 0（持平） |
| druid-oracle-ember-fusillade | 无 buff spec | 0（持平） |
| huntress-ritualist-bow-shot | 无 buff spec | 0（持平） |
| huntress-spirit-walker-twister | 有（limit=1） | 0（持平） |
| mercenary-gemling-legionnaire-explosive-grenade | 无 buff spec | 0（持平） |
| mercenary-tactician-wolf-pack | 有（limit=1） | 0（持平） |
| monk-invoker-frost-bomb | Blasphemy+ElementalWeakness（hex）等 | 0（持平） |
| monk-martial-artist-flicker-strike | 无 buff spec | 0（持平） |
| monk-martial-artist-twister | Sniper's Mark（mark） | 0（持平） |
| ranger-deadeye-explosive-grenade | 无 buff spec | 0（持平） |
| ranger-pathfinder-ice-shot | Freezing Mark（mark） | 0（持平） |
| sorceress-chronomancer-essence-drain | 有（limit=1） | 0（持平） |
| sorceress-disciple-of-varashta-comet | 有（limit=1） | 0（持平） |
| sorceress-stormweaver-comet | Arctic Armour 等 | 0（持平） |
| warrior-smith-of-kitava-shield-wall | 无 buff spec | 0（持平） |
| warrior-titan-shield-wall | 无 buff spec | 0（持平） |
| witch-abyssal-lich-detonate-dead | 有（limit=1） | 0（持平） |
| witch-blood-mage-coiling-bolts | **Impurity（aura，ChaosResistance buff）**、Blasphemy+Despair/Enfeeble | 0（持平） |

逐值持平的机制解释（非偶然）：

1. **aura 防御 buff 通道等值**：BuffSpec.mods 与 `aura_buff_modifiers` 同一
   取数/归因口径（同 `map_aura_buff_stat`、同 SourceId，C1 已测等值）；本语料
   无 `AuraEffect`/`BuffEffect(OnSelf)` 系词条 → `mult = 1.0`，
   `ScaleAddMod` 在 `scale == 1.0` 时原值返回（`ModStore.lua:45-79` 镜像的
   early-return），故缩放路径逐位 = 直注路径。witch-blood-mage 的 Impurity
   （ChaosResistance buff）即该等值的活体证据。
2. **条件置位无消费**：新路径置 `AffectedByAura` / `AffectedBy<名>`（vendor
   CalcPerform.lua:2107-2110）与 `EnemyCursed`/`EnemyMarked`（:2969-2984）、
   multiplier `BuffOnSelf`（:2949-2951）/`CurseOnEnemy`（:2983）——18-build
   语料中无任何词条以这些条件/乘数为 tag，置位零消费（行为提升的接线已就位，
   命中词条时自动生效）。
3. **hex 在面板口径被 vendor 同款 gate 挡下**：`mode_effective=false`（ninja
   面板口径）下 hex 不入槽（vendor :2289 `(mode_effective and ...) or mark`
   同口径）；mark 不受 gate，两个 mark build 入槽（§2.2）。curse 携带词条 M3
   本就为空（C1 范围声明），不影响数值列。

### 2.2 curse 面板新增字段（旧路径恒 0/空 → 新路径真值，加法型）

| build | enemy_curse_limit | curse_slots |
|-------|-------------------|-------------|
| druid-oracle-comet | 0 → 1 | []（hex 被 mode_effective gate） |
| huntress-spirit-walker-twister | 0 → 1 | [] |
| mercenary-tactician-wolf-pack | 0 → 1 | [] |
| monk-invoker-frost-bomb | 0 → 1 | [] |
| monk-martial-artist-twister | 0 → 1 | ["Snipers Mark"] |
| ranger-pathfinder-ice-shot | 0 → 1 | ["Freezing Mark"] |
| sorceress-chronomancer-essence-drain | 0 → 1 | [] |
| sorceress-disciple-of-varashta-comet | 0 → 1 | [] |
| sorceress-stormweaver-comet | 0 → 1 | [] |
| witch-abyssal-lich-detonate-dead | 0 → 1 | [] |
| witch-blood-mage-coiling-bolts | 0 → 1 | [] |
| 其余 7 build（无 aura/curse 技能） | 0 → 0 | []（buff_pass 空转，无 spec） |

`enemy_curse_limit = 1` = 基线 1（vendor CalcSetup.lua:648，数据镜像
`base_player_mods.json::enemy_curse_limit`）+ Σ BASE（语料无加成词条）。
归类：**正确性修复（保留）**——vendor `output.EnemyCurseLimit`（:2830）与
curse 槽位（:2845-2896）首次有真值；display_catalog 不含此二字段（蓝图 §6.3
「M3 不扩 display_catalog」），不影响 parity 比较列。

## 3. 差异分类汇总（蓝图 C5-1 三分类）

| 分类 | 条数 | 明细 |
|------|------|------|
| 正确性修复（保留） | 2 字段 ×11 build | §2.2 curse 面板真值（vendor :2830/:2845-2896 依据） |
| 口径简化副作用（buff_pass.rs 文档 (a)-(i)） | 0 | 简化 (a)-(i) 在本语料零命中：无 per-skill AuraEffect 词条（a）、无 party（b）、无 auraCannotAffectSelf 载体（c）、无 ExtraAuraEffect（d）、乘区 1.0 不触发取整精度（e）、无 Debuff spec（f）、Blasphemy 的 CurseFromAura 权重未建模但 hex 本就被 gate 挡下（g）、无 SelfCast/CurseBuff（h）、撇号名（Sniper's→Snipers）只影响 curse_base 查表回退 0，单 mark 无槽位竞争，零数值影响（i） |
| 意外（修复） | 0 | — |

## 4. C5-2 切换裁决输入

- display 面板维度零 diff + curse 面板加法型 → **可切换**。
- D5 MAIN 口径核对（vendor CalcSetup.lua:583-597 实读）：非 CALCS 模式
  `buffMode` 恒 `"EFFECTIVE"` → `env.mode_buffs = true`（与 mode_combat /
  mode_effective 同置）。PoBR 编排入口对应置 `with_mode_buffs(true)`；
  `mode_effective` 维持调用方选项（D5 显式裁决：「敌侧 debuff/curse 维持既有
  mode_effective 口径」——PoBR 的面板/有效双口径由调用方区分，vendor 的
  MAIN=EFFECTIVE 恒等仅约束 mode_buffs/mode_combat 两维）；`mode_combat`
  置位属 B4 独立行为 commit。
- feature `buff-pass-aura` 取舍：**删 flag**（回退通道 = revert C5-2/C5-3
  commit，与 M1-T2.4 statmap 切换同模式）；`DataOrchestratorOptions::buff_pass_aura`
  脚手架字段同步删除。
