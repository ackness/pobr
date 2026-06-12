# version-bump-drill 第一次演练发现项（M3 T5-F，P18）

- 演练日期：2026-06-12
- 脚本：`devs/scripts/version-bump-drill.sh`（第一版，蓝图 m3-orchestration.md §8.3）
- 形态：无新版本 → **当前版本（4.5.0.3.4）输入重放** + 人工模拟一处数值变更
- 输入：`pipeline/tables`（主仓库本地快照）+ vendor `PathOfBuilding-PoE2`
  （commit `2df5a74`）；`pipeline/tree/data.json` 本地缺失（树域 SKIP）
- 结果：overlay 域 17/17 可执行抽取全部 byte-diff=0；base 域 4 文件漂移（见 F1）；
  `cargo build --workspace` 零改动编译通过；ninja_parity 套件可运行且通过

> 登记口径（蓝图 §8.3）：「必须改 Rust 才能吸收」的每项一条 → 转入 M5/M6 数据化
> 清单；纯数据/流程项一并登记备查，不要求本阶段清零。

## 「必须改 Rust 才能吸收」清单（→ M5/M6）

### F1 `_meta.regen_command` 内嵌调用方 `--out` 入参，重放校验被迫「就地写+快照还原」

- 现象：`sync-pob-catalog` 各 extract 把**实际传入的** `--out` 路径写进产物
  `_meta.regen_command`（如 `extract_curse_priority.rs::build_meta`，vendor-root 按约定
  写 canonical 相对路径，但 out 不是）。把产物重放到临时路径必然产生 `_meta` 自指
  差异（假阳性），drill 脚本只能对已提交文件就地覆写、byte-diff 后还原。
- 吸收方式：改 `tools/sync-pob-catalog` 各 extract 的 `build_meta`——`regen_command`
  的 `--out` 统一写 canonical 仓库相对路径（与 vendor-root 同约定），与实际入参解耦。
  一次性 Rust 工具改动 + 全部 overlay 重生成（搬迁 commit，byte-diff 仅 `_meta`）。
- 模拟变更实测佐证：vendor `Data.lua` cursePriority `Despair 8→88` → 重抽取产物与
  已提交 overlay 的差异 = `curse_base.Despair` 一处数值 + `_meta.regen_command`
  （后者纯因 out 路径不同）——数据变更本身被 JSON 完整吸收，Rust 零改动。
- **状态：已吸收**（M3-W3，commit `017b24b`）。`sync-pob-catalog` 在
  `out_for_meta` 赋值处统一归一化（`extract_lua::canonical_out_for_meta`）：
  out 路径含 `data/` 组件时截取最后一个 `data/...` 相对段；不含时按 `--what`
  目标的 canonical 默认路径表 `data/<version>/overlay/<file>` 回退（version 从
  out / `--version-file` 路径组件推导，推不出时省略 `--out`）。全部 17 个可执行
  extract 重生成核验 **byte-diff=0**（已提交 `--out` 本就是 canonical 相对路径，
  无需搬迁 commit）；临时路径重放（保留 `data/<ver>/overlay/<file>` 尾段结构）
  实测含 `_meta` 逐字节一致。drill 脚本步骤 3 的「就地覆写+快照还原」hack 已
  简化为临时路径重放直接 byte-diff（commit `b80f829`）。
  附注：`mod_parser_rules` 的 headless 引导以 vendor src 为 cwd，重放须用
  **绝对** `--vendor-root`（drill 脚本本就重写为绝对路径，不受影响）。

### F2 adapter 对输入表缺列静默降级，无必需列断言（F3 的 Rust 侧加固项）

- 现象：`pobr-data-adapter` 对 `.dat` 导出缺列走 serde `Option` 默认（如
  `ArmourTypes` 缺 `IncreasedMovementSpeed` 列 → `movement_penalty` 整列静默缺失），
  产物结构合法但语义降级，只能靠 byte-diff 事后发现。
