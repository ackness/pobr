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

`runtime_pack.py` 在已封存的完整 schema-3 组合上做传输投影，复用原有
`GameData` 文件路径、覆盖顺序与 DTO。它不读取原始客户端表，不实现另一套计算加载器。
`data-requirements.json.runtime_pack` 声明发行格式、块选择和投影；
[`build-data.lock.json`](build-data.md)分别锁住RePoE游戏数据、翻译库语言和PoBR本地政策代码。
显示输入仍由 `data-imports.lock.json` 固定的 RePoE / 翻译库生成。

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
后者封住整个发行契约。投影前的 snapshot/runtime 身份、recipe 所有权、producer receipt pin
只在 `build-receipt.json` 保存一次。保留兼容输入仍标为保留兼容，不能声称由 RePoE 新导出。

构建已有完整组合（`--bundle` 是包含 `bundle.json` 与 `data/` 的目录）：

```sh
mise run data:pack create --bundle "$FROZEN_BUNDLE" --recipe "$FROZEN_RECIPE" \
  --display "$VERIFIED_DISPLAY_JSON" --out .cache/runtime-release-a
mise run data:pack verify --pack .cache/runtime-release-a/player --expected-id "$RELEASE_ID"
mise run data:pack verify --pack .cache/runtime-release-a/runtime-pack.zip --expected-id "$RELEASE_ID"
```

`create` 对 recipe 的完整版本文件／共享规则 inventory 与原 bundle 身份逐一核对，
不访问 recipe 中历史绝对来源路径。输出必须是新目录，不能指向正式 `data/`。
ZIP 使用固定文件顺序、时间、权限和 deflate 参数，里面是同一份四文件发行包的原始字节。
raw 和逐文件 gzip-9 分解写入 build receipt；这些是本地大小，不是 HTTP 时间或实际传输测量。

## 既有 CLI / WASM 适配

```sh
mise run data:pack materialize --pack .cache/runtime-release-a/runtime-pack.zip \
  --expected-id "$RELEASE_ID" --out .cache/runtime-materialized-a
# Existing CLI resolves data/CURRENT and the existing schema-3 loader:
POBR_DATA_ROOT="$PWD/.cache/runtime-materialized-a/data" mise exec -- ./target/debug/pobr \
  calculate-build --file "$BUILD_CODE_FILE" --json
# Explicit candidate Web sync; normal sync continues to select adopted data:
POBR_RUNTIME_PACK="$PWD/.cache/runtime-release-a/player" \
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
PoBR自己的Git/artifact ref不能供应完整数据；旧`ackness/pobr`核心输入锁已移除并显式拒绝。
旧组合封存与本地`export-inputs`工具仅用于诊断，不是正式外部数据发布或默认获取通道。

```sh
mise run data:prepare
mise run data:imports --out .cache/declared-display
mise run data:ordinary execute --adapter target/debug/pobr-data-adapter --out .cache/declared-ordinary
mise run data:pack import --imports .cache/declared-display --ordinary .cache/declared-ordinary \
  --out .cache/declared-runtime
```

后续重放加`--offline`，只读认证后的内容寻址缓存；不回退到tracked快照或其它版本。
新build lock同时固定game与language的真实仓库、完整commit、路径和摘要。
`runtime_pack.ordinary`仍声明六域生产需求／锁及八个业务替换路径。
声明普通生产就必须提供`--ordinary`；完成状态、目标、需求／锁、source handoff、
producer receipt及业务文件均重新核验，不能静默沿用旧领域文件。

Full CI在需要实物的Rust、Web和自动化测试前显式prepare；Rust多版本测试还显式安装
固定旧回退包，保持活动CURRENT不变。`runtime`任务等待同次普通生产与显示校验，
接收outputs-only artifact后组合精简三块；失败／跳过会阻止Full gate。
游戏／语言源表、verification与版本binding不进入Web玩家包。
远端CI、来源准入、完整消费者验收和采用仍须分别确认。

定向验证：`test-build-data.py`覆盖两owner组合、临时安装、回退版本、缺失／损坏pin和禁网cache；
`test-runtime-pack.py`与`test-runtime-ordinary.py`覆盖三块传输、领域替换及CLI物化。
这些合成契约不等于Rust真实生产、数值parity或远端CI证据。
