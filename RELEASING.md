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

按顺序进行，前一步发布之后才能开始下一步：

1. **RePoE 发布 `game/<patch>-<n>`。** 在 [ackness/repoe](https://github.com/ackness/repoe) 按它的
   [RELEASING.md](https://github.com/ackness/repoe/blob/master/RELEASING.md) 声明、构建并发布游戏包
   Release（包、`receipt.json`、`SHA256SUMS`）。新快照声明的 `contract_version` 必须等于 PoBR 的
   `CONTRACT_VERSION`；需要新契约字段时，先在 PoBR 合并契约改动并打 `contract/<n>.<m>`，RePoE 再升级依赖
   （[数据契约版本](docs/data-architecture.md#31-数据契约版本)）。
2. **翻译库发布 `locale/<patch>-<n>`。** 在 [ackness/poe2-zh-cn-stats](https://github.com/ackness/poe2-zh-cn-stats)
   按它的 [RELEASING.md](https://github.com/ackness/poe2-zh-cn-stats/blob/main/RELEASING.md) 发布语言包，并附上为新游戏包
   交易目录生成的辅助显示（`support-display.zh-CN.json`、`supportDescriptions.zh-CN.json`）。辅助显示绑定
   `game/<patch>-<n>` 中 `overlay/trade_catalog.json` 的 SHA-256，所以只能在游戏 Release 之后生成；
   语言 Release 不带它时，PoBR 的 `data:bump` 拒绝换目标。
3. **翻译库的 `published` PR。** 发布后在翻译库另开 PR，把该 Release 的附件 pin 写进配置的 `published`，
   让它的 CI 持续下载并核对这个 Release；然后再到 PoBR 改锁。

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
- 命令按 `SHA256SUMS` 校验下载的附件，读取 `receipt.json`，替换两个包条目与 `support_display`，
  重算快照 pin、运行身份与 `release_id`，写入 `pipeline/data.lock.json` 和 `data/CURRENT`，再运行 `data:prepare`。
  导入失败时恢复原来的锁和 `CURRENT`，以非零状态退出。
- 标准输出是一份 `pobr-data-bump/v1` JSON：`before`／`after`（目标、快照、运行身份、`release_id`、
  各包的 tag 与 SHA-256、辅助显示）、`identity_changed`，以及本地已导入旧快照时的成员差异。保存它，贴进 PR 描述。
  成员路径带版本目录，所以新补丁的成员差异会把每个文件都列成删除加新增；这时改看 owner receipt 里各成员的分类。
- `data:bump` 不提交、不推送，也不开 PR。

同一个 PR 里迁移绑定游戏包或补丁的消费者，否则 PR 或完整 CI 会失败：

| 文件 | 何时改 | 怎么改 |
| --- | --- | --- |
| `crates/pobr-gamedata/tests/fixtures/repository-test-pins.json` | 任何新游戏包，包括同一补丁 | 按测试报出的实际值重绑该版本的 `game_manifest_sha256`，审阅变化的计数后更新 `pins`；只换语言不用改 |
| `web/src/lib/configGameRelevance.json` | 新游戏包 | `mise exec -- python3 devs/scripts/config_relevance.py` |
| `devs/ci/parse-coverage-baseline.json` | 新补丁，或覆盖率改变 | 按文件 `_meta.regen` 运行 `mise exec -- bash devs/scripts/regen-check.sh` 后更新 |
| 示例 golden 的 `data_version`（如 `examples/demo-bd-test/panel-golden.json`） | 新补丁 | 改成新补丁号 |

数值 golden 的重新抓取和计算语义修复按证据另行处理，不能降低基线换取通过
（[维护者指南](docs/maintainer-guide.md#运行时数据与会话)）。

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
- 审阅身份差异：两个包的 tag 与 SHA-256 是要采用的 Release（`producer_commit` 在锁的 diff 里）；`support_display`
  指向新的语言 Release；`test_fixtures` 保留原条目并带着它们发布时的 `target`；成员的增删改与 owner 的发布说明一致。
- PR gate 与完整 CI 都在当前 head 上通过后，用 merge commit 合并（`mise run pr-ready <N>` 可做只读预检）。
  master push 会再跑一次完整 CI，不部署。要上线，按 §4 发版。

## 3. 同一补丁上只换一个 owner

目标不变时 `data/CURRENT` 也不变，其余步骤同 §2：先 `data:prepare`，bump，迁移消费者，提交、开 PR、
`mise run ci <分支>`、审阅身份差异、合并。

**只换语言。** 翻译库发布新的 `locale/<patch>-<m>` 并照常补 `published` PR，游戏包不需要重新发布：

```sh
git switch -c chore/data-bump-locale-<patch>-<m> origin/master
mise run data:bump ackness/poe2-zh-cn-stats locale/<patch>-<m>
```

语言 Release 不带辅助显示时保留原条目，但它必须仍与新语言包配套：`data:bump` 核对它绑定的交易目录等于游戏包的，
Web 测试另要求它的 `_meta.source_commit` 等于新语言包 `i18n/zh-CN/_meta.json` 的 `source_commit`。
后者不成立时，请翻译库在这个 Release 里一并发布新的辅助显示。只换语言不需要重绑测试基线。

**只换游戏包。** RePoE 在同一补丁上发布 `game/<patch>-<n+1>`。交易目录不变时只 bump 游戏 owner：

```sh
git switch -c chore/data-bump-game-<patch>-<n+1> origin/master
mise run data:bump ackness/repoe game/<patch>-<n+1>
```

交易目录改变时原辅助显示不再绑定，`data:bump` 拒绝：翻译库须为新目录发布下一个 `locale/<patch>-<m>`
（语言包字节可以不变），PoBR 一次 bump 两者。任何新游戏包都要按 §2 的表重绑测试基线。

## 4. 发版上线

发版是一个单独的小 PR，只升 `[workspace.package].version`：

```sh
git fetch origin
git switch -c release/v0.x.y origin/master
# 编辑 Cargo.toml 的 [workspace.package] version = "0.x.y"
mise exec -- cargo metadata --format-version 1 > /dev/null   # 刷新 Cargo.lock 中继承 workspace 版本的包
mise exec -- bash devs/scripts/release-check.sh origin/master  # 应输出 release=true：SemVer 递增、锁版本一致、tag 未被占用
git commit -am "chore: release v0.x.y"
git push -u origin HEAD
gh pr create --fill
```

- PR 的 `plan checks` 作业运行同一项版本检查（“Validate version bump”）；`Cargo.toml`／`Cargo.lock` 的改动会选中全部轻量检查。
- 用 merge commit 合并。确认 master 上的合并提交后，在它上面打 annotated tag 并推送：

  ```sh
  git fetch origin
  git log -1 --format='%H %s' origin/master   # 应是 release/v0.x.y 的合并提交
  git tag -a v0.x.y -m "Release v0.x.y" <合并提交>
  git push origin v0.x.y
  ```

- tag 触发 [release.yml](.github/workflows/release.yml)：`release identity` 核对 tag 等于 workspace 版本、
  `Cargo.lock` 一致且 tag 指向该提交；`release validation` 对该提交运行完整 CI；`publish standalone WASM`
  把同次构建的 WASM 包、体积报告和 `SHA256SUMS` 附到 GitHub Release `v0.x.y`；
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

tag 不可移动、不可删除。同一 tag 的失败作业可以重跑（重跑会覆盖同名 Release 附件），但只在同次 run 的产物仍保留时有效
（`web-dist` 3 天，`wasm-release` 14 天）；过期后重跑整个 run。需要改代码时发下一个版本。

## 5. 回退

只能向前回退：已发布的 tag 不动，只有最高的稳定 v0 tag 会部署，重跑旧 tag 的 workflow 不会部署旧版本。

1. **数据回退。** 在新分支上回退 bump PR 的合并提交，恢复上一份锁的原字节（连同 `data/CURRENT`、
   `support_display`、`release_id` 和迁移过的消费者）：

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
