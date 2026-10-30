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
[SKILL.md](https://github.com/ackness/pobr/blob/master/.claude/skills/use-pobr-wasm/SKILL.md)
和 [API 指南](https://github.com/ackness/pobr/blob/master/.claude/skills/use-pobr-wasm/references/api.md)。
生成的 `pobr_wasm.d.ts` 描述 JS 导出；`api-types.ts` 提供 JSON 请求和响应类型，
其依赖的 `generated/request.ts` 也随包附带。类型使用 `import type` 引入。
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
不改变 dev/test 设置。locale TOML 在构建主机上校验并生成字符串表；继续编辑原 locale
文件即可，TOML 解析器不进入 WASM。

构建前检查源码指纹、工具版本、相关环境与 Cargo 配置、生成绑定哈希，以及编译出的 schema。
输入和产物未变就复用现有包及其构建记录，不调用 wasm-pack。
`mise run web build-wasm --force` 强制通过 Cargo 重建，但不清缓存。
记录缺失或格式过旧时，正常重建一次；只有构建成功且期间输入未变，才写入新记录。

打包再次核对 Rust 源码、内嵌语言文件、共享 `*_config_options.json`、manifest、lockfile、
构建配置和 JSON 类型，检查生成绑定哈希，并从已编译引擎读取 schema。
它还比较同步数据与当前快照、patch、共享层的字节，包含同版本数据修改。
过期时需先重建或同步；打包本身不构建、不下载。
直接运行 wasm-pack 而没有构建记录的产物无法打包。
之后无关的提交不会改变二进制来源：发布 manifest 保留编译时的 commit 和 dirty 标记。
缓存统计与 dev/test 规则见[开发工作流](https://github.com/ackness/pobr/blob/master/docs/development-workflow.md)。
旧的版本归档保留到显式删除，打包不会清理发布历史。

CI 会测试打包脚本、构建 WASM、打包，再解压到临时目录，计算新角色和已有构筑。
手动触发会保存 `wasm-release` Actions artifact 和大小报告。
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
