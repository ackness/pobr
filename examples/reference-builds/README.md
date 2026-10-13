# reference-builds —— 用来衡量准确度的当前赛季构筑

每个构筑码自带导出工具当时算出的 `<PlayerStat>`。`mise run accuracy` 把 PoBR 的结果与这些参考值逐行比较（5% 以内算吻合），不需要 vendor 或 luajit。参考值不是真值：导出工具的版本未知，PoB2 自身也有假设和缺陷；不一致的行是线索，由游戏自己的词条文本和数据判断谁对。

## 两组构筑，两种用途

| 目录 | 用途 | 规则 |
|------|------|------|
| `tuned/` | 找差异、修引擎。`mise run accuracy examples/reference-builds/tuned/builds` 看每个构筑的差异，`mise run diagnose <构筑>` 看丢掉的输入 | 修完后上调 `reference_report.rs` 里的基线 |
| `holdout/` | 只测量。它的总数回答“没人调过的构筑上 PoBR 有多准” | 不对其中的构筑运行 `diagnose`，不按它的差异挑选要修的内容；只看 `mise run accuracy` 的总数 |

`examples/wasm-benchmark/builds` 的 8 个构筑同样已被逐个修过，属于前一类。

一旦按 `holdout/` 里某个构筑的差异改了引擎，这组数字就不再说明“没调过的构筑”。需要新一轮目标时，把 `holdout/` 的构筑移入 `tuned/`，再抓一批新的作 `holdout/`。

## 抓取

```bash
mise run builds:fetch examples/reference-builds/tuned --pick first
mise run builds:fetch examples/reference-builds/holdout --pick last
```

每个升华取一个角色：poe.ninja 最新赛季该升华的索引行按（账号, 角色名）排序，去掉已入库的角色，取第一个或最后一个。选择不依赖任何计算结果。导出不可用的角色记为失败，不替换。`selection.json` 记录赛季、快照和每个候选；`--replay` 按它重新抓取（快照被 poe.ninja 回收后会失败）。

构筑目录的格式与 `examples/wasm-benchmark/builds` 相同：`code.txt`（PoB2 构筑码）和 `meta.json`（来源、职业、主技能、码的哈希）。

## 记录

| 日期 | 组 | 构筑数 | `TotalDPS` 吻合 | 吻合行 |
|------|----|--------|-----------------|--------|
| 2026-10-13（抓取时，未做任何针对性修改） | `tuned/` | 23 | 11（48%） | 796/1061（75.0%） |
| 2026-10-13（抓取时，未做任何针对性修改） | `holdout/` | 23 | 7（30%） | 844/1073（78.7%） |
| 2026-10-14（第一轮：按 `tuned/` 的差异修引擎） | `tuned/` | 23 | 14（61%） | 905/1061（85.3%） |
| 2026-10-14（同一批改动，未查看其中任何构筑） | `holdout/` | 23 | 11（48%） | 949/1073（88.4%） |

吻合行里有按游戏规则有意不同于参考值的行（例如护身符授予的技能不保留精魄，见 `reference_report.rs` 基线旁的说明），所以上限不是 100%。
