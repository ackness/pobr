# PoBR Web

PoB2 风格的 PoBR Web 前端。**与计算引擎完全解耦**：只消费 `apps/pobr-wasm`
的 JSON 契约（`web/src/api/types.ts` ↔ `apps/pobr-wasm/src/build_api.rs`，
形状由 Rust 侧 `tests/contract_golden.rs` 钉住），不 import Rust 类型、
不复刻任何公式。

## 快速开始

```bash
# 一次性前置（仓库根目录）
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

cd web
npm install
npm run build-wasm    # wasm-pack 构建 pobr-wasm → src/wasm/pkg/（gitignored）
npm run sync-data     # data/<version>/ JSON → public/data/（gitignored）
npm run dev           # http://localhost:5173
```

无 wasm / 数据时可用 mock 后端独立开发 UI：

```bash
VITE_POBR_BACKEND=mock npm run dev
```

mock fixture 由真实契约生成（契约变更后重跑并提交）：

```bash
cargo test -p pobr-wasm --test gen_fixtures -- --ignored
```

## 命令

| 命令 | 说明 |
|------|------|
| `npm run dev` | Vite dev server |
| `npm run build` | tsc + 生产构建（dist/） |
| `npm run typecheck` | 仅类型检查 |
| `npm run test` | vitest 单元测试 |
| `npx playwright test` | E2E 冒烟（先 build-wasm + sync-data + build） |
| `npm run build-wasm` | 重建 wasm 包 |
| `npm run sync-data` | 重新同步游戏数据到 public/ |

## 结构

```
web/src/
├── api/          # 后端唯一入口：types.ts（契约）+ wasm/mock 双后端
├── hooks/        # useBuildSession（导入/重算/归因编排）
├── components/   # 按 feature 分目录：shell/import/sidebar/items/skills/calcs/tree/config
├── lib/          # statDisplay（侧边栏字段目录：分组/双语标签/格式/着色）
├── fixtures/     # mock 后端数据（gen_fixtures 生成）
└── styles/       # tokens.css（设计变量收口）+ global.css
```

## 数据流

1. 启动：JS fetch `public/data/manifest.json` 列出的全部 JSON → `stageDataFile`
   注入 wasm → `initStagedData()` 构建 `BuildData`（一次，之后零 I/O）。
2. 导入：`decodeBuildJson(code)` → 结构化 build（角色/树/装备文本/技能组/config）。
3. 计算：`calculateBuildJson({pob_code, ...覆盖})` → display_catalog 全量键值 +
   unsupported 词条 + 聚合属性 breakdown。
4. 归因：`attributionJson({pob_code, fields})` → 逐来源「移除后重算」边际贡献
   （点击触发，计算量 = 1 + 来源数）。
5. 天赋树：`public/data/<version>/base/passive_tree.json` 静态加载（不经 wasm）。