- 吸收方式：adapter 入口对各表做**必需列存在性断言**（清单随版本配置），缺列即
  fail-fast 报表名+列名，而不是产出降级 JSON。属 adapter（Rust 工具层）改动。
- **状态：已吸收**（M3-W3，commit `05d1d88`）。
  `pobr-data-adapter::required_columns` 在 `--raw` 适配开始前对全部消费表
  （EN 14 张 + 繁中边车 3 张）核对首行列键（清单 = 各 `Raw*` 行结构 serde
  rename 全集，含 `ArmourTypes.IncreasedMovementSpeed`、
  `GrantedEffects.AdditionalStatSets` 等 Option/default 列），缺列跨表汇总报
  「表名 + 列名」；表文件缺失立即报错（覆盖 F8 快照整目录缺失场景）。断言逻辑
  由构造缺列最小 JSON 的单测覆盖，不依赖本地 `pipeline/tables/` 快照。

## 数据 / 流程发现项（备查，不计入 Rust 清单）

### F3 `pipeline/tables` 本地快照漂移 → base 域 4 文件重放 byte-diff≠0

- 漂移文件：`base_items.json` / `granted_effects.json` / `granted_effect_levels.json`
  / `granted_effect_stat_sets.json`。
- 根因：本地表快照**旧于**已提交产物的生成输入——快照 `ArmourTypes.json` 只有
  `[Armour, Evasion, EnergyShield, Ward]` 列（缺 `IncreasedMovementSpeed` →
  `movement_penalty` 缺失 512 条）；`GrantedEffects` 缺 `AdditionalStatSets` 列
  （→ `additional_stat_set_ids` 缺失 540 条）；`granted_effect_stat_sets` 较已提交
  少 6 个 id（`HeraldOfBloodPlayer`/`HeraldOfIcePlayer`/`AtziriElementalSpearCombo`
  等，对应 GrantedEffectStatSets 重下事件）；`granted_effect_levels` 4212 条
  共同键差异（同根因连带）。
- 处理：按 `pipeline/README.md` 第 2 步重下表快照后 regen-check/drill 应恢复零
  diff；**不需要改 Rust**。连带流程项：drill 步骤 1 的「下载校验占位」要升级为
  表清单 + 列级 schema 校验（pipeline 下载脚本侧），否则快照漂移只能在步骤 2 的
  byte-diff 才暴露。

### F4 人工策展域无自动重放通道（版本 bump 盲区）

- `buff_definitions.json`（对账命令 `check-buff-refs`，人工复核 `--write` 刷新行段
  hash）与 `special_mods.json`（对账命令 M5b A-3 后启用）有约定但不在 drill 自动
  路径；`high_precision_mods.json` / `local_mods.json` 的 `_meta` 无 regen/对账命令
  字段（仅 source/notes）。版本 bump 时这四个域靠人工记忆。
- 处理：给后两者补 `_meta` 对账说明；drill 后续版把「策展域对账命令」也纳入
  可执行步骤（buff_definitions 的 check-buff-refs 已可机跑）。
- **状态：已吸收**（M3-W3，commit `b80f829`）。
  `high_precision_mods.json` / `local_mods.json` 的 `_meta` 已补 `audit` 字段
  （版本 bump 人工对账口径：准源 / vendor 对照点 / 复核后须更新的字段；loader
  对 `_meta` 不反序列化，无需 schema 扩字段）；drill 脚本新增步骤 3b——
  `check-buff-refs` 对账 `buff_definitions.json`（漂移即 FAIL）。
  `special_mods.json` 的对账命令待 M5b A-3 落地后接入（既有登记不变）。

### F5 树域输入不在本地快照

- `pipeline/tree/data.json` 缺失 → 树域重放 SKIP。版本 bump 演练输入清单需包含
  树导出（与 F3 的清单校验合并处理）。

### F6 vendor 副本抽取依赖 `.pob2-version.txt` 约定路径

