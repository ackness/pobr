# 00 — 蓝图总索引 · 跨阶段依赖 · 执行顺序（总架构评审版）

> 撰写：2026-06-11 · 总架构评审产出。本文是 8 份实施蓝图的导航与跨阶段裁决记录；与单份蓝图冲突时**以本文裁决为准**（各蓝图正文已同步修订，见 §5 修正日志）。
> 上游：21-roadmap.md（阶段排期）· 20-target-architecture.md（P# 裁决）。

---

## 1. 蓝图索引

| 蓝图 | 阶段 | 一句话 | 体量 | 内部并行形态 | parity 目标 |
|---|---|---|---|---|---|
| [m1-skills-gems.md](m1-skills-gems.md) | M1 | gem quality / statmap JSON 化 / support 裁决 / 等级字段族 / 多 statSet | ~3 人周 | W0 串行 → T1–T5 并行 → T2.4/W-J 串行收尾 | 进攻 ≥40% |
| [m2-defence.md](m2-defence.md) | M2 | 扣池状态机 / keystone 开关 / taken-as / Block·Spirit·Evade·Stun 面板 / EHP 口径切换 | ~4 人周 | W0 契约批 → A–E 并行 → F 串行收口 | 防御 ≥80% |
| [m3-orchestration.md](m3-orchestration.md) | M3 | config_interpreter / buff_expander / aura·curse buff_pass / 异常闭环 / 第一次 drill | ~4 人周 | T0 地基 → T1–T5 并行 | 进攻 ≥55% / 防御 ≥85% |
| [m4-offence-deep.md](m4-offence-deep.md) | M4 | 归因 RFC + MH/OH·暴击双 pass / ModFlags 扩位 / 全乘区 / 技能 DoT / 触发接线 | ~5 人周 | T0–T5 六线（T1 关键路径先行） | 进攻 ≥70% |
| [m5a-minions.md](m5a-minions.md) | M5(a) | minions/spectres 入库 / minion build 链路 / createMinionSkills / mirage | ~2 人周 | A0 → A∥B∥C∥D∥E | 召唤 build 扩集 |
| [m5b-special-statdesc.md](m5b-special-statdesc.md) | M5(b) | special_mods 框架 + S0–S2 批次 / oracle parsemod / statdesc 渲染链路 | ~31 人日 | A∥B∥C∥D∥E（B-1 契约先行） | unsupported 率下降 |
| [m5c-item-tree.md](m5c-item-tree.md) | M5(c) | pobr-item 落地 / variant·applyRange / 武器局部结算 / 树字段·节点效果管线 | ~30 人日 | WI-0 拆分 → A–F 并行（passive_inject 接力） | BuildRaw 往返契约 |
| [m6-parser-rules.md](m6-parser-rules.md) | M6 | ModParser 六表 JSON 化 / scan 引擎重写 / precompile / stat_id 双通道 / 第二次 drill（终局验收） | ~4 人周 | A–F 六线（M6.3 切换 commit 硬串行点） | parse diff=0 / 零回归 |

---

## 2. 跨阶段依赖图（producer → consumer）

### 2.1 数据表 / 数据列

