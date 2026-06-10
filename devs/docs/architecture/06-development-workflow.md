# 开发流程与工程策略

---

## 1. 基本策略

PoBR 采用 greenfield Rust 架构。目标是兼容 PoB 的数据格式、用户工作流和计算结果，同时避免复制 Lua 的共享可变状态、全局表和 UI/计算耦合。

核心原则：

- 先定义计算契约，再写实现。
- 机制不明确时先查 `agent-docs/` 本地资料库，再用 PoB-PoE2、官方 patch notes、PoE2 Wiki 或游戏数据交叉验证。
- 先跑通 Modifier/ModDB/最小计算闭环，再接入导入导出和 UI。
- 计算内部使用稳定 ID，显示文本通过 i18n。
- 每个 crate 有独立测试，跨 crate 行为用 golden fixtures。
- 性能优化以 benchmark 证据为准。

---

## 2. 推荐开发顺序

### 2.1 第一个可运行版本：计算内核

1. 建立 workspace：`crates/`, `apps/`, `tools/`, `fixtures/`。
2. `pobr-data`：定义计算需要的稳定 ID、Item、Gem、Skill、Modifier、GameData。
3. `pobr-core::modifier`：定义 `Modifier`, `ModTag`, `CalcConfig`。
4. `pobr-core::mod_db`：实现 `BASE`/`INC`/`MORE`/`FLAG`/`OVERRIDE`/`LIST` 聚合。
5. `pobr-core::mod_parser`：实现英文 PoB modifier parser/cache。
6. `pobr-core::calc`：实现属性、生命/魔力、抗性、简单 hit damage、DPS。
7. `apps/pobr-cli`：提供 `parse-mod`, `sum-mods`, `calc-fixture` 命令。

第一版 CLI 服务计算验证，它输出 JSON 和 breakdown，作为 CI 和 golden regression 的稳定入口。

### 2.1.1 机制资料查证流程

实现任何游戏机制前，先按以下顺序确认公式和术语：

1. 查 `agent-docs/` 中对应主题，例如 `resistances.md`、`damage-types.md`、`damage-defence-order.md`、`skill-speed.md`、`ailments.md`。
2. 对照 PoB-PoE2 计算实现：`CalcSetup.lua`、`CalcPerform.lua`、`CalcOffence.lua`、`CalcDefence.lua`、`CalcSections.lua`、`BuildDisplayStats.lua`、`ConfigOptions.lua`、`Data.lua`、`QuestRewards.lua`。
3. 对照官方 patch notes、PoE2 Wiki、游戏数据或 PoE2DB。
4. 若 `agent-docs/` 与官方/PoB-PoE2/数据源冲突，以可验证的一手或更接近游戏数据的来源为准。
5. 发现 `agent-docs/` 错误时直接修正文档，并在修改处保留来源说明。

`agent-docs/` 是开发输入资料，不是最终权威。计算实现和测试必须引用可交叉验证的结论。

### 2.2 输入来源版本

1. `pobr-item`：实现英文 raw item text 解析和 custom item draft。
2. `pobr-tree`：实现 allocated node modifier collection。
3. `pobr-build`：实现 Build 状态、PoB Build Code decode/encode、XML roundtrip。
4. `pobr-build::CalcOrchestrator`：封装计算入口和缓存。
5. golden regression：用完整 Build fixture 对比关键输出。

### 2.3 应用版本

1. CLI 稳定后做 `pobr-wasm`。
2. WASM API 稳定后做 `pobr-desktop`。
3. GUI 第一屏直接是可编辑 Build 工作区，支持计算预览、导入 BD、粘贴物品、创建自定义物品、切换语言。

---

## 3. Fixture 优先

目录约定：

```
fixtures/
├── pob-codes/
│   ├── poe1-simple.txt
│   └── poe1-minion.txt
├── raw-items/
│   ├── en-US/
│   └── zh-TW/
├── builds/
│   ├── xml/
│   └── snapshots/
└── i18n/
```

每个 fixture 需要记录：

- 来源：手工构造、PoB 导出、游戏复制文本、pobb.in。
- 游戏版本。
- 预期覆盖的功能点。
- 是否允许数值误差。

---

## 4. 测试分层

| 层级 | 示例命令 | 目标 |
|------|----------|------|
| 单元测试 | `cargo test -p pobr-core` | 类型、解析、聚合函数正确 |
| 兼容测试 | `cargo test -p pobr-build --test pob_code` | PoB code/XML roundtrip |
| fixture 测试 | `cargo test -p pobr-item --test raw_items` | raw item text 不回退 |
| i18n 测试 | `cargo run -p lint-i18n` | 语言包 key 和参数一致 |
| golden 测试 | `cargo test -p pobr-build --test golden` | 完整 build 输出对齐 |
| benchmark | `cargo bench -p pobr-core` | 热路径性能回归 |

---

