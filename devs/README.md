# 开发与验证工具

本目录只保留开发命令、固定基线、测试夹具和参考来源声明。
命令从仓库根目录通过 [mise](../mise.toml) 执行；用法和验证范围见
[开发与 CI 工作流](../docs/development-workflow.md)。

| 路径 | 职责 |
| --- | --- |
| `scripts/` | mise driver、CI／发布策略、工具安装、覆盖率与诊断、样本抓取及其测试 |
| `hooks/` | 可选的 pre-push 本地检查；安装与跳过方法见开发工作流 |
| `ci/` | 解析覆盖率和 WASM 包大小的固定基线 |
| `fixtures/` | sync catalog parity 的受控测试输入 |
| [reference-sources.json](reference-sources.json) | 独立固定 PoB2 测试参考与 LuaJIT 来源 |

运行生成的审计、性能、诊断和抓取结果放在忽略的 `.cache/`，不在 `devs/` 新建报告目录。
例如 vendor 差异报告默认写入 `.cache/vendor-delta/reports/`。
