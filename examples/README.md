# 构筑样本与回归输入

这里保留仍有明确读者的构筑、来源记录和对照身份。临时抓取、诊断输出和个人试验放在
忽略的 `.cache/` 中；新增样本时说明使用者，不按目录日期将历史回归输入当作过期数据。

| 目录 | 用途 |
| --- | --- |
| [demo-bd-test](demo-bd-test/README.md) | 固定 parity、导入／保存和 golden 回归；原始抓取记录及维护工具一并保留 |
| [wasm-benchmark](wasm-benchmark/) | 八个固定 WASM 性能和请求样本 |
| [reference-builds](reference-builds/README.md) | tuned 对照、仅测量的 holdout、扩展性能语料和抓取选择历史 |
| [poe-ninja](poe-ninja/README.md) | 仍被节点优化和浏览器回归直接读取的单个构筑样本 |
| `build-file/` | `.build` 文件导入、身份和保存回归 |

`reference-builds/ninja-top10-2026-10-26` 仍用于扩展性能、节点共享和存档回归。
`batch-2026-10-27` 的来源元数据参与后续抓取去重。二者都保留现有选择；
holdout 只用于测量，不用于 triage、诊断或指导修复。

样本中的版本和参考值保持原始身份。数据升级后的重新采集与基线变更按
[验证流程](../docs/development-workflow.md)执行。