```
M1-W0/T4  granted_effect_levels.{mana_mult, reservation族, stored_uses, level_requirement}
              ├──→ M1-T4.4/T4.5（cost/spirit 聚合）
              └──→ M5a-C1（createMinionSkills 选级，level_requirement）
M1-T2     overlay/skill_stat_map.json + rules/stat_map_engine（删 skill_stat_map.rs 751 行）
              └──→ M4-W-D1（dot 基值映射走此表；蓝图已勘误，禁止恢复 Rust 启发式）
M1-T3/T5  GrantedEffectDef token 表达式列 / 多 statSet / GemEffects 外键
              ├──→ M3-T3（curse 技能识别用 skill_types token）
              └──→ M5a-A3（granted_effect_minions 边车 merge 进同一 Def）
M2-D      base_items.{block_chance, spirit}（16-G4 的 M2 份额）──→ M2 自消费
M2-W0.4   game_constants 四常量（ehp_calc_*, normal_enemy_dps_mult）──→ M2-F
M3-T1     overlay/config_options.json ──→ M3-T4（敌人异常状态来自 config enemy effects）
M3-T2     overlay/buff_definitions.json ──→ M3-T3/T4（flask handler buff:onslaught_flask）
M3-T4     base_items.flask{}/charm{} 列（16-G4 的 M3 份额）
M4-W-A2   overlay/high_precision_mods.json（唯一生产点，裁决 §4-3）
              ├──→ M4 mod_db ScaleAddMod/round_more
              ├──→ M5c-E2（节点效果缩放取整，只消费）
              └──→ M6（scan 引擎/写侧原语，只消费）
M4-T4     base_items.weapon.reload_time_ms（16-G4 的 M4 份额）+ skill_overrides 通道抽 dotIs*/bolt_count/doubleHits
M4-T5     overlay/trigger_configs.json + catalog/triggers.rs（M5a-D2 的 MirageConfigDef 扩展同文件，A 守门）
M5a-A2/A3 overlay/{minions,spectres,granted_effect_minions}.json ──→ M5a-B/C
M5b-C1    generated/special_derived.json（adapter 生产）──→ M6-T7 迁入 precompile-mods 并扩展（裁决 §4-5）
M5b-E     overlay/stat_descriptions.json + mods.json.rendered_lines ──→ M5c-A4 Craft 渲染（TODO(M5b) 标注）/ M6+ stat_id mods 域
M5c-C1    overlay/local_mods.json（唯一生产点，裁决 §4-4）──→ M5c-C2/C3、M2 标 consumer:"m2" 条目、M6 消费
M5c-D1    passive_tree 字段（is_attribute/options/isSwitchable/…）──→ M5c-D2/D3/E4、M6-T9（stat_id 树域）
M6-T2     overlay/mod_parser_rules.json ──→ M6-B 引擎
M6-T7     generated/parsed_mods.json + 覆盖率棘轮 ──→ 运行时零解析热路径
```

### 2.2 框架接口 / 机制

```
M0  rules/{mod.rs,registry.rs} + ruleset.rs + overlay merge + extract-lua 骨架 ──→ 全部阶段
W3  RuntimeConstants/RuleSet 注入管道 ──→ M2/M3/M4/M5*/M6 全部"常量经 cfg.constants"约定
M1  StatMapMode::Compare 双跑模式框架 ──→ M3 config 双跑 / M6 parser 双跑复用同模式
M2  keystone_registry（开关读 flag）──→ M3-E2 keystone_merge（注入 mods 含 flag 自动接通）──→ M5b-B5（复用，不另建展开点，裁决 §4-6）
M2  pool_damage/reduce_pools ──→ M2-F numberOfHitsToDie；（M5a mirage / M4 触发子计算互为同族 env-clone 原语，A0 互查复用）
M3  T0: SourceKind 四变体 + BuffSpec/env_finalize 框架 ──→ M3 各 track；M5a-D1 的 SourceKind::Mirage 沿同先例追加
M3  T1: rules/value_expr.rs（五算子+受限谓词唯一求值器，裁决 §4-1）──→ M5b-B2 复用(+enums) ──→ M6-T4 复用(+:cap)
M3  T5-E1: ModTag actor/limitActor + 求值上下文 ──→ M4-W-A3 EvalContext 在其上扩 PerStat/globalLimit（开工对齐签名）
M3  T5-F: version-bump-drill.sh 第一版 ──→ M6-T10 扩展（非新建，裁决 §4-7）
M3  Q3: HandlerRegistry 签名扩 HandlerCtx ──→ M5b-B2 以届时 master 签名为准统一（两蓝图已互相声明）
M4  RFC: PassId/Combine 归因模型 ──→ M4-T2；P17 红线约束 M2/M5a 不得提前改 TraceGraph
M4  W-A1 ModFlags 30 位 ──→ M5b ModTemplate.flags 位名 / M6 flag_phrases 位名反查
M4  W-A2 mod_db 写侧原语 ReplaceMod ──→ M4-W-D2 弩 Multiplier 回写
M4  W-A3 globalLimit ──→ M4-W-C1 DOUBLED 词条 / M6 DOUBLED form（未落地则 M6 保 Unsupported 单列）
M5b D-1: oracle --mode parsemod ──→ M5b-D2 differential ──→ M6-T6 复用扩展（裁决 §4-8）
M5b A-1/A-2: corpus.rs + unsupported 报表 ──→ M5a-E2 / M5c-§5.3 复用（裁决 §4-9）
M6  B: parse_mod(text, &CompiledParserRules) 收掉 M5b 双签名；D: 五调用方注入收口
```