## 5. CI Gate

每个 PR 至少跑：

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p lint-i18n
```

涉及计算、Modifier、item parser、Build Code、语言包的 PR 需要补对应 fixture 或 golden snapshot。

### 5.1 数据防线（M0 W4b 落地）

数据-框架分离的 CI 防线（对应 `audits/rearchitecture-2026-06-10/20-target-architecture.md` §4
的"三道 CI 防线"中的 ① + M0 任务清单中的禁内嵌 lint），当前落地两道：

**防线 ① 可再生性检查** — `devs/scripts/regen-check.sh`

- 用 `pipeline/` 本地输入重跑 `pobr-data-adapter`，产物与已提交的 `data/<patch>/` 逐文件
  byte-diff；任何漂移（内容不一致或重生成文件无对应已提交文件）退出码非零并列出差异文件。
- 两个域独立探测输入：`pipeline/tables/{English,Traditional Chinese}/`（物品/词缀/技能域）
  与 `pipeline/tree/data.json`（被动树域）；**本地缺输入时明确报 SKIP（退出码 0），不是失败**
  ——`pipeline/tables/` 等中间物已 gitignore，CI 环境默认没有。
- 布局兼容：已提交产物按存在性探测，优先 `data/<ver>/base/<文件>`（三层目录新布局），
  回退 `data/<ver>/<文件>`（旧平铺布局）。
- 已知排除：`manifest.json` 暂不参与 diff——当前 manifest 是 `--raw` / `--tree` 多步合并产物，
  单步重跑得到的域列表不完整；manifest v2（三段 domains）由单一步骤幂等生成后纳入。

**防线 ② 禁内嵌大数组 lint** — `cargo test -p pobr-data --test no_embedded_data`

- 扫描 `crates/pobr-data/src/` 全部 `.rs`：连续字面量元素行 > 200 的数组/常量表即失败
  （游戏数据必须走数据管线落 `data/<ver>/`，不得写死在框架代码里）。
- 按文件 allowlist 豁免存量内嵌表：`monster.rs` / `minion.rs` / `constants.rs`
  （M0 W2/W3 迁库对象）；**新文件不得加入 allowlist**。
- 实现为 pobr-data 的集成测试，随 `cargo test --workspace` 自动执行，无需单独 CI 步骤。

**M0 后收紧计划**：

1. W2/W3 把 monster/minion/constants 内嵌表迁入 `data/<ver>/` 后，allowlist 清空；
2. CI 提供（缓存的）pipeline 输入，防线 ① 由 SKIP 改为必跑；manifest v2 落地后把
   `manifest.json` 纳入 byte-diff；
3. 后续阶段补齐另两道防线：overlay drift（extract-lua 产物 vs vendor commit diff 报告）
   与 generated 一致性（precompile 重生 == 已提交）。
4. 仓库暂无 GitHub workflow；引入后把 `regen-check.sh` 接为独立 job（无输入即 SKIP），
   lint 已随 `cargo test` 覆盖。

---

## 6. 数据生成工具

`tools/` 只负责生成和验证，不参与运行时计算。

| 工具 | 职责 |
|------|------|
| `export-poe-data` | 从 PoB-PoE2/PoE 数据源转换为 PoBR 数据包 |
| `gen-mod-cache` | 生成 Rust 可读 Modifier cache，并与 fixture 对比 |
| `sync-pob-catalog` | 从 PoB-PoE2 核心 Lua 文件抽取 output/display/breakdown/quest reward catalog，并检查 PoBR parity matrix；`extract-lua` 子命令经 luajit + 最小 stub 环境执行 vendor Lua 数据文件，把人工策展层固化为 `data/<版本>/overlay/*.json`（确定性、可重跑，见工具 README） |
| `lint-i18n` | 检查语言包完整性、fallback、格式参数 |

生成产物必须可复现：同一输入数据和工具版本生成相同输出。

---

## 7. 性能策略

- 默认使用清晰数据结构。
- 热路径先写 benchmark，再优化。
- `unsafe` 只允许出现在独立模块，并需要 benchmark 与测试证明收益。
- 并行计算只在只读快照阶段展开，避免多线程写 `Env`。
- SIMD/SoA/cache 分桶放到后期性能阶段。

---

## 8. 文档维护

当实现改变 crate 边界、Build Code 兼容规则、语言包格式、fixture 目录或 CI 命令时，同步更新：

- `00-overview.md`
- `02-crate-design.md`
- `03-module-interfaces.md`
- `04-migration-roadmap.md`
- `05-compatibility-and-i18n.md`
- `06-development-workflow.md`
- `11-implementation-progress.md`

文档以可执行契约为主，避免只描述愿景。

每个实现步骤结束前必须更新 `11-implementation-progress.md` 的勾选项，说明本轮完成了什么、还剩什么。该文件记录真实工程状态，不能用路线图愿景替代。
