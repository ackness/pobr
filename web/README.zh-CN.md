# PoBR Web

[English](README.md) | **简体中文**

> **⚠️ 测试版。** 应用与底层 wasm/JSON API 仍在迭代中，可能随时变动或不稳定。

PoB2 风格的 PoBR Web 前端。**与计算引擎完全解耦**：只消费 `apps/pobr-wasm`
的 JSON 契约（`web/src/api/types.ts` ↔ `apps/pobr-wasm/src/build_api/`，
形状由 Rust 侧 `apps/pobr-wasm/tests/contract_golden.rs` 钉住），不 import Rust 类型、
不复刻任何公式。

## 多 BD 与阶段

通过“管理 BD”分别保存冰霜射击、旋风等不同玩法。复制当前阶段可规划开荒、过渡和毕业，
每个阶段独立保存加点、装备、技能、配置与笔记。切换时保留当前进度及尚未应用的装备草稿，
原有单份存档会自动迁移。

“分享此 BD · 全部阶段”会下载包含所有阶段与备注的 PoBR JSON 文件。
接收方从文件导入后获得一个独立 BD，不覆盖已有 BD。未应用的编辑草稿只留在本地与完整备份中。
PoB 分享码继续导出当前阶段及其导入的 PoB 配置组。“下载备份”和“备份全部 BD”都包含本机
全部 BD，恢复完整备份需确认替换。存档保存在当前浏览器，可通过文件分享或跨设备迁移；
如果浏览器保存失败，选择器旁会提示及时下载备份。

顶栏显示应用版本与实际加载的数据版本。提升页的全位置分析取消后可继续剩余位置；
仅修改预算、区服或赛季不会重新计算。离开提升页或修改 BD、目标、装备类别会清除临时分析结果。

## 快速开始

以下命令均在仓库根目录运行，使用 `web/package.json` 指定的 pnpm 版本；CI 使用 Node 22。已有可用工具链时跳过安装步骤。

```bash
# 一次性前置（仓库根目录）
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

pnpm --dir web install --frozen-lockfile
pnpm --dir web build-wasm    # wasm-pack 构建 pobr-wasm → src/wasm/pkg/（gitignored）
pnpm --dir web sync-data     # data/<version>/ JSON → public/data/（gitignored）
pnpm --dir web dev           # http://localhost:5173
```

无 wasm / 数据时可用 mock 后端独立开发 UI：

```bash
VITE_POBR_BACKEND=mock pnpm --dir web dev
```

mock fixture 由真实契约生成（契约变更后重跑并提交）：

```bash
cargo test -p pobr-wasm --test gen_fixtures -- --ignored
```

## 开发与验证

独立引擎下载、Browser/Node.js 接入、agent 技能、Release 附件及 CI 体积比较见
[WASM 打包说明](../docs/wasm-package.md)。