---

## 3. 整体执行顺序与并行建议

### 3.1 主序（roadmap 既定，蓝图无变更）

```
M0 收尾(W3) → M1 → M2 → M3 → M4 → [M5a ∥ M5b ∥ M5c] → M6 → M7
```

### 3.2 可以激进并行的点

1. **M1 尾段 ∥ M2 头段**（可压缩 ~1 周）：M2-W0（词条/字段/契约/常量四个纯增量 commit）与 M2-A（pool_damage 纯函数库，无 perform 接线）不依赖 M1 任何产物，可在 M1 的 T2.4 切换收尾期并行开工。**交集与约束**：
   - `pipeline/config.json` + adapter `main.rs`：M1-W0 冻结后，M2-D 的 ShieldTypes 增表必须走独立的 W0 式独占 commit（两蓝图均已有此约定）；
   - spirit 族（`survivability.rs`/`skill_mechanics.rs`/OutputTable spirit 字段）：M1-T4.5 先合并，M2-D rebase 补 efficiency；字段切分 = M1 出 `spirit_reserved` 聚合、M2-W0.2 出 `spirit`/`spirit_unreserved` 池值（两蓝图已写入）；
   - `calc_orchestrator.rs`：M1 多函数区 vs M2-D 装备注入段（L781-930）不重叠，函数级归属即可；
   - `data/<ver>/**` regen 产物：每个含 regen 的 PR 合并前重跑 regen-check，后合并者负责 rebase 重生。
2. **M3-T0 ∥ M2-F**：M3 的接口地基（SourceKind/buffMode 字段/env_finalize 空框架，全部零行为）可在 M2-F 的 EHP 切换审查期并行落地。
3. **M4 的 RFC 评审 ∥ M3 尾段**：归因 RFC（M4 §1）是纯文档评审，可提前到 M3 验收期进行，缩短 M4-T2 等待。
4. **M5 三线并行**（roadmap 既定）+ **第 0 步硬序**：**M5c-WI-0（calc_orchestrator 拆出 item_local.rs / passive_inject.rs，纯搬迁 0.5-1 人日）必须在三线开工前最先合并**——它重排了 M5a-B/M5b-B4 都要碰的 2662 行热点文件；三线全部 rebase 其上再开工。其余热点：
   - `mod_parser.rs` 三写者：合并序 **M5b-B3（special 查表插入主流程，结构性）→ M5a-B3（minion 前缀段）→ M5c-E1（半径词条新模式）**；各自单 commit、段不重叠；
   - `ninja_parity.rs`：M5b-A 管报表段、M5a-E 管 baseline/allowlist 段、M5c 旁挂专项用例——后合并者 rebase；**M5b-A1/A2 建议三线开工第 0 天先行合并**（M5a-E2、M5c-§5.3 都消费它）；
   - `catalog/mod.rs` / `gamedata ruleset.rs` / `domains/mod.rs`：一表一文件互不冲突；挂载行按字母序 append，三线各自小 PR 快合，后合者 rebase（M5b/M5c 均指定 B 为仲裁人，跨蓝图沿用：**先合并者的仲裁人临时代理**）；
   - `rules/registry.rs` 签名扩参（M5b-B2）先于 M5a-D 的 mirage handler 注册。
5. **M6 内部**：A（抽取）/B（引擎，mini fixture 先行）/C（ModCache golden）三线第 0 天即可并行；硬串行点只有 M6.3 切换 commit。
6. **跨阶段不可并行的硬约束**（重申）：T5.3 全量 stat 入库 → M1-T2.4 删旧码；M2-A/B 全量 + C-1/2 → M2-F；M3-T2 → M3-C5 双跑；RFC 评审 + W-A1 → M4-W-B2；E-3 离线 diff 达标 → M5b-E4 转生产列；C1 语料 diff=0 → M6.3 切换。

### 3.3 全局台账（评审新增要求）

