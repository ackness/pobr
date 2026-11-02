# 宝石品质：目标事实生产与兼容范围

`overlay/gem_quality_stats.json` 由 `pobr-data-adapter --ordinary-domain gem-quality` 从固定的
RePoE 目标事实（`GrantedEffectQualityStats`、`GrantedEffects`、`Stats`）和本目录的
[`0.5.5.5-facts-policy.json`](0.5.5.5-facts-policy.json) 生成。保留历史文件路径、`GemQualityStatsDef` schema 和
加载后的 patch 行为；`overlay/` 不表示该文件来自 vendor Lua。
命令、输入校验和收据见 [ordinary quality](../../../tools/pobr-data-adapter/ordinary-quality.md)。

adapter 先检查外部 manifest 与政策的 SHA-256，再校验必需列、外键、数组长度及完整效果范围；
全部通过后才写入新的输出目录。校验失败不写出品质文件，也不会用空表或旧版数据兜底。
相同输入与政策重复生成，完整产物字节一致。它不需要 vendor、LuaJIT、网络或旧品质产物。

## 映射与保留的计算边界

| 输入 | 输出 / 行为 |
|------|-------------|
| `GrantedEffect` | 经 `GrantedEffects._index → Id` 转为稳定 effect ID |
| `Stats` / `AltStats` | 经 `Stats._index → Id` 转为稳定 stat ID |
| `StatsValuesPermille` / `AltStatValuesPermille` | 除以 1000 得到每 1% 品质的斜率 |
| 主品质 / 备用品质 | 主品质在前，备用品质在后并标记 `alt: true`；各自顺序与重复 stat 均保留 |
| `ApplyToStatSets` / `AltApplyToStatSets` | 非空值保留在 `_meta.unapplied_stat_set_scopes`；不新增运行时作用域语义 |

运行时继续按现有逻辑对每条斜率乘品质后向零截断，再相加。
主 / 备用品质仍由已有消费者选择；备用品质已参与部分保留计算，不能丢弃，
也不能据此宣称所有备用品质机制都已支持。

政策固定 **410 个启用效果**，与迁移时的旧产物一致。目标事实另有 19 个品质效果，
均未被对照 vendor 的九份技能模板导出；具体 ID 与待验证理由列在政策的
`compatibility.excluded_effects`，这不表示游戏中不可用。
不能用 `IsSupport` 一刀切筛选：当前范围包含四个该字段为真的非宝石效果。
作用域语义、新增效果或删除效果都需要单独的机制核对与计算回归。

## 来源与参考

政策的 `reference` 记录迁移对照用的旧产物与其来源收据摘要，`compatibility` 记录对照 vendor 提交
`ce566eac45ea8a86477f513c7ee65a1ebe60014e` 和旧 Lua 产物的 SHA-256。这些只是迁移对照证据，
路径按记录时的 Git 版本解释。政策是维护者审查记录，不是上游签名；adapter 验证输入与政策一致，
不会联网重新证明来源。

## 更新版本

当前品质事实由独立 RePoE 提供，PoBR 保留主／备用属性、数值与作用域的解释和消费。
更新来源先在 RePoE 固定补丁、schema、完整原值与证据，再更新 PoBR 声明输入和品质政策：

1. 核对旧版与新事实的效果、属性、单位、主 / 备用品质和作用域差异。
   明确每个效果的启用或待验证决定；不要仅修改哈希来消除校验错误。
2. 新建对应补丁的政策文件，固定目标 manifest 与核验后的范围。
   保持行为的迁移可保留原对照；机制变更应记录新的对照和回归依据。
3. 输出到新的 `.cache/` 目录，检查有序载荷差异与确定性，
   跑 adapter、品质 / 保留和相关完整 Build 回归。
4. 完成完整数据更新、词条审计和项目要求的检查后，再通过[锁驱动候选与显式采用](../../runtime/locked-update.md)推进活动版本。

新效果、删除效果或 scope 变化必须有明确的政策决定；不同补丁的旧事实不能重新标记为当前来源。

## 定向验证

```bash
mise exec -- cargo test -p pobr-data-adapter --bin pobr-data-adapter skills::quality::
mise exec -- cargo test -p pobr-data-adapter --bin pobr-data-adapter ordinary_facts::quality::
mise exec -- cargo test -p pobr-build --test skills gem_quality
mise exec -- cargo test -p pobr-build --test skills spirit_reservation::
mise exec -- cargo test -p pobr-wasm --test contract_golden memory_backend_matches_dir_backend
```

主机契约测试覆盖历史和活动数据版本、三份完整 Build 的 q0/q19/q20，比较文件和内存加载。
涉及加载或计算边界的改动，再比较原生与真实 WASM 的相同输入；无序诊断可排序，数值比较不放宽。
Web 回归用爆炸手雷验证 q0 → q20 导致重算、再改回 q0 恢复原值：

```bash
mise run web:e2e e2e/build-roundtrip.spec.ts -g 'gem quality edits' --workers=1
```

离线测试使用合成版本号和重排索引，覆盖数值更新、未知机制拒绝和来源不匹配；
它们不代表已验收尚未发布的真实补丁。
