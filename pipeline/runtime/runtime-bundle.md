# 运行时数据组合封存与精简发行包

`data/<version>/manifest.json` 固定版本文件；运行时还读取同级 `overlay-common/` 的共享规则。
`data:bundle` 将两者连同临时导入的翻译、兼容表封存在一个独立目录中，让历史组合可以被取回。
封存不重新导出数据，也不替代来源、游戏数值或消费者验证。

```sh
mise run data:bundle -- --out .cache/data-bundles/first
mise run data:bundle:verify -- --bundle .cache/data-bundles/first
```

默认选择 `data/CURRENT`，也可传 `--version`。来源必须有补丁匹配的 schema-3 manifest；
所有已登记文件逐一验证 SHA-256 和 JSON。域候选标记、列漂移、损坏输入及符号链接会使封存失败。
输出必须是正式 `data/` 之外的新目录，已有目录不会被覆盖。输入在生成期间变化时不完成输出。

产物结构为 `bundle.json`、`data/CURRENT`、`data/<version>/` 及 `data/overlay-common/`。
只复制 manifest 登记的版本文件及全部共享 JSON，不把临时报告和用户 `patch/` 当作发行内容。
`bundle.json` 是最后写入的完成标记；若写入失败遗留了部分目录，验证会拒绝该目录。

收据中的两个身份用途不同：

- `bundle_id` 固定整个封存内容及收据契约。传输或取回旧包时，将已核对交付记录中的 ID
  传给 `--expected-id`；仅检查包内自洽，不能确认拿到的是原先指定的包。
- `runtime_identity` 按 Rust `GameData::identity()` 的同一格式计算默认运行时输入身份，
  包括共享规则，不包括用户 patch。manifest 的排版可能改变 bundle ID，而不改变这个运行时身份。

调用示例（`POBR_BUNDLE_ID` 来自事先核对的封存记录）：

```sh
mise run data:bundle:verify -- --bundle .cache/data-bundles/first --expected-id "$POBR_BUNDLE_ID"
POBR_DATA_ROOT="$PWD/.cache/data-bundles/first/data" mise exec -- cargo run -p pobr-cli -- \
  calculate-build --file examples/demo-bd-test/builds/sorceress-disciple-of-varashta-comet/code.txt --json
```

选择另一个已验证组合的 `data/` 目录即可进行独立的读取/回滚演练，无需改动仓库活动包。
本地 patch 作为另外的身份维护，回滚后需要针对选中的数据组合重新验证，再由消费者显式应用。
patch 合并类型冲突或快照摘要错误会拒绝 WASM 初始化，保留当前已加载的数据、结果缓存和构筑缓存；
成功切换才清空这些派生缓存。类型检查通过不证明 patch 对另一个版本的语义仍然适用。

本入口检验的是字节封存与可取回性：不会为旧数据补造来源收据，也不会将客户端解码、
游戏机制、国服数值或整库计算标成已认证。正式发布/采用仍须完成相应领域与消费者验收。

## 三块 JSON 发行包

正式 Web（不带 `POBR_RUNTIME_PACK` 的 `pnpm sync-data`）与独立 WASM 包
（`web/scripts/package-wasm.mjs`）目前仍发布逐文件的 schema-3 快照；三块发行包只在
Full CI `runtime` 任务中由同次 WASM 消费，数据锁的 `release_id` 固定该候选格式。
切换正式加载路径前须先在目标设备测量冷加载与数据更新时的重下载：逐文件按文件内容寻址、
可并发加载；三块顺序加载，任一文件变化都会重下整块。

`runtime_pack.py` 在已封存的完整 schema-3 组合上做传输投影，复用原有
`GameData` 文件路径、覆盖顺序与 DTO。它不读取原始客户端表，不实现另一套计算加载器。
[`data.lock.json`](../data.lock.json) 固定目标、locale、RePoE 游戏数据、翻译库语言、PoBR 本地规则
以及期望的发行身份 `release_id`；发行格式与块选择由 `runtime_pack.py` 固定。
选定 locale 的 `display_texts` 随翻译库的语言发布进入完整快照，组包时直接读取。

玩家目录恰有四个文件：`manifest.json` 与 `blocks/{core,rules,locale}.json`。
核心包含全部 base 域及历史 `passive_trees`；规则块包含 overlay、generated、共享规则与兼容表，
包括 SkillsPanel 使用的 `trade_catalog.gems`。语言块包含一个选定 locale 的外部固定词典、
`display_texts` 与新的 schema-3 文件清单。locale 与 gameplay realm 分别登记，不能互相推导。

每块的 `files` 是原有相对路径到 **JSON 字符串** 的映射。内层文档使用排序对象键和紧凑空白，
原始数组顺序、数字 token、字符串和 null 保持不变；Web 将字符串重新编码为 UTF-8，
不经 `JSON.parse` / `JSON.stringify` 重写游戏数字。这保留 i64、浮点精度和导入文本。
不使用新二进制格式。