- **handler 总预算 <100 是全局闸门**（架构 §5）：config ≤54（M3）+ buff ≤8（M3）+ special ≤12（M5b）+ trigger（M4，预估 ≤8）+ mirage 2（M5a）+ tag/preflag 推断失败 ≤15（M6）≈ 99——已逼近上限。建议 M5b-C4 的闸门测试升级为**全 registry 单点断言**（按 id 前缀分域计数 + 总数断言），各阶段新增 handler 时同测试更新，避免各蓝图各自 <100 的错觉。
- **baseline 常量**：每阶段行为提升按"独立 bump commit"纪律顺序进行；并行期（M1∥M2、M5 三线）由先合并者 bump，后合并者 rebase 重跑后再 bump。

---

## 4. 跨阶段裁决摘要（本次评审定案，已回写各蓝图正文）

| # | 争议点 | 裁决 | 回写位置 |
|---|---|---|---|
| 1 | **DSL 三处方言风险**（M3 ConfigEffect / M5b ValueExpr / M6 template） | 同一套受限语言（架构 §5）：五算子+谓词求值器唯一实现 = `rules/value_expr.rs`，**M3-T1 起建**，M5b 复用并加 enums 闭集，M6 复用并加 `:cap`；schema 形态可异、求值器禁止三套 | m3 §4.4、m5b B-2、m6 §3/§11.2 |
| 2 | **16-G4 base_items 列落库归属**（roadmap 附 A 标 M1） | 消费阶段自带：block/spirit→M2-D，reload→M4-T4，flask/charm→M3-T4；M1 不做；例外 level_requirement→M1-T4（同表低成本） | m1 §0 范围澄清 + W0/T4 |
| 3 | **high_precision_mods.json 三处竞产**（M4/M5c/M6 各自兜底建表） | 初版已由 **M0-W4d** 落库（overlay/high_precision_mods.json，零接线）；**M4 W-A2** 负责接线与字段扩展；M5c-E2、M6 只消费 | m4 W-A2、m5c E2/§7.1、m6 §1.7/§14.1 |
| 4 | **local_mods.json 两处竞产**（M5c/M6） | 初版已由 **M0-W4d** 落库并接线（overlay/local_mods.json，is_weapon_local_mod 注入化）；**M5c WI-C1** 负责结构化局部结算扩展；M6 只消费 | m5c §7.1、m6 §1.7/§14.1 |
| 5 | **special_derived.json 两套生产工具**（M5b adapter vs M6 precompile-mods） | M5b-C1 先以 adapter 产 keystone 段；M6-T7 把生产**迁移**进 precompile-mods 并扩展 per-gem/skill_names 段，迁移 commit keystone 段 byte 等价，adapter 步骤退役 | m5b C-1、m6 §1.6 |
| 6 | **Keystone LIST 展开两套实现**（M3 keystone_merge vs M5b B-5 orchestrator 展开点） | 展开通道唯一 = M3 `calc/keystone_merge.rs` + `set_keystone_mods`；M5b-B5 收窄为解析面条目 + 端到端验证（2→1 人日） | m5b B-5 |
| 7 | **version-bump-drill.sh 两次"新建"**（M3 vs M6） | M3-T5-F 建第一版；M6-T10 在其上扩展（补 precompile 步），不新建 | m6 §8/T10 |
| 8 | **oracle parseMod 模式两次开发**（M5b D-1 vs M6 §5.3） | M5b-D1 建 `--mode parsemod`；M6 复用做字段增量，不另起模式名 | m5b D-1、m6 §5.3 |
| 9 | **unsupported 报表两套**（M5a-E2 vs M5b-A2，且同改 ninja_parity.rs） | owner = M5b-A2（corpus.rs）；M5a-E2 复用分类函数只加 minion 维度；ninja_parity.rs 按段分工（M5b-A 报表段 / M5a-E baseline 段） | m5a §4.2 表 |
| 10 | **granted_effects.minion_list 归属**（roadmap 附 A 标 M1 入库） | M1 明确不做；**M5a-A3 必做**（overlay 边车，非 .dat 列）；A0 检查降级为防御性核验 | m1 §0、m5a §1.3 |
| 11 | **M4-W-D1 引用已删除的 skill_stat_map.rs**（蓝图 bug） | M1-T2.4 已删该文件；dot 映射走 overlay/skill_stat_map.json + stat_map_engine，缺条目走数据通道补，禁止恢复 Rust 启发式 | m4 W-D1 |
| 12 | **spirit 族 M1/M2 重叠** | M1-T4.5 = 技能侧预留聚合（spirit_reserved）；M2 = 池本值/unreserved/efficiency；并行时 M1 先合并 | m1 T4.5、m2 Track D |
| 13 | SourceKind::Mirage 变体无人认领 | M5a-D1 追加（pobr-data/src/source.rs 单行，A 守门），沿 M3-T0 先例 | m5a D1 |

