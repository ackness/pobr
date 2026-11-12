<a id="pobr-wasm-package"></a>

# PoBR WASM 独立包

GitHub Release 的 `pobr-wasm-v<version>-web.tar.gz` 包含计算引擎、ES module 绑定、
TypeScript 声明、JSON 契约类型、匹配的游戏数据、可执行示例、`use-pobr-wasm` skill 和
PoBR 许可证。运行它不需要 PoBR 网页、Cloudflare Worker、Lua 或 Rust。

<a id="download-and-run"></a>

## 下载与运行

从选定的 [GitHub Release](https://github.com/ackness/pobr/releases) 下载归档和
`SHA256SUMS`，解压后保留目录结构。需要 Node.js 22 或更新版本：

```bash
tar -xzf pobr-wasm-v<version>-web.tar.gz
cd pobr-wasm-v<version>-web
node example.mjs
# Or calculate a UTF-8 file containing a PoB2 build code:
node example.mjs --build-code build.txt
```

默认示例计算一个 90 级 Warrior，显式关闭默认的 5% 生命任务奖励，并验证合成词条
`+100 to maximum Life` 恰好增加 100 生命。输出包括包、数据和 schema 版本、生命值和
诊断数量。导入构筑码时，示例检查生命值为正，并报告未支持词条和物品错误。

`manifest.json` 记录包版本、编译时源码提交及 dirty 状态、编译输入指纹、工具版本、
已编译的 JSON schema 版本、数据版本，以及运行文件的大小和 SHA-256。
归档旁的 `SHA256SUMS` 覆盖发布资产。
API 仍是 beta：调用方应固定引擎与数据配对，并用 `schemaVersion()` 检查支持的 JSON 格式。
schema 编号没变，不代表游戏计算没变，也不代表已覆盖 PoB2 的所有机制。

<a id="agent-skill-and-browsernodejs-api"></a>

## Agent skill 与浏览器 / Node.js API

归档含 `skills/use-pobr-wasm/`。把整个目录复制到 agent 的 skill 目录后，调用
`$use-pobr-wasm`。安装后仍可用；运行其中示例时通过 `--package` 指定解压的发布目录。

归档中的 `skills/use-pobr-wasm/references/api.md` 说明浏览器 / Web Worker 用法、
初始化顺序、JSON 示例、错误处理和限制。仓库原件是
[SKILL.md](https://github.com/ackness/pobr/blob/master/.agents/skills/use-pobr-wasm/SKILL.md)
和 [API 指南](https://github.com/ackness/pobr/blob/master/.agents/skills/use-pobr-wasm/references/api.md)。
生成的 `pobr_wasm.d.ts` 描述 JS 导出；`api-types.ts` 提供 JSON 请求和响应类型，
其依赖的 `generated/request.ts` 和 `generated/dataInfo.ts` 也随包附带。类型使用 `import type` 引入。
完整计算响应的 `provenance` 包括引擎版本、数据版本和有效数据包 SHA-256；
报告数值差异时，应把它与计算输入一起保留。

<a id="build-and-release-maintenance"></a>

## 构建与发布维护

在 checkout 中通过 `mise run setup` 准备仓库指定的工具，然后运行：

```bash
mise run web build-wasm
mise run web sync-data
mise run web package-wasm
mise run web smoke-wasm-package
```

输出在 `.cache/wasm-release/`。打包复用已有 WASM 和完整 Web 数据清单，
包含共享 overlay 与 i18n，不包含 UI、图片、玩家示例构筑和维护审计报告。

`build-wasm` 选择 `wasm-release` profile：release 优化、ThinLTO、一个 codegen unit，
不改变 dev/test 设置。

构建前检查源码指纹、工具版本、相关环境与 Cargo 配置、生成绑定哈希，以及编译出的 schema。
输入和产物未变就复用现有包及其构建记录，不调用 wasm-pack。
`mise run web build-wasm --force` 强制通过 Cargo 重建，但不清缓存。
记录缺失或格式过旧时，正常重建一次；只有构建成功且期间输入未变，才写入新记录。

打包再次核对 Rust 源码（含编译进引擎的 `*_config_options.json`）、manifest、lockfile、
构建配置和 JSON 类型，检查生成绑定哈希，并从已编译引擎读取 schema。
它还比较同步数据与当前快照、patch、共享层的字节，包含同版本数据修改。
过期时需先重建或同步；打包本身不构建、不下载。
直接运行 wasm-pack 而没有构建记录的产物无法打包。
之后无关的提交不会改变二进制来源：发布 manifest 保留编译时的 commit 和 dirty 标记。
缓存统计与 dev/test 规则见[开发工作流](https://github.com/ackness/pobr/blob/master/docs/development-workflow.md)。
旧的版本归档保留到显式删除，打包不会清理发布历史。

CI 会测试打包脚本、构建 WASM、打包，再解压到临时目录，计算新角色和已有构筑。
手动触发会保存 `wasm-release` Actions artifact 和大小报告。

组包后的 runtime 候选另用以下窄检查，输入为同次 CI 的 standalone artifact 目录和
`pipeline/runtime/runtime_pack.py materialize` 的新输出根（含 `bundle.json`、`data/CURRENT`）：

```bash
node web/scripts/smoke-runtime-candidate.mjs --package <wasm-release-dir> --data <materialized-root> --expected-runtime <release.runtime_identity> --out <fresh-receipt.json>
```

四个参数必填，输出父目录须已存在。脚本核对包归档、JS/WASM 摘要及编译 schema，
复用 `snapshotFiles` 映射候选，逐次核对计算 provenance；不会解出或加载包内旧数据 gzip。
它运行三份现有 worked build、既有 `+100 Life` 编辑检查，并用明确 synthetic 的未支持
词条哨兵核对原文与保存重开的诊断保留：`extra_modifiers` 直接返回原文，保存到 custom block
后重开带 `Config:` 来源；除此来源转换外，未支持／物品诊断逐项相等。
非空诊断不使检查失败。路径穿越、symlink、错误摘要、输入在运行中变化或输出已存在均拒绝；
全部检查通过后才写 `pobr-runtime-candidate-smoke/v1` 收据，记录代码／包 pins、候选身份、
staging 清单、请求摘要和结果／诊断。收据不证明数值准确率，也不单独认证 artifact 来自同次 CI；
同 run 下载与候选 `verify/materialize` 的接线由 CI 调用方保证。聚焦测试中的微型 WASM 只导出
schema，计算器为明确标记的 synthetic fixture，不能替代真实候选消费证据。

推送 `v*` tag 时，由 [release.yml](../.github/workflows/release.yml) 校验发布身份，
并在完整 Rust、Web、自动化检查都通过后附加 Release 资产；
Release 不存在时创建，已有说明则保留。tag 必须与 Cargo workspace 版本一致。
带 `-rc.1` 等版本后缀的 tag 创建 prerelease，不发布 npm 包。

`wasm-size.json` / `wasm-size.md` 记录原始字节数、gzip level 9 估算、实际归档大小，
以及按哈希固定的 pob-web 实测参考。
`devs/ci/pob-web-wasm-size-baseline.json` 保存获取与测量日期、原始和 gzip 字节、
产物哈希、压缩运行时。CI 读取这份已提交快照，不下载会变化的上游产物。
参考部署的二进制与数据源码版本未知；包装层 / shims 的提交不能替代产物来源版本。

获取并测量参考包，再打包、运行 PoBR：

```bash
mise exec -- node web/scripts/benchmark-pob-web.mjs --prepare
mise exec -- node web/scripts/measure-pob-web-size.mjs --output devs/ci/pob-web-wasm-size-baseline.json
mise run web package-wasm
mise run web smoke-wasm-package
mise exec -- node --test web/scripts/measure-pob-web-size.test.mjs web/scripts/package-wasm.test.mjs
```

参考数据包只压缩一次，使用 gzip level 9；传输估算另压缩 WASM 和 JS，不重复压缩数据 gzip。
PoBR 完整发布归档还含类型、文档、示例和 skill；参考快照没有同等发布归档，
因此只比较其运行组件，不能虚构一个参考归档大小。

[pob-web 的 Lua 打包脚本](https://github.com/krauthaufen/pob-web/blob/master/packages/pob-data/bundle-lua.mjs)
把 PoB Lua 源码、游戏数据和运行库一起打包；其
[WASM 构建](https://github.com/krauthaufen/pob-web/blob/master/packages/lua-wasm/scripts/build.mjs)
编译的是 Lua 解释器。PoBR 则把计算代码直接编译到 WASM。
报告比较 WASM + JS + 压缩数据，并另算全 gzip 传输估算。
数据内容、机制支持和压缩方式不同，大小对比或打包冒烟测试不能证明数值一致。
这里测的是磁盘与传输大小，不是运行时内存。

<a id="runtime-performance-comparison"></a>

## 运行性能对照

性能测量是按需实验，不是每次提交的门禁。先用 `mise run setup` 准备开发环境，
构建 WASM 并同步数据；报告写入忽略的 `.cache/`，不要把一次主机测量当成当前版本的性能承诺。

`mise exec -- node web/scripts/benchmark-pob-web.mjs --prepare` 获取本地参考快照，保留上游许可证，
固定包装层、shims 和加载补丁，并校验哈希。部署产物的二进制 / 数据源码版本未知。
后续准备只离线校验原快照；测新部署应使用新目录。

构建 WASM、同步数据后，在仓库根运行：
`mise exec -- node web/scripts/bench-pob-web.mjs --iterations 9 --warmup 2`。
完整结果写入 `.cache/pob-web-performance.json`；失败案例保留错误、不报耗时比，
并使退出码为 1。加 `--pobr-only` 可不依赖参考快照，单独测量全部 PoBR 构筑，
结果写入 `.cache/pobr-performance.json`，不提供跨引擎耗时比。

脚本显式选择共享 parity 语料中的七个历史构筑，并发现
`examples/wasm-benchmark/builds/` 的全部样本。
历史性能组曾排除失败或不稳定的构筑，它们仍在 parity 语料中。
该性能组经过结果筛选，不能当作无偏样本。
新 poe.ninja 抓取保留真实赛季、快照、角色 URL、获取日期和构筑码哈希，不自动变成 parity golden。
样本来源见各构筑目录的 `meta.json`。

基准报告生成后，用 `mise exec -- node web/scripts/snapshot-pob-web-benchmark.mjs --output <file>`
保存到 `.cache/` 下的精简记录，保留原始采样和诊断、去掉完整分解响应。
`--output` 必填，避免默认覆盖旧记录。脚本回归检查：

```bash
mise exec -- node --test web/scripts/benchmark-comparison.test.mjs web/scripts/benchmark-builds.test.mjs web/scripts/benchmark-pob-web.test.mjs
```

Lua 计时包含 Node Worker 宿主、适配检查和上游无界面 UI 状态准备。
两侧实现、JSON 响应和支持机制不同。这些测试不能推导纯计算内核速度、增量编辑延迟、
浏览器表现、内存占用或数值一致性。

<a id="web-data-load-comparison"></a>

### Web 数据加载：逐文件快照与紧凑包

`web/scripts/bench-web-load.mjs` 在浏览器里对比同一份构建站点的两种数据目录：
`sync-data` 发布的逐文件快照（当前生产方式），以及经 `syncRelease` 发布的紧凑发行包
（`POBR_RUNTIME_PACK`，三个 block）。每次运行启动全新的无头 Chromium（无 HTTP 与代码缓存），
打开没有存档的 `/build`，记录启动屏消失、默认角色第一次计算结果出现，再改一次等级记录第二次计算。
本地服务器发送 brotli（quality 5）预压缩响应和与托管相同的缓存头；网络档位是一条共享的
FIFO 链路（9 Mbit/s、每个响应 150 ms 延迟，近似 DevTools 的 Fast 4G），CPU 档位是 DevTools
的 4 倍降速，同时作用于页面与计算 Worker（第二次计算 9 ms → 39 ms 证实 Worker 也被降速）。
内存是页面与 Worker 所在 renderer 进程的 PSS 合计（含 JS 堆和 WASM 内存）。

```bash
mise run web build-wasm && mise run web sync-data && pnpm --dir web exec vite build
python3 -m pipeline.runtime.runtime_pack build --data data --out .cache/runtime-pack
node web/scripts/bench-web-load.mjs --pack .cache/runtime-pack/player --iterations 7 [--profiles host,4g]
```

内存读取 `/proc`，只适用于 Linux 主机。

报告写入 `.cache/web-load-performance.json`，保留每次采样。下表是 2026-11-05 在开发主机上
（AMD Ryzen 9 7950X3D，32 线程，124 GB，Playwright Chromium 1248，数据 0.5.5.5，
紧凑 `release_id` `9932ea87...`）各 7 次的中位数。这台机器远快于普通玩家设备，
CPU 降速只是近似，结论看两者的相对差，不把绝对值当作用户体验。

| 档位 | 数据目录 | 首次结果 | 启动屏消失 | 首次计算（消失后） | 第二次计算 | 传输 | 其中数据 | 数据原始字节 | 请求数 | 峰值 PSS | 稳定 PSS |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 主机 | 逐文件 | 1.10 s | 1.04 s | 60 ms | 9 ms | 8.29 MB | 4.73 MB | 72.1 MB | 97 | 594 MB | 467 MB |
| 主机 | 紧凑 | 1.43 s | 1.36 s | 61 ms | 8 ms | 7.55 MB | 3.98 MB | 48.8 MB | 14 | 585 MB | 440 MB |
| CPU 4× | 逐文件 | 1.63 s | 1.56 s | 72 ms | 39 ms | 8.29 MB | 4.73 MB | 72.1 MB | 97 | 564 MB | 466 MB |
| CPU 4× | 紧凑 | 1.96 s | 1.89 s | 75 ms | 41 ms | 7.55 MB | 3.98 MB | 48.8 MB | 14 | 599 MB | 442 MB |
| 4G | 逐文件 | 10.12 s | 10.07 s | 58 ms | 7 ms | 8.29 MB | 4.73 MB | 72.1 MB | 97 | 567 MB | 466 MB |
| 4G | 紧凑 | 9.88 s | 9.82 s | 64 ms | 7 ms | 7.55 MB | 3.98 MB | 48.8 MB | 14 | 537 MB | 423 MB |
| CPU 4× + 4G | 逐文件 | 10.57 s | 10.50 s | 69 ms | 35 ms | 8.29 MB | 4.73 MB | 72.1 MB | 97 | 591 MB | 499 MB |
| CPU 4× + 4G | 紧凑 | 10.34 s | 10.28 s | 57 ms | 36 ms | 7.55 MB | 3.98 MB | 48.8 MB | 14 | 544 MB | 457 MB |

传输除数据外还有 WASM 2.24 MB 与其余页面资源 1.32 MB（压缩后），两种方式相同。
主线程 JS 堆两者都约 37 MB；差别在 Worker 一侧。

解读：

- 紧凑包少约 9% 传输（数据 4.73 → 3.98 MB）和约 6–8% 稳定内存，原因是 block 里的文件是紧凑 JSON
  （原始 72 → 49 MB），且只含发行包声明的一种语言（zh-CN；zh-TW 的名词表与词缀表不在包内）。
- 网络是瓶颈时（4G）紧凑包快约 2%（≈0.24 s），与少掉的字节量相符；97 个请求按 8 路并发
  也没有让逐文件方式明显多付延迟。
- CPU 或网络不是瓶颈时紧凑包反而慢约 0.33 s（主机 +30%，CPU 4× +20%）：Worker 先整体 `JSON.parse`
  每个 block，再对其中每个文件做 UTF-8 编码、SHA-256 与一次语法 `JSON.parse`，三个 block 依次进行；
  逐文件方式边下载边校验，下载与校验重叠。
- 首次计算与之后的计算两者相同（同一引擎和同一份已暂存数据），差别全部在数据加载。
- 未测：重复访问（两种方式的数据都是不可变缓存）、真实手机与低内存设备、生产 CDN 的实际压缩级别。

结论：目前不切换，Web 继续使用逐文件快照。紧凑包没有明显更好——慢 CPU 或快网络下更慢，
慢网络下只快约 2%——而切换并不是小改动：

1. 语言范围：发行包只带一种语言，切换会让 zh-TW 失去原生基底／技能／词缀名，退回由简中转写。
   需要先让发行包（`runtime_pack`、`runtimeRelease.ts` 的 `validateRelease`／`validateSnapshot`）
   携带全部语言，或按语言分出可选 locale block。
2. 增量更新：逐文件方式在数据 bump 后只重新下载变化的文件；紧凑包任一 base 文件变化都要重下整个
   27.5 MB 的 core block（brotli 后约 1.75 MB）。
3. 解码成本：要在快设备上不变慢，需要去掉对每个文件的二次 `JSON.parse`、让 block 下载与解码重叠，
   或把 block 改成不需整体解析的格式。
4. 接线：CI `web` 作业在 `sync-data` 前组包并设置 `POBR_RUNTIME_PACK`／`POBR_RUNTIME_RELEASE_ID`；
   直接读取 `public/data/<version>/` 的 E2E、Vitest 基准与 `smoke-wasm-package` 需要改用物化后的候选。

若只为减少传输，可考虑的低风险方向是让逐文件快照发布紧凑 JSON，但文件字节由 snapshot manifest 的
SHA-256 固定，需要数据 owner 侧重新封存，不能在 PoBR 的 `sync-data` 里改写。
