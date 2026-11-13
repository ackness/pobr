# 换数据与发版

本文是维护者的端到端步骤，跨三个仓库：新游戏补丁、只换语言、发版上线与回退。
它们遵守的规则见[数据架构 §5](docs/data-architecture.md#5-更新采用与发布)，
CI 与 release workflow 的机制见[开发工作流](docs/development-workflow.md#发布与部署)，
当前锁与缺口见[数据现状](docs/data-status.md)。

换数据和上线是两个独立的 PR：

| 动作 | PR 改什么 | 合并后 |
| --- | --- | --- |
| 换数据（§1–§3） | `pipeline/data.lock.json`；换游戏包时另有按游戏包登记的测试基线，新补丁还有 `data/CURRENT` | master 运行完整 CI，不部署 |
| 发版（§4） | 只改 workspace 版本（`Cargo.toml`、`Cargo.lock`） | 推送 `v0.x.y` tag：完整验证、发布 Release 附件、部署网站 |

发版带上 master 当时已有的全部数据与代码；数据 bump PR 里不升版本。

## 1. 新游戏补丁：owner 先发布

按顺序完成 owner 发布与核对，再到 PoBR 改锁：

1. **RePoE 发布 `game/<patch>-<n>`。** 在 [ackness/repoe](https://github.com/ackness/repoe) 按它的
   [RELEASING.md](https://github.com/ackness/repoe/blob/master/RELEASING.md) 声明、构建并发布游戏包
   Release（包、`receipt.json`、`SHA256SUMS`）。新快照声明的 `contract_version` 必须等于 PoBR 的
   `CONTRACT_VERSION`；需要新契约字段时，先在 PoBR 合并契约改动并打 `contract/<n>.<m>`，RePoE 再升级依赖
   （[数据契约版本](docs/data-architecture.md#31-数据契约版本)）。
2. **翻译库发布 `locale/<patch>-<n>`。** 在 [ackness/poe2-zh-cn-stats](https://github.com/ackness/poe2-zh-cn-stats)
   按它的 [RELEASING.md](https://github.com/ackness/poe2-zh-cn-stats/blob/main/RELEASING.md)，先在配置 PR 中固定
   `expect.package` 的 `bytes`、`sha256`，审阅合并并确认同一提交的 CI 通过后，再发布语言包。
   宝石说明译文随语言包 `display_texts.json` 的作用域发布，英文随游戏包发布
   （[数据架构](docs/data-architecture.md#3-发布与导入契约)）。
3. **核对语言 Release。** 翻译库 CI 按 `locale/<patch>-<n>` → `release/<patch>-<n>.json`
   映射新 tag 的配置，下载 Release 的 zip 和 `SHA256SUMS`，与配置 pin 及重建的 zip 核对。
   确认不可变 Release 通过核对后，再到 PoBR 改锁。

## 2. 新游戏补丁：PoBR 改锁

从最新 master 开分支，先导入当前锁（这样 bump 才能列出成员差异），再一次 bump 两个 owner：

```sh
git fetch origin
git switch -c chore/data-bump-<patch>-<n> origin/master
mise run data:prepare
mise run data:bump ackness/repoe game/<patch>-<n> ackness/poe2-zh-cn-stats locale/<patch>-<m>
```

游戏包与语言包的序号各自递增，`<n>` 与 `<m>` 不必相同。

- owner 也可简写为 `repoe`、`poe2-zh-cn-stats`。目标改变时只 bump 一个 owner 会被拒绝。
- 命令按 `SHA256SUMS` 校验下载的附件，读取 `receipt.json`，替换两个包条目，
  重算快照 pin、运行身份与 `release_id`，写入 `pipeline/data.lock.json` 和 `data/CURRENT`，再运行 `data:prepare`。
  导入失败时恢复原来的锁和 `CURRENT`，以非零状态退出。
- 标准输出是一份 `pobr-data-bump/v1` JSON：`before`／`after`（目标、快照、运行身份、`release_id`、
  各包的 tag 与 SHA-256）、`identity_changed`，以及本地已导入旧快照时的成员差异。安装后还会在 `outputs`
  给出 L2 的逐构筑、逐 stat 差异，并在 stderr 打印带空归因栏的 Markdown 表格。保存 JSON 与表格，贴进 PR 描述。
  成员路径带版本目录，所以新补丁的成员差异会把每个文件都列成删除加新增；这时改看 owner receipt 里各成员的分类。
- L2 报告失败只记 `outputs.status = "unavailable"` 和原因，不回滚已安装的锁；修好环境后运行
  `mise run snapshot:report` 补报告。`--no-prepare` 不跑 L2。报告不替代提交前的快照验证。
- `data:bump` 不提交、不推送，也不开 PR。

同一个 PR 里迁移绑定游戏包或补丁的消费者，否则 PR 或完整 CI 会失败：

| 文件 | 何时改 | 怎么改 |
| --- | --- | --- |
| `crates/pobr-gamedata/tests/fixtures/repository-test-pins.json` | 任何新游戏包，包括同一补丁 | 按测试报出的实际值重绑该版本的 `game_manifest_sha256`，审阅变化的计数后更新 `pins`；只换语言不用改 |
| `web/src/lib/configGameRelevance.json` | 新游戏包 | `mise exec -- python3 devs/scripts/config_relevance.py` |
| `devs/ci/parse-coverage-baseline.json` | 新补丁，或覆盖率改变 | 按文件 `_meta.regen` 运行 `mise exec -- bash devs/scripts/regen-check.sh` 后更新 |
| `examples/output-snapshot.json` | 任何 `runtime_identity` 改变，包括只换语言 | 审阅并填写 L2 每行归因后，`mise run snapshot:record`；与锁一起提交，即使输出没有变化也重绑身份 |
| `devs/reference-sources.json` 的 `pob2.data_version` 和 PoB2 golden（如 `examples/demo-bd-test/panel-golden.json`） | PoB2 跟上当前补丁时 | 核实 pin 的游戏版本，用对应 PoB2 重抓参考；落后期间保留旧身份，不给旧 golden 重贴新版本 |

按[维护者指南的 L2 / L3 规则](docs/maintainer-guide.md#parity-体系回归基准)逐行归因并接受数据变化；
计算语义修复放在另一个引擎 PR。接受后的输出成为新锁的 L2 快照。

新补丁（包括 1.0 上线）在本地准备好数据并完成上表迁移后，提交前运行一次组合验收：

```sh
mise run data:accept
```

四步必须全部 PASS；步骤、参数、环境准备与结果含义见[新补丁验收](docs/development-workflow.md#新补丁验收)。
这不代替 L2 逐行归因或合并前的完整 CI。

提交、推送并开 PR，然后在推送的分支上请求完整 CI：

```sh
git add pipeline/data.lock.json data/CURRENT <迁移的消费者文件>
git commit -m "chore(data): pin game/<patch>-<n> and locale/<patch>-<n>"
git push -u origin HEAD
gh pr create --title "chore(data): bump to <patch>" --body-file <PR 描述>   # 按 PR 模板写，附上 data:bump 的输出
mise run ci chore/data-bump-<patch>-<n>
mise run ci-status --branch chore/data-bump-<patch>-<n>
```

- 分支的数据锁与 master 不同，所以完整 CI 自动加跑数据 bump 冒烟 E2E
  （[data-bump-smoke.spec.ts](web/e2e/data-bump-smoke.spec.ts)）。PR gate 不运行它，合并前必须有这次完整 CI。
  核对 run 的 head SHA 与结果，dispatch 成功不等于通过。
- 审阅身份差异：两个包的 tag 与 SHA-256 是要采用的 Release（`producer_commit` 在锁的 diff 里）；可选的
  `test_fixtures` 保留原条目并带着它们发布时的 `target`；成员的增删改与 owner 的发布说明一致。
- PR gate 与完整 CI 都在当前 head 上通过后，用 merge commit 合并（`mise run pr-ready <N>` 可做只读预检）。
  master push 会再跑一次完整 CI，不部署。要上线，按 §4 发版。

## 3. 同一补丁上只换一个 owner

目标不变时 `data/CURRENT` 也不变，其余步骤同 §2：先 `data:prepare`，bump，迁移消费者，提交、开 PR、
`mise run ci <分支>`、审阅身份差异、合并。

**只换语言。** 翻译库按 [§1 的语言包发布与核对步骤](#1-新游戏补丁owner-先发布)发布并核对新的
`locale/<patch>-<m>`，游戏包不需要重新发布：

```sh
git switch -c chore/data-bump-locale-<patch>-<m> origin/master
mise run data:bump ackness/poe2-zh-cn-stats locale/<patch>-<m>
```

只换语言不需要重绑游戏测试 pins；运行身份改变时仍按 §2 重录 L2。

**只换游戏包。** RePoE 在同一补丁上发布 `game/<patch>-<n+1>`，只 bump 游戏 owner：

```sh
git switch -c chore/data-bump-game-<patch>-<n+1> origin/master
mise run data:bump ackness/repoe game/<patch>-<n+1>
```

任何新游戏包都要按 §2 的表重绑测试基线。

## 已知目标的本地回放

用已发布的 `poe2 / 0.5.5.5 / international` 验证三仓库的 build → staging → bump/install → L2，
不采用新数据。游戏声明为 RePoE `release/0.5.5.5-3`，语言配置为翻译库 `release/0.5.5.5-4.json`；
PoBR 起始锁须仍引用对应 Release。发布规则仍见 [§1](#1-新游戏补丁owner-先发布) 和两个 owner 的发布文档。

每轮使用三个固定 revision 的干净 worktree，记录各自 `HEAD`、tree 与 mise 工具版本；新 worktree 先 `mise trust`。
按[本地复用](docs/development-workflow.md#本地复用)复用编译缓存，各树保留自己的可写 target。
下面的路径变量须改成这三个 worktree 的绝对路径；默认 release 输出必须尚不存在。不要复用已完成的回放目录。

```sh
set -eu
REPOE_WT=/absolute/path/to/repoe-worktree
LOCALE_WT=/absolute/path/to/locale-worktree
POBR_WT=/absolute/path/to/pobr-worktree
REPLAY=$(mktemp -d "${TMPDIR:-/tmp}/pobr-replay.XXXXXXXX")
test ! -e "$REPOE_WT/.cache/release"
test ! -e "$LOCALE_WT/.cache/release"
mkdir "$REPLAY/data"
cp "$POBR_WT/pipeline/data.lock.json" "$REPLAY/data.lock.json"
cp "$POBR_WT/data/CURRENT" "$REPLAY/data/CURRENT"
cp -R "$POBR_WT/data/overlay-common" "$REPLAY/data/overlay-common"
```

输入只从固定 Git 对象、不可变 Release 或已核对的同源缓存取得。下面的 `--fetch` 获取缺失的 owner 输入；
PoBR prepare 默认下载缺失的锁定归档。也可普通复制已有缓存条目：RePoE 的 Release 输入放在
`$REPOE_WT/.cache/release/inputs/downloads/<sha256>`，pin 来自所选声明的 `inputs.lock.json`；
PoBR 的游戏／语言 ZIP 和 `test_fixtures` 归档放在 `--cache` 指定目录下的 `<sha256>`，pin 来自起始锁。
build/prepare 都重新校验 bytes、SHA-256、manifest 和目标；不能只凭文件名认定缓存可信。
准备好这三份 PoBR 归档后可给 prepare 加 `--offline`，但 publication preflight 仍读取远端元数据，
所以整条流程不承诺离线。RePoE Python 环境按其 AGENTS 配置，已有 venv 可只读复用，不必同步环境。

```sh
(cd "$REPOE_WT" && mise run release:build -- --config release/0.5.5.5-3 --fetch --check)
(cd "$LOCALE_WT" && mise run release:build -- --config release/0.5.5.5-4.json --fetch --check)
(cd "$POBR_WT" && mise exec -- python3 -B -m pipeline.runtime.build_data \
  --lock "$REPLAY/data.lock.json" --cache "$REPLAY/cache" \
  --install "$REPLAY/data" --out "$REPLAY/old-prepared")
env -u POBR_DATA_VERSION \
  POBR_DATA_ROOT="$REPLAY/data" POBR_SNAPSHOT_LOCK="$REPLAY/data.lock.json" \
  POBR_OUTPUT_SNAPSHOT="$POBR_WT/examples/output-snapshot.json" \
  POBR_SNAPSHOT_REPORT="$REPLAY/old-l2.json" \
  mise -C "$POBR_WT" run snapshot:report
```

这四步须以 0 退出，旧快照须真正安装，才能把 bump 的成员与 L2 报告作为比较证据。接着分别运行公开发布 dry-run，
保留完整日志和退出码；下面只接受退出码 1，仍须核对日志中的**全部拒绝原因**，任何额外错误都先停下排查：

```sh
if (cd "$REPOE_WT" && mise run release:publish -- \
  --config release/0.5.5.5-3 --dry-run --reuse) >"$REPLAY/game-publish.log" 2>&1; then
  cat "$REPLAY/game-publish.log"; echo "unexpected publish preflight success" >&2; exit 1
else
  status=$?; cat "$REPLAY/game-publish.log"; test "$status" -eq 1
fi
if (cd "$LOCALE_WT" && mise run release:publish -- \
  --config release/0.5.5.5-4.json --dry-run --reuse) >"$REPLAY/locale-publish.log" 2>&1; then
  cat "$REPLAY/locale-publish.log"; echo "unexpected publish preflight success" >&2; exit 1
else
  status=$?; cat "$REPLAY/locale-publish.log"; test "$status" -eq 1
fi
```

这组历史配置会成功 staging，然后拒绝发布：游戏声明 `game/0.5.5.5-3`，下一序号已是 `-4`；
语言声明 `locale/0.5.5.5-4`，下一序号已是 `-5`，且相同内容已发布（日志同时提示配置映射不符、无需重发）。
远端状态改变或出现其他拒绝时，先解释差异，不能把任意 exit 1 当作预期结果。
staging 成功不等于 publication preflight 通过；不修改历史声明来争取 exit 0，也不运行日志里打印的发布命令。

将**本轮公开命令**产生的 ZIP、新 receipt 和 SHA256SUMS 普通复制到新镜像，保留原始绑定：

```sh
GAME_ASSETS="$REPOE_WT/.cache/release/publish/poe2-international-0.5.5.5-3/assets"
LOCALE_ASSETS="$LOCALE_WT/.cache/release/publish/poe2-international-0.5.5.5-4/assets"
GAME_MIRROR="$REPLAY/mirror/ackness/repoe/releases/download/local-n4/game-0.5.5.5"
LOCALE_MIRROR="$REPLAY/mirror/ackness/poe2-zh-cn-stats/releases/download/local-n4/locale-0.5.5.5"
mkdir -p "$GAME_MIRROR" "$LOCALE_MIRROR"
cp "$GAME_ASSETS/pobr-game-0.5.5.5.zip" "$GAME_ASSETS/receipt.json" "$GAME_ASSETS/SHA256SUMS" "$GAME_MIRROR/"
cp "$LOCALE_ASSETS/pobr-locale-0.5.5.5.zip" "$LOCALE_ASSETS/receipt.json" "$LOCALE_ASSETS/SHA256SUMS" "$LOCALE_MIRROR/"
(cd "$GAME_MIRROR" && shasum -a 256 -c SHA256SUMS)
(cd "$LOCALE_MIRROR" && shasum -a 256 -c SHA256SUMS)
POBR_RELEASE_BASE_URL="file://$REPLAY/mirror" POBR_RELEASE_TRANSPORT=https \
  POBR_DATA_ROOT="$REPLAY/data" mise -C "$POBR_WT" run data:bump -- \
  repoe local-n4/game-0.5.5.5 locale local-n4/locale-0.5.5.5 \
  --lock "$REPLAY/data.lock.json" --cache "$REPLAY/cache" --data-root "$REPLAY/data" \
  >"$REPLAY/bump.json" 2>"$REPLAY/bump.log"
cat "$REPLAY/bump.json" "$REPLAY/bump.log"
mise -C "$POBR_WT" run data:check
```

镜像恰有上述两个目录、各三个附件；`local-n4/*` 只是在本地 URL/副本锁中使用的别名，不是真实 tag，
不能给新构建配旧的已发布 receipt。`file://` 已在本地环境实测，其他环境先验证传输；必要时用只绑定
`127.0.0.1` 的本地 HTTP 镜像替换 base URL。不上传附件。

bump 须以 0 退出并实际安装；不可用 `--no-prepare` 代替回放。按 [§2](#2-新游戏补丁pobr-改锁) 检查完整身份／成员／
`outputs` 结果，`outputs.status = "unavailable"` 仍须排查。保留 build 的 N3 汇总、receipt 中的详情／省略计数及有界值，
按 [L2 命令](docs/development-workflow.md#l2-输出快照命令) 检查计算差异。这一已知内容回放应保持包字节、全部安装文件、
runtime/release 身份与 L2 输出不变；新 receipt 如因生产者或路径不同而变化，要逐字段记录，不能重写证据。
要验证重复性，在固定代码／工具／输入下完整跑两轮，分别重建、staging、安装，并比较 ZIP、receipt、SHA256SUMS、
锁、安装文件、N3 与 L2。保留两轮目录；不改生产锁／默认数据，不运行真实 tag、push、Release 或部署。

## 4. 发版上线

发版是一个单独的小 PR：升 `[workspace.package].version`，并在 [CHANGELOG.md](CHANGELOG.md) 顶部写本版一节。

```sh
git fetch origin
git switch -c release/v0.x.y origin/master
git log --first-parent --format='%s%n    %b' v<上一版>..origin/master   # 本版包含的 PR：合并提交标题有 PR 号，正文是 PR 标题；squash 合并的标题末尾带 PR 号
# 编辑 Cargo.toml 的 [workspace.package] version = "0.x.y"，在 CHANGELOG.md 写本版一节（写法见下）
mise exec -- cargo metadata --format-version 1 > /dev/null   # 刷新 Cargo.lock 中继承 workspace 版本的包
mise exec -- bash devs/scripts/release-check.sh origin/master  # 应输出 release=true：SemVer 递增、锁版本一致、CHANGELOG 有本版一节、tag 未被占用
python3 devs/scripts/release-policy.py notes v0.x.y .cache/release-notes.md   # 取出本版一节，作为 PR 说明
git commit -am "chore: release v0.x.y"
mise run e2e release   # 须全部通过：本地完整浏览器套件，写下该提交 tree 的回执
git push -u origin HEAD
gh pr create --title "chore: release v0.x.y" --body-file .cache/release-notes.md
```

- 浏览器套件只在本地运行（`mise run ci` 不再接受 `--e2e`）；层级、回执与端口见[浏览器测试](docs/development-workflow.md#浏览器测试)。
  发版带新游戏补丁时，先完成 [§2 的新补丁验收](#2-新游戏补丁pobr-改锁)；仍须运行完整的 `e2e release`。

CHANGELOG 的一节用英文写，读者依次是 pobr.win 的玩家、使用 WASM 包／CLI／数据包的开发者；写读者看到的变化，不写实现过程：

- 标题 `## [0.x.y] - YYYY-MM-DD`（发版日期），下面两三句概述本版最重要的变化。
- 小节按此顺序，没有内容的省略：`### Breaking changes`、`### Added`、`### Changed`、`### Fixed`、`### Data`、
  `### For developers`、`### Internal`。`### Breaking changes` 必须保留，核对后没有就写 `- None.`。
- 破坏性变更指 WASM、CLI 或数据包的使用者要改代码或数据才能继续工作的变化，例如 WASM 导出或 JSON 字段被删除、改名、
  改类型或改变取值含义，`SCHEMA_VERSION` 或数据契约 `CONTRACT_VERSION` 改变，CLI 子命令或参数改变，
  旧版保存的构筑或本地设置读不出来。只新增可选字段不算。
- 每条末尾标出 PR，如 `(#322)`；范围内每个 PR 至少出现一次，发版 PR 本身不列。
- 每段、每条写成一行，不手动折行：GitHub 的 Release 和 PR 说明会把换行原样显示。
- 文件底部加本版的对比链接 `[0.x.y]: https://github.com/ackness/pobr/compare/v<上一版>...v0.x.y`。

`release-policy.py` 检查标题与日期格式、`### Breaking changes` 小节和对比链接，不检查内容；内容由 review 核对。

- PR 的 `plan checks` 作业运行同一项版本检查（“Validate version bump”）；`Cargo.toml`／`Cargo.lock` 的改动会选中全部轻量检查。
- 用 merge commit 合并。确认 master 上的合并提交后，在它上面打 annotated tag 并推送：

  ```sh
  git fetch origin
  git log -1 --format='%H %s' origin/master   # 应是 release/v0.x.y 的合并提交
  mise run release:e2e-check <合并提交>        # 须 PASS
  git tag -a v0.x.y -m "Release v0.x.y" <合并提交>
  git push origin v0.x.y
  ```

- `release:e2e-check` 不通过时，`git switch --detach <合并提交>` 后重跑 `mise run e2e release`，通过后再打 tag。
- tag 触发 [release.yml](.github/workflows/release.yml)：`release identity` 核对 tag 等于 workspace 版本、
  `Cargo.lock` 一致、CHANGELOG 有本版一节且 tag 指向该提交；`release validation` 对该提交运行完整 CI；
  `publish standalone WASM` 创建 GitHub Release `v0.x.y`，附上同次构建的 WASM 包、体积报告和 `SHA256SUMS`，
  说明是 CHANGELOG 本版一节、对比链接和体积报告；
  `deploy stable v0 to Cloudflare Pages` 把同次的 `web-dist` 部署到 Pages 项目 `pobr-web` 的 `master`
  分支，即 [pobr.win](https://pobr.win) 与 [pobr-web.pages.dev](https://pobr-web.pages.dev)。
- 只有稳定的 `v0.x.y`、且是 origin 上最高的稳定 v0 tag 才部署；带 prerelease 后缀的版本只发布为 prerelease。
- 核对：`gh run list --workflow release.yml` 中该 tag 的 run 成功，且 deploy 作业实际执行了部署步骤；
  [Releases](https://github.com/ackness/pobr/releases) 上有附件；打开两个站点，顶栏显示 `v0.x.y`。
  站点部署的是同次 run 的 `web-dist`，数据由 tag 提交上的锁决定：`git show v0.x.y:pipeline/data.lock.json`
  里的包 tag 应是预期的 Release。界面上的数据版本（悬停顶栏版本或看概览）只显示游戏补丁号，同一补丁的新包不会改变它。

- 显示检查：`mise run builds:fetch .cache/ninja-<日期>` 抓当前赛季的 poe.ninja 构筑，再运行
  `mise run audit:display --release v0.x.y --strict .cache/ninja-<日期>`。应以 0 退出：Errors 与 Raw ids 两节为空，
  宝石目录的原始 ID 名为 0。
  想在打 tag 前先看，对本地 `mise run web build` 的结果运行同一命令，去掉 `--release`。
  报告各节的含义见 [diagnostics.md](docs/diagnostics.md#前端显示审计)。

tag 不可移动、不可删除。同一 tag 的失败作业可以重跑（重跑会覆盖同名 Release 附件，不改 Release 说明），但只在同次 run 的产物仍保留时有效
（`web-dist` 3 天，`wasm-release` 14 天）；过期后重跑整个 run。需要改代码时发下一个版本。

## 5. 回退

只能向前回退：已发布的 tag 不动，只有最高的稳定 v0 tag 会部署，重跑旧 tag 的 workflow 不会部署旧版本。

1. **数据回退。** 在新分支上回退 bump PR 的合并提交，恢复上一份锁的原字节（连同 `data/CURRENT`、
   `release_id` 和迁移过的消费者）：

   ```sh
   git switch -c revert/data-bump-<patch>-<n> origin/master
   git revert -m 1 <bump PR 的合并提交>
   mise run data:prepare
   ```

   然后像 §2 一样开 PR、`mise run ci <分支>`（锁改变，冒烟 E2E 会运行）、合并。不要用 `data:bump` 手动 bump 回旧 Release：
   revert 恢复的是逐字节相同的锁。回退目标必须仍被当前代码接受：快照的 `contract_version` 须等于当前的
   `CONTRACT_VERSION`，`game/0.5.5.5-1` 及更早的游戏包（快照仍列 `i18n/`、不声明 `contract_version`）也会被拒绝；
   这种情况由 owner 发布修正后的新 Release，向前修复。想在 revert 之前先在本地试旧锁，用
   `git show <bump PR 的合并提交>^1:pipeline/data.lock.json > .cache/rollback.lock.json` 取出它，再
   `mise run data:update --lock .cache/rollback.lock.json --out .cache/rollback-review`（[锁驱动的本地选择](pipeline/runtime/locked-update.md)）。
2. **代码回退。** 同样 `git revert` 出问题的合并提交，经 PR 合并。
3. **上线。** 按 §4 发下一个版本（`v0.x.(y+1)`）。

Cloudflare Pages 控制台里的部署回滚不在本流程内：仓库没有配置或使用它，生产站点只跟随最高的稳定 tag。