### 4.1 留给阶段 owner 的未决项（评审不代裁）

- M5a 开放问题 1（minions/spectres 物理层 base/ vs overlay/）与 M5b 开放问题 1（stat_descriptions 层归属）：同一类"生产工具定层 vs 逻辑 L1"分歧，建议总架构一次性裁决并在 20 文档 §3.1 加表注（两蓝图的 overlay 先行方案均可接受，不阻塞）。
- M2 开放问题 3（防御 ≥80% 的分母口径）与 M3 §9.2 顺延口径：阶段验收 reviewer 拍板。
- M5b enums / M6 `:cap` 两项 DSL 微扩展：均已按 ≥20 条目闸门论证，按各自蓝图走架构 review 流程确认。
- M3 Q1（buff_definitions 人工归纳偏离 P13）：需架构确认 overlay 通道例外。

---

## 5. 修正日志（本次评审对蓝图文件的直接修改）

| 文件 | 修改 |
|---|---|
| m1-skills-gems.md | W0 增 PlayerLevelReq 列；T4.1/T4.2 增 level_requirement；§0 增"范围澄清"段（16-G4/minion_list 归属修订）；T4.5 增与 M2 的 spirit 边界与并行序 |
| m2-defence.md | Track D survivability/skill_mechanics 段增与 M1-T4.5 的衔接与合并序 |
| m3-orchestration.md | §4.4 A4 增 `rules/value_expr.rs` 单一 DSL 实现裁决；§2.2 归属表 T1 行增 value_expr.rs |
| m4-offence-deep.md | W-D1 勘误（skill_stat_map.rs 已被 M1 删除，改走 overlay 表）；W-A2 标注 high_precision 唯一生产点 |
| m5a-minions.md | §1.3 minion_list/level_requirement 归属定案；E2 unsupported 报表改复用 M5b；D1 补 SourceKind::Mirage 归属 |
| m5b-special-statdesc.md | B-2 复用 value_expr.rs；B-5 改复用 M3 keystone_merge（2→1 人日）；C-1/D-1 增与 M6 衔接声明 |
| m5c-item-tree.md | E2 high_precision 改消费 M4 表；§7.1 开放问题改为已裁决 |
| m6-parser-rules.md | §1.7/§14.1 小查表归属定案；§1.6 special_derived 生产迁移声明；§5.3 复用 M5b parsemod 模式；§8/T10 drill 改扩展 M3 第一版；§3/§11.2 value_expr 来源修订 |

### 4.2 阶段 owner 裁决（2026-06-11，开工前生效）

1. **数据层归属（M5a-Q1 / M5b-Q1）**：采纳「生产工具定层」为唯一规则并视为对 20 文档 §3.1 的表注——adapter（.dat 可再生）→ base/，extract-lua / 人工策展（vendor 来源）→ overlay/。minions/spectres/stat_descriptions 当前生产路径是 vendor 抽取 → **overlay/ 先行**；后续若 pipeline 补齐 .dat 表则迁 base/（迁移 commit byte 等价）。
2. **M2 防御验收分母口径**：采纳 m2 蓝图 §6.3 双指标——「扩列后 defensive_rows ≥80% **且** 旧 8 列子集命中数 ≥111（不倒退）」。M3 的 ≥85% 若 M2 终点 <80%，按增量口径顺延（M3 至少 +5pp 且不倒退）。
3. **DSL 微扩展**：M5b enums（受限闭集映射）与 M6 `:cap` 算子均**批准**，前提不变：各自 ≥20 条目受益证明写入 PR 描述 + value_expr.rs 单点实现 + handler 全局台账 <100 断言（采纳 §4-13 警示，M5b-C4 升级为全 registry 分域断言）。
4. **buff_definitions.json P13 例外**：**批准**人工归纳 + vendor_ref 行号 + oracle 对拍 + 行段 hash drift 告警作为 overlay 通道的认可例外（doActorMisc 是过程代码无法 luajit 序列化）；例外范围仅此一表，新增例外需回到本节裁决。