验证范围遵循根目录 [CLAUDE.md](../CLAUDE.md#验证分层本地提交默认定向验证)。
日常修改运行相关 Vitest 文件与 typecheck；交互变化补对应 Playwright spec。
WASM 产物缺失或其 Rust 源码、依赖、features、工具链变化时重建 WASM；源数据变化或同步数据缺失时
运行 `sync-data`。E2E 使用生产 dist，需要构建当前 Web 代码；`build` 已含 typecheck。

Web 经 `src/api/wasmBackend.ts` 在浏览器中调用 WASM，规划器复用计算结果。
`public/_worker.js` 是部署于 Cloudflare Pages 的 HTTP 适配层，Vite 本地复用它处理
WeGame / 市集接口；它不运行 Rust 计算。修改该文件需运行实际 workerd 测试。

## 命令

| 命令 | 说明 |
|------|------|
| `pnpm --dir web dev` | Vite dev server |
| `pnpm --dir web build` | tsc + 生产构建（dist/） |
| `pnpm --dir web typecheck` | 仅类型检查 |
| `pnpm --dir web test src/lib/mainSkill.test.ts` | 指定 Vitest 单元测试 |
| `pnpm --dir web test:worker` | Worker 的实际 workerd 运行时测试 |
| `pnpm --dir web exec playwright test e2e/build-roundtrip.spec.ts` | 指定 E2E spec；需要当前 dist 和已准备的 WASM/数据 |
| `pnpm --dir web build-wasm` | 重建 wasm 包 |
| `pnpm --dir web sync-data` | 重新同步游戏数据到 public/ |

## 结构

```
web/src/
├── api/          # 后端唯一入口：types.ts（契约）+ wasm/mock 双后端
├── hooks/        # useBuildSession（导入/重算/归因编排）
├── components/   # Features: shell/import/sidebar/items/skills/calcs/tree/config/trade/guidance/shared
├── lib/          # statDisplay / i18n / trade / optimize / annotations …
├── fixtures/     # mock 后端数据（gen_fixtures 生成）
└── styles/       # tokens.css（设计变量收口）+ global.css
```

## 语言支持

- **界面**：en-US / 繁中 / 简中三语（顶栏切换，UI 文案字典在 `src/lib/i18n.ts`）。
- **游戏名词**：zh-TW 与 zh-CN 边车均已入库（简中来自国服词典转录，
  `node pipeline/gen-zh-cn.mjs` 再生成）；宝石选择器支持简繁英搜索。
- **中文物品输入**：物品词条行与基底名可直接用简中（国服文本），
  wasm 侧模板反查翻译为英文 canonical 后进现有解析器；结构行（`Rarity:`）保持
  PoB 格式；未知中文行与未知英文行一样保留原文、报告 unsupported 诊断，其未建模效果不计入结果。繁中侧只有名词边车
  （词条行模板未入库）。

## 功能

- **新建 build**（PoB2 语义）：启动即有默认角色——Build 页切职业/升华/等级，
  Tree 页点选加点，全程实时重算，无需任何 code。
- **一键导入**：Build 页粘贴 PoB2 code 或 WeGame 分享链接，导入装备、技能和天赋；
  编辑态可导出回可分享的 PoB2 code。
- **装备**：人形槽位布局 + PoB 文本编辑、符文插槽、药剂/护符槽、
  物品库（对比/一键换装）、槽位备注。
- **技能**：技能组编辑（宝石/等级/品质/辅助）、主技能选择 + 实时伤害占比。
- **天赋树**：加点、属性三选一、珠宝插槽、节点搜索、节点威力热力图。
- **寻优与市集**：选择位置、目标和预算，自动识别物品类型，展示词条评分、贡献估计与市集权重。直接打开官方市集挑选，不必选择基底或导入商品 JSON。国服使用立即购买模式，点击市集商品的 Sum 可按权重排序。调整预算只更新链接，不重复计算。词条贡献是参考估计，不等于整件换装提升。

## 数据流

1. 启动：JS fetch `public/data/manifest.json` 列出的全部 JSON → `stageDataFile`
   注入 wasm → `initStagedData()` 构建内存 `GameData` / `BuildData`，后续计算读取内存数据。
2. 导入：PoB2 code 或 WeGame 分享数据 → 结构化 build（角色/树/装备文本/技能组/config）。WeGame 导入需运行 `pnpm --dir web dev` / `pnpm --dir web preview` 或部署随构建提供的 Pages worker，详见 [导入说明与限制](../docs/wegame-import.md)。
3. 计算：编辑态经 `useBuildSession` 组装请求 → `calculateBuildJson(request)` → display_catalog 全量键值 +
   unsupported 词条 + 聚合属性 breakdown。
4. 归因：`attributionJson(request)` → 逐来源「移除后重算」边际贡献
   （点击触发，计算量 = 1 + 来源数）。
5. 天赋树：`public/data/<version>/base/passive_tree.json` 静态加载（不经 wasm）。
