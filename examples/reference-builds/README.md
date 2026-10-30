# reference-builds —— 用来衡量准确度的当前赛季构筑

每个构筑码自带导出工具当时算出的 `<PlayerStat>`，主技能召唤物的构筑还带 `<MinionStat>`。`mise run accuracy` 把 PoBR 的结果与这些参考值逐行比较（5% 以内算吻合），不需要 vendor 或 luajit。参考值不是真值：导出工具的版本未知，PoB2 自身也有假设和缺陷；不一致的行是线索，由游戏自己的词条文本和数据判断谁对。

## 两组构筑，两种用途

| 目录 | 用途 | 规则 |
|------|------|------|
| `tuned/` | 找差异、修引擎。`mise run accuracy examples/reference-builds/tuned/builds` 看每个构筑的差异，`mise run diagnose <构筑>` 看丢掉的输入，`mise run compare <构筑>` 与钉定的 PoB2 逐个 stat 对照 | 修完后上调 `reference_report.rs` 里的基线 |
| `holdout/` | 只测量。它的总数回答“没人调过的构筑上 PoBR 有多准” | 不对其中的构筑运行 `diagnose` 或 `compare`，不按它的差异挑选要修的内容；只看 `mise run accuracy` 的总数 |

`examples/wasm-benchmark/builds` 的 8 个构筑同样已被逐个修过，属于前一类。`tuned/selection-holdout-2026-10-13.json` 与 `tuned/selection-holdout-2026-10-15.json` 是并入 `tuned/` 的两批 holdout 的抓取记录。

`ninja-top10-2026-10-26/` 是额外的 Forbidden Rites 活动服样本：固定快照 `0757-20261026-16736`，按网站默认等级降序保留每个升华的前十行及同级原顺序，共 230 个码。它使用排名选择，与下文 `builds:fetch` 的账号排序不同。`build-codes.json` 是码合集，`index.csv` 和 `selection.json` 记录来源，`validation.json` 记录完整性校验。本批已用于引擎修复，不是独立 holdout，也没有加入默认门禁语料。运行 `mise run accuracy examples/reference-builds/ninja-top10-2026-10-26/builds` 可复测整批；默认模式与参考模式应分开测量，两者都不是游戏实测真值。


`batch-2026-10-27/` 保存了抓取脚本联网核验时取得的 23 个新构筑，来自 Forbidden Rites 普通服快照 `1537-20261027-28463`。按账号和角色名排序、排除已有角色后，每个升华取第一个；[selection.json](batch-2026-10-27/selection.json) 记录来源和选择，[validation.json](batch-2026-10-27/validation.json) 记录构筑码完整性校验。这批样本尚未做计算准确度评估，也未加入默认门禁语料。

一旦按 `holdout/` 里某个构筑的差异改了引擎，这组数字就不再说明“没调过的构筑”。需要新一轮目标时，把 `holdout/` 的构筑移入 `tuned/`，再抓一批新的作 `holdout/`。

## 抓取

```bash
mise run builds:fetch examples/reference-builds/tuned --pick first
mise run builds:fetch examples/reference-builds/holdout --pick last
```

每个升华取一个角色：poe.ninja 最新赛季该升华的索引行按（账号, 角色名）排序，去掉已入库的角色，取第一个或最后一个。选择不依赖任何计算结果。导出不可用的角色记为失败，不替换。`selection.json` 记录赛季、快照和每个候选；`--replay` 按它重新抓取（快照被 poe.ninja 回收后会失败）。

构筑目录的格式与 `examples/wasm-benchmark/builds` 相同：`code.txt`（PoB2 构筑码）和 `meta.json`（来源、职业、主技能、码的哈希）。

## 结果与维护

当前门禁基线以 `crates/pobr-build/tests/parity/reference_report.rs` 为准，
已确认的参考差异见 [PoB2 差异说明](../../docs/pob2-differences.md)。
提高基线时，在提交或 PR 中说明改动原因、所用数据与实际检查。
生成的 triage、日志和一次调查的数值报告放在 `.cache/`；样本来源和可复现输入继续入库。
