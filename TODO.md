# PoBR Web 可视化 TODO

> 目标：为 PoBR 做一个 Web 前端（参考 PoB2 的 UI 布局），**前端与后端计算逻辑完全分离**。
> 分离原则：前端只消费 `apps/pobr-wasm` 暴露的 **JSON 契约**（wasm-bindgen 或纯 JSON 字符串），
> 不 import 任何 Rust 类型、不复刻任何计算公式；所有数值（含 breakdown/归因）都由后端算好、
> 前端只做展示。契约变更 = 后端改 `pobr-wasm` 信封 + 前端改 TypeScript 类型，两侧各自演进。
>
> 跟进方式：完成一项就把 `[ ]` 改成 `[x]`，并在行尾追加 PR 号（如 `(PR#NN)`）。

## Phase 0 — JSON 契约扩展（后端侧，前端的唯一依赖面）

现状：`pobr-wasm` 只有 `calculate_json`（MinimalInput 标量 + modifier 文本）和 `translate`。
前端要还原 PoB2 体验，需要在 `pobr-wasm` 增加以下 JSON 入口（全部复用现有 crate 能力，零新计算逻辑）：

- [x] 0.1 `decode_build_json(pob_code) -> BuildJson`：包装 `pobr-build::decode_pob_code`，
      返回结构化 build（角色/职业/升华、装备文本块、技能组、天赋节点 id 列表、config） (PR#56)
- [x] 0.2 `calculate_build_json(BuildJson) -> OutputJson`：包装 `CalcOrchestrator`，
      返回 `extract_display_values(&OutputTable)`（display_catalog 强类型字段目录）的全量键值 (PR#56)
- [x] 0.3 输出中附带 breakdown：每个展示字段可选携带 `(base + Σinc + Πmore)` 分解与来源列表
      （TraceGraph → 序列化为扁平 JSON，前端只渲染） (PR#56)
- [x] 0.4 `attribution_json(...)`：暴露 `attribute()` / `AttributionReport::direct()`，
      支撑「这条词条贡献了多少 DPS」面板（PoBR 相对 PoB 的差异化卖点） (PR#56)
- [x] 0.5 天赋树静态数据导出：节点坐标/连线/图标 key 的 JSON（一次性从 `pobr-tree` + data/ 导出，
      前端作为静态资产加载，不在运行时经过 wasm） (PR#56，passive_tree.json 已含坐标/连线，经 `npm run sync-data` 作为静态资产直出)
- [x] 0.6 契约冻结：`web/src/api/types.ts` 手写 TS 类型 + 一个 Rust 侧 golden 测试钉住 JSON 形状
      （schema 变更必须同时改两处，测试挂 = 契约破坏） (PR#56)

## Phase 1 — 前端脚手架

- [x] 1.1 `web/` 目录：Vite + React + TypeScript，**不进 cargo workspace**，独立 `package.json` (PR#56)
- [x] 1.2 wasm 构建接线：`wasm-pack build apps/pobr-wasm --features wasm` → `web/` 引用产物；
      写进 `web/README.md` 的一条构建命令 (PR#56)
- [x] 1.3 API 层唯一入口：`web/src/api/`（wasm 调用 + JSON 解析全部收口在这里，
      组件层看不到 wasm），预留同签名的 mock 后端（fixture JSON）供 UI 独立开发/测试 (PR#56)
- [x] 1.4 设计基线：暗色主题 tokens（参考 PoB2 配色：深底/金色高亮/职业色），CSS 变量收口在
      `web/src/styles/tokens.css` (PR#56)

## Phase 2 — Build 导入 + 侧边栏总览（第一个可用切片）

- [x] 2.1 「Import Build Code」输入框：粘贴 PoB2 code → `decode_build_json` → 展示角色概要 (PR#56)
- [x] 2.2 左侧常驻 stat 侧边栏（PoB2 式）：Life/Mana/ES、三抗、护甲/闪避、DPS 等
      display_catalog 字段分组展示；数值直接来自 0.2 的输出 (PR#56)
- [x] 2.3 unsupported_modifiers 提示区（后端返回什么就列什么） (PR#56)
- [x] 2.4 用 `examples/demo-bd-test/builds/*` 的真实 build 做 E2E 冒烟（Playwright，1 条主流程） (PR#56)

## Phase 3 — 页签框架 + Items / Skills 面板（只读）

- [x] 3.1 PoB2 式顶部页签：Tree / Skills / Items / Calcs / Config / Notes（先全部占位） (PR#56)
- [x] 3.2 Items 页：装备槽位网格 + 词条文本展示（原始文本块直出，稀有度着色） (PR#56)
- [x] 3.3 Skills 页：技能组列表（主技能 + 辅助宝石、等级/品质） (PR#56)
- [x] 3.4 主技能切换 → 重算 → 侧边栏数值刷新 (PR#56)

## Phase 4 — Calcs 页（breakdown 可视化，PoBR 差异化）

- [x] 4.1 字段点击 → 展开 breakdown：base/inc/more 分解 + 逐来源贡献列表（消费 0.3） (PR#56)
- [x] 4.2 归因视图：按装备/天赋/宝石/配置分组的贡献占比（消费 0.4） (PR#56)
- [ ] 4.3 与 PoB2 Calcs 页对照走查一轮，记录展示口径差异

## Phase 5 — 天赋树查看器（只读，工作量最大，放最后）

- [x] 5.1 Canvas/SVG 渲染节点 + 连线（消费 0.5 静态数据），已加点高亮 (PR#56)
- [x] 5.2 缩放/平移 + 节点 hover 显示词条 (PR#56)
- [x] 5.3 （可选，二期）交互加点 → 重算 (PR#56，树上点选加点/取消即时重算；白手起 build 同步落地)

## Phase 6 — Config 页 + i18n + 收尾

- [x] 6.1 Config 页：mode_combat / mode_buffs / 敌人参数等开关 → 重算 (PR#56)
- [x] 6.2 i18n：接 `translate(lang, key)`，en-US / zh-TW 切换 (PR#56，UI 词条目录双语切换；`translate` 已在 API 层暴露)
- [x] 6.3 视觉走查：320/768/1024/1440 断点截图、键盘导航、reduced-motion (PR#56，Playwright 断点截图 + 无横向溢出断言 + 键盘焦点冒烟)
- [x] 6.4 前端 CI：`web/` 独立 job（tsc + eslint + vitest + build），不阻塞 Rust gate (PR#56)

## 明确不做（YAGNI，需要时再开新条目）

- 后端 HTTP 服务（wasm 内嵌足够；Trade 联网等真需要时再说）
- 装备/天赋的编辑器（先只读展示，编辑是独立大项目）
- 桌面壳（`apps/pobr-desktop` 骨架保持现状）
