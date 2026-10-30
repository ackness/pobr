# CI 与协作流程研究及 PoBR 适配

研究日期：2026-10-30。PoBR 基线：`d08e619e5c72b0c88d2d17dc85d160662ebc510a`。
这是有日期的研究记录；当前操作以[开发工作流](../development-workflow.md)为准。

## 调研范围与证据

固定版本，逐个追踪 workflow 调用的 actions、脚本与下游发布：

- Magpie：`6d154ef1e1fdad522b88ea617dc2e2f32eeeea38`；5 个提交内 workflow、独立签名发布仓库、4 个 PR 的多轮 review、Issue 样本与公开 ruleset。见[完整研究](workflow-research/magpie.md)。
- T3 Code：`5ef56853ef1854da50bc1909c6c0a32700f2cb13`；24 个 workflow、共享变更检测器、贡献准入与 triage、模板、review bot、发布链、4 个 PR 与公开 ruleset。见[完整研究](workflow-research/t3code.md)。

分别记录仓库里的声明、可执行检查、实际 PR/run 的行为、GitHub 服务端设置。
以下结论来自有限样本，不推断所有历史贡献都遵守同一流程，也不把未读到的私有设置当成不存在。

## 两种组织方式中值得保留的逻辑

Magpie 按 Go 子系统组织代码，每个子系统的契约有唯一文档，Makefile 统一命令。
它的 review 会追踪真实输入、调用链、当前主干与准确 head；通过故意恢复旧行为确认回归测试有用。
GUI 浏览器验证与多平台 Go CI 是不同证据；发出的二进制还要在目标环境启动验证。
这些做法适合 PoBR 的稳定 ID、JSON 契约、数据快照与真实 WASM 边界。
参见其[子系统约定](https://github.com/yetone/magpie/blob/6d154ef1e1fdad522b88ea617dc2e2f32eeeea38/docs/subsystems/README.md)和 [PR #1294](https://github.com/yetone/magpie/pull/1294)。

T3 按应用、共享包、基础设施、native、维护脚本分层；本地和 CI 共用受影响包与反向依赖图。
无法可靠判断影响时扩大验证；统一 `Check` 只接受检测器明确解释的跳过。
验证流程与拥有写权限的报告/发布流程分开，并对 artifact、run 和 SHA 做关联。
适合迁移的是边界与判定逻辑，而不是目录名称、八个服务端分片、付费 runner 或移动平台矩阵。
参见[变更检测器](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.github/scripts/ci-changes.cjs)和 [CI](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/.github/workflows/ci.yml)。

PoBR 已有清楚的 Cargo 依赖方向，不需要为 CI 改动搬迁产品代码。需要梳理的是：
`mise.toml` / `devs/scripts` 负责命令，workflow 负责事件、环境和权限，
`docs/development-workflow.md` 负责操作，维护者指南负责跨主题约束，模板负责收集证据。
研究和过往事故保存在有日期的记录中，避免热路径文档不断膨胀。

## 不直接迁移的上游习惯

- Magpie 没有提交内 Issue/PR 模板；不能把优质 Issue 的内容描述成已有模板。
- Magpie 文档要求发布前 Test 通过，但源仓库和下游 release workflow 没有实现这条自动依赖。应把关键门禁写进执行图。
- Magpie 的预览流程虽隔离检测、执行、发布，却把 DeepSeek 密钥交给 PR 代码。PoBR 无需引入这个成本或权限模型。
- T3 的 VOUCHED、贡献准入与模型限制来自其维护容量，不能当成通用的 review 流程。
- T3 的公开 ruleset 仍要求八个独立状态，与 workflow 注释所述单一 `Check` 不一致；人工批准要求为零。不能从 bot APPROVED 推断强制人工 review。
- Magpie 的公开 main ruleset 只限制删除与强推。文档里的 review 习惯不等于 GitHub 已强制执行。
- Magpie [PR #1354](https://github.com/yetone/magpie/pull/1354) 在多轮技术验证后仍因用户不接受布局而回滚。测试和录屏不能代替产品方向的验收。

依据：[Magpie 预览说明](https://github.com/yetone/magpie/blob/6d154ef1e1fdad522b88ea617dc2e2f32eeeea38/.github/ui-preview/README.md)、
[Magpie ruleset](https://github.com/yetone/magpie/rules/24232762)、
[T3 ruleset](https://github.com/pingdotgg/t3code/rules/13856153)、
[T3 贡献约定](https://github.com/pingdotgg/t3code/blob/5ef56853ef1854da50bc1909c6c0a32700f2cb13/CONTRIBUTING.md)。
服务端规则链接可变，以上描述对应研究日的只读观察。

## 凝结后的判断顺序

1. 先明确行为、契约、维护者和权威来源。
2. 再按变更与依赖决定需要的检查，成本不同的项目可以选不同层次。
3. PR 记录问题、结果、验证和限制；review 绑定最终版本，区分阻断问题与建议。
4. 用稳定的汇总状态表达“应该执行的检查都通过了”。检测器失败、取消或意外跳过都不能成为通过。
5. 通过独立权限发布同一次验证过的产物；版本、SHA、校验和与部署渠道能追溯。
6. 实际失败反哺一条回归断言或一处当前契约，避免为假想风险叠加机器人和流程。

贡献准入、是否可以安全执行代码、代码正确性、谁有权合并，分别判断。
Issue 是诊断证据，未实现功能清单不自动构成开发任务；这与 PoBR 的静态模拟范围保持一致。

## PoBR 基线中的具体断点

- `pr.yml` 在所有事件中只比较 `HEAD^1..HEAD`，可能漏掉多提交 push 前面的代码。
- workspace 升版使 master 的 Rust/Web 检查都跳过，但原完整 CI 不响应 master push。
  [run 40900695778](https://github.com/ackness/pobr/actions/runs/40900695778) 确认跳过行为；该次 release PR 已通过完整检查，不能据此断言该版未验证。
- 没有稳定的汇总状态；只改 `pipeline/` 也没有相关自动检查。
- PR 与功能 Issue 模板要求 PoB2 Lua file:line，与游戏数据优先的贡献指南冲突。
- PR 模板要求每次本地 parity，与定向验证预算冲突。
- `v0.*` 包含 RC，旧发布流程会部署 RC 到生产；不同 tag 并发还可能发生旧版覆盖新版。
- `sort -V` 不满足 SemVer 的 RC→stable 顺序；远端查询失败当作 tag 不存在；锁文件只检查 CLI。
- 本地与 CI 的 wasm-pack 版本来源不一致；Dependabot 没有覆盖 Web 依赖。
- 当日 master protection 没有 required checks/reviews，rulesets 为空。YAML 修复只能提供检查，不能宣称合并规则已经启用。

## 可复用 skill

全局 `repo-workflow` skill（个人 skills 目录） 保存判断流程与分主题参考，
不依赖 PoBR 的包名、编译器、runner 或外部机器人。入口只加载通用决策，
需要时才读上游案例、迁移检查。通用 skill 独立安装于 `~/.codex/skills/repo-workflow`；本仓库只维护
[PoBR workflow 适配](../../.claude/skills/workflow/SKILL.md)，不复制通用决策。

适配结果与当前检查责任、PR/review/Issue 生命周期、发布操作及 hosted activation，
集中维护在[开发工作流](../development-workflow.md)，而不把本次研究当作第二本操作手册。

## 本次适配与验证结果

- 全局通用 skill 独立安装于 `~/.codex/skills/repo-workflow`；项目只保存薄的 `workflow` adapter。共享流程不绑定委派工具，具体调用交给当前场景与所选工具。
- 默认流程保持轻：PR 模板是说明提示，没有固定审计 JSON、额外批准次数或每个改动必须创建独立 reviewer。复杂或高风险改动才扩大验证与 review。
- `PR gate` 只校验真实的应执行检查；可选 `pr-ready` 辅助核对当前 head/checks，并输出带 `--match-head-commit` 的命令，不自动合并。
- 自动路由覆盖整个 push、重命名两侧、未知输入和 Web 文本 fixture；完整验证可复用，验证 jobs 无发布密钥。
- 发布按 SemVer、继承锁版本、准确 tag/commit 和同次产物验证；上传前重验远端 tag，生产部署在串行队列中再次检查最高稳定 v0 版本。
- 本地 mise 升至当前稳定版 2026.10.23；项目精确工具/下载锁统一，CI 的原生 Action 使用同一份版本，并经官方发布 API 核对最新稳定标签。Actions 更新每日分组，其余依赖按月维护。
- 日常 setup 不编译产品或默认获取大参考数据；oracle、浏览器和图像工具按任务自动准备。实际首次安装发现并修复了 Homebrew PATH 覆盖 pnpm、固定 LuaJIT 的 macOS deployment target，以及缓存 runtime 的 alias 问题。
- 已实跑：首次与重复 setup、无个人工具配置的版本核验、oracle 的固定 LuaJIT/JSON、自动化契约测试、Rust 局部测试、Web 定向测试、pipeline 测试、Chromium 启动、固定图像工具的 DDS→WebP 转换和官方 Linux gh 资产校验。
- 通用 skill 在微型 Python 项目上前向测试：去掉不必要的选择器，保留一个无条件快速测试 job；正常测试通过，故意破坏实现时失败。说明同一逻辑可以导出更简单的流程。
- 独立审查发现的导出依赖、移动 tag、文本 fixture、CI gh/Node 对齐问题均已作窄修正与验证。没有为流程形式引入额外服务或调度器。
- 未推送、发起 PR、修改 GitHub 服务端规则或发布；真实 hosted CI 与 required-check activation 仍需首次上线确认。未跑完整应用 E2E 或产品全量构建；本次没有改动产品行为。