- extract 默认读 `<vendor_root>/../../.pob2-version.txt`；对 vendor **副本**（模拟
  变更场景）需显式 `--version-file`。drill 脚本当前用真 vendor 检出无此问题；
  文档化即可。
- **状态：已吸收**（M3-W3，commit `b80f829`）。drill 脚本头注 / 用法已
  说明 vendor 副本场景须显式 `--version-file`，并新增 `--version-file` 透传
  参数（重放时注入 / 覆写各 regen 命令），模拟变更演练不再需要手工逐条改命令。

### F7 precompile 步骤占位

- parser 规则 precompile（M6）未落地，drill 步骤 4 恒 SKIP；M6 落地后接入。

### F8 flask/charm 基底数据列（adapter 增列）被输入快照缺失阻塞（M3-T4 D2 / Q4 侦察结论）

- **Q4 结论（charm 基底可得性）**：PoE2 charm 在 `.dat` **没有独立 item_class**——
  `base_items.json` 实查 13 个 charm 基底（Thawing/Staunching/Ruby/… Charm）均为
  `item_class = "UtilityFlask"`、基底 id `Metadata/Items/Flasks/FourCharm*`；
  LifeFlask×9 / ManaFlask×9 另列。**无需补 ItemClasses 表**，charm/flask 判别走
  基底名（已在 `pobr-core::item::classify_utility_item` 落地）。
- **阻塞**：`flask{duration, charges, buff_stats[]}/charm{...}` 数据列的真源是
  `Flasks` `.dat` 表（含 BuffDefinition/BuffStatValues 外键），该表不在
  `pipeline/config.json` tables 清单；本地 `pipeline/tables/` 快照**整目录缺失**
  （F3 漂移的恶化形态），且 CDN 已下线 4.5.0.3.4（同 `_tablesUnavailableForPinnedPatch`
  注记）→ adapter 无法重跑，**任何 base_items.json 增列都无法按「重跑 byte-diff
  仅新列增量」纪律产出**。按蓝图 Q4「缺口登记不硬造」处理：本波 adapter 不增列。
- **现状口径**：charm/flask **物品文本词条**已进计算（item.rs flask 分支 →
  FlaskBuff/CharmBuff 载荷 → env_finalize 阶段 3 merge，mode_combat 门控）；缺的是
  **基底常驻 buff**（vendor `item.base.charm.buff`，如 Ruby Charm
  `+25% to Fire Resistance`、Thawing Charm `Immune to Freeze`——PoB2 XML 物品文本
  不含这些行）与 duration/charges（M3 范围声明本就不建模）。
- **吸收路径（登记 M5/M6 数据化清单）**：
  1. 版本升级重下时把 `Flasks`（及其 BuffDefinitions 关联表）加入
     `pipeline/config.json` tables，adapter 增列 `flask{}/charm{}`（L1 搬迁）；
  2. 在此之前如需 charm 基底 buff，走 extract-lua 兜底通道（vendor
     `Data/Bases/flask.lua` 的 `charm{duration, chargesUsed, chargesMax, buff[]}` /
     `flask{...}` 字段，与 GemEffects 等同例）→ `overlay/flask_bases.json`。

## 模拟数值变更演练记录（蓝图 §8.3「人工模拟一处数值变更」）

1. 复制 vendor 检出 → 临时副本，改 `src/Modules/Data.lua:282`
   `["Despair"] = 8` → `88`（aura/curse 域数值）。
2. `sync-pob-catalog extract-lua --what curse-priority --vendor-root <副本>/src
   --version-file <真 vendor>/.pob2-version.txt --out <临时>`。
3. 与已提交 `overlay/curse_priority.json` 对比：数据差异**仅** `curse_base.Despair`
   一处；`_meta` 差异仅 `regen_command`（F1 的 out 路径自指）。
4. 结论：vendor 数值变更由 overlay JSON 完整吸收，Rust 零改动；build + ninja_parity
   可跑（drill 步骤 5/5b 通过）。