投影删除 document-level `_meta`、语言 `_meta.json`，以及 special rule 的可选
`vendor_pattern` / `source_note` / `examples` 审计字段。保留所有其他数据，包括
`batch`、`verified`、`exporter`、`game_source`、CSD 分支／lookup／unrendered、
native passive provenance。最后一项是当前 Rust 的运行时校验和 modifier 归因输入，不能当作离线报告删除。
空值翻译阻断项不裁剪；缺译不会重新启用陈旧词典回退。

release manifest 固定 target、locale、全部虚拟文件 SHA-256、每块 SHA-256／长度／成员清单，
以及 `runtime_identity` 与 `release_id`。前者按原有 `GameData::identity` 算法标识**投影后的**字节；
后者封住整个发行契约。投影前的 snapshot runtime 身份只在 `build-receipt.json` 保存一次。
保留兼容输入仍标为保留兼容，不能声称由 RePoE 新导出。

发行包由 `mise run data:pack build` 从已准备的 owner 快照组装（见下文「独立数据仓库输入与 CI」）。
输出必须是新目录，不能指向正式 `data/`。`build` 同时写出 `runtime-pack.zip`：
固定文件顺序、时间、权限和 deflate 参数，里面是同一份四文件发行包的原始字节。

## 既有 CLI / WASM 适配

```sh
mise run data:pack build --out .cache/runtime-pack
mise run data:pack materialize --pack .cache/runtime-pack/runtime-pack.zip \
  --expected-id "$RELEASE_ID" --out .cache/runtime-materialized
# Existing CLI resolves data/CURRENT and the existing schema-3 loader:
POBR_DATA_ROOT="$PWD/.cache/runtime-materialized/data" mise exec -- ./target/debug/pobr \
  calculate-build --file "$BUILD_CODE_FILE" --json
# Explicit candidate Web sync; normal sync continues to select adopted data:
POBR_RUNTIME_PACK="$PWD/.cache/runtime-pack/player" \
POBR_RUNTIME_RELEASE_ID="$RELEASE_ID" mise exec -- node web/scripts/sync-data.mjs
```

materialize 生成完整 legacy-shaped `data/` 与可由 `data:bundle:verify` 核验的 `bundle.json`。
它与 Web staging 的每个虚拟文件逐字节相同，所见 runtime identity 相同。
WASM 的 `stageDataBytes` / `initStagedDataVerified` 无需 Rust 修改。
独立 WASM 发行脚本默认仍使用已采用数据；本入口生成的候选不替换既有正式发行物。

Web sync 验证后只发布 manifest 与三个按 SHA-256 寻址的块，不复制 legacy 单文件目录或离线证据。
发布先准备完整临时目录，再替换目标目录；替换失败会恢复原目录。如果恢复操作也失败，
错误信息给出保留的 `previous` 备份路径，可将该完整目录恢复到原位置后重试，不能只恢复 manifest。
`loadDataPack` 每次只读取一块，核对 release、块、成员及内部 snapshot inventory，再沿用现有 staging；
仅保留页面后续读取的文件。单独请求虚拟文件时由其块恢复。重复启动请求相同 immutable URL。
新块格式必须有 SHA-256 支持，缺少时失败；原有 manifest/schema-1/2/3 加载兼容路径仍保留。
未知格式、混合 target/locale、损坏块、缺失成员或错配 snapshot 都拒绝初始化。

## 独立数据仓库输入与 CI

完整运行输入由 [`build_data.py`](build-data.md) 在临时目录组合：RePoE游戏包、
翻译库语言及PoBR自有解释规则逐一认证，必须恢复锁中的完整snapshot/runtime身份。
PoBR自己的Git/artifact ref不能供应完整数据。本地组合封存（`data:bundle`）只用于诊断，
不是正式外部数据发布或默认获取通道。

```sh
mise run data:prepare
mise run data:pack build --out .cache/runtime-pack     # 从已准备的 owner 快照组精简三块
mise run data:pack verify --pack .cache/runtime-pack/player
```

`build` 逐项校验已安装快照的 schema-3 清单摘要，确认其运行身份等于数据锁，再投影成三块；
锁中记录了 `release_id` 时结果必须一致（`--expected-id` 可另行指定）。
它不重跑任何领域生产或显示生成：这些字节已在 owner 发布中，PoBR 只负责打包。
数据锁同时固定 game 与 language 的真实仓库、完整 commit、tag、路径和摘要。

Full CI在需要实物的Rust、Web和自动化测试前显式prepare。`runtime`任务等待同次WASM构建，
自己prepare并从快照组精简三块，
再由同次WASM消费；失败／跳过会阻止Full gate。
游戏／语言源表、verification与版本binding不进入Web玩家包。
远端CI、来源准入、完整消费者验收和采用仍须分别确认。

定向验证：`test_build_data.py`覆盖两owner组合、数据锁schema与tag、临时安装、回退版本、缺失／损坏pin和禁网cache；
`test_runtime_pack.py`覆盖三块传输、从快照组包与CLI物化。
这些合成契约不等于Rust真实生产、数值parity或远端CI证据。
