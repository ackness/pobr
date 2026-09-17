# 宝石品质：官方表生产与兼容范围

从 `4.5.5.2` 开始，`overlay/gem_quality_stats.json` 由 `pobr-data-adapter`
直接读取官方表导出的 JSON 生成。保留历史文件路径、`GemQualityStatsDef` schema 和
加载后的 patch 行为；`overlay/` 不再表示该文件来自 vendor Lua。

## 独立生成

在仓库根目录运行：

```bash
cargo run -p pobr-data-adapter -- \
  --gem-quality pipeline/tables/English \
  --quality-source "pipeline/gem-quality/$(cat data/CURRENT).json" \
  --out data --patch "$(cat data/CURRENT)"
```

输入只有 `GrantedEffectQualityStats.json`、`GrantedEffects.json`、`Stats.json`
与受审的版本收据。编译好的 adapter 不需要 vendor、LuaJIT、网络或旧品质产物。
首次 Cargo 构建仍需要已缓存的 Rust 依赖，或联网下载依赖。

收据固定游戏版本、三张输入表的 SHA-256、来源证据和效果适用性决定。
adapter 先检查版本和输入字节，再校验必需列、外键、数组长度及完整效果范围；
全部通过后，将完整 JSON 写入同目录临时文件并替换目标文件。
校验失败不会改动已有品质文件，也不会用空表或旧版数据兜底。
相同输入与收据重复生成，完整产物字节一致。

`pipeline/regen-all.sh` 在其他生成步骤之前执行这个入口，品质域只有一个日常生产者。
`devs/scripts/regen-check.sh` 有收据与本地品质表时会在临时目录重生成并比较；
缺少输入时会明确打印该域跳过。其他数据域仍保留各自的生成和失败策略。
历史版本的已提交文件继续可用，不要求为它们补造官方来源收据。

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

首次迁移固定为旧产物的 **410 个效果、691 条有序属性**。
原始表另有 19 个效果，均未被对照 vendor 的九份技能模板导出。
具体 ID 与待验证理由列在 `4.5.5.2.json` 的 `excluded_effects`；这不表示游戏中不可用。
不能用 `IsSupport` 一刀切筛选：当前范围包含四个该字段为真的非宝石效果。
作用域语义、新增效果或删除效果都需要单独的机制核对与计算回归。

## 首次来源核对

[4.5.5.2.json](4.5.5.2.json) 记录：

- `pathofexile-dat@15.2.0` 与实际使用的社区 schema 内容指纹。
- 从缓存索引定位、重导出的三张表与输入 JSON 逐字节相同；涉及的三个数据 bundle
  与 `4.5.5.2` CDN 字节相同。索引只记录本地缓存指纹，没有声称完成 CDN 比对。
- 原 Lua 产物的完整 SHA-256 与 vendor 提交
  `ce566eac45ea8a86477f513c7ee65a1ebe60014e`，仅作为迁移对照证据。

收据是维护者审查记录，不是上游签名。adapter 验证输入与收据一致，不会联网重新证明来源。
schema 的 `latest` URL 会变化；精确重放应使用按收据指纹保存的 schema、bundle 和表，
不能仅因下载地址或目录名相同就更新指纹。完整原始输入沿用本地缓存方式，不进入仓库。

## 更新版本或原始表

通常运行 `bash pipeline/bump-version.sh` 即可。下载器成功导出官方表后，
`advance-receipt.py record-export` 记录目标补丁、配置指纹和三张表的 SHA-256；
仅目标版本还没有收据时，`advance` 核对上一版收据与品质产物，并以稳定 ID 比较语义。
效果集合、主 / 备用属性 ID 及顺序、stat-set 作用域不变时，数值斜率可自动更新，
表内 `_index` 重排也无需修改代码。原有启用 / 排除决定和原始对照依据保持不变。
新收据注明它证明成功的 exporter 执行与输入字节绑定，不冒充首次迁移的独立 CDN 字节比对。

新效果、效果删除、属性身份或作用域变化会停止自动推进，避免未知机制静默参与计算。
已有受审收据不会被覆盖；`--skip-download` 复用缓存时仍必须通过补丁和输入字节核验。
需要新兼容决定或同版本输入变化时：

1. 从目标补丁导出表，记录 exporter、实际 schema、bundle 与表的指纹。
   重导出时使用独立目录，避免 exporter 清空现有 `tables/English` 后中途失败。
2. 核对旧版与新表的效果、属性、单位、主 / 备用品质和作用域差异。
   明确每个效果的启用或待验证决定；不要仅修改哈希来消除校验错误。
3. 新建 `pipeline/gem-quality/<patch>.json`，固定核验后的来源与范围。
   保持行为的迁移可保留原对照；机制变更应记录新的对照和回归依据。
4. 先用 `--out .cache/gem-quality-review` 生成，检查有序载荷差异与确定性，
   跑 adapter、品质 / 保留和相关完整 Build 回归，再更新版本产物。
5. 完成完整数据更新、词条审计和项目要求的检查后，再通过现有流程推进活动版本。

直接运行 `regen-all.sh` 仍要求已有匹配收据，不负责下载或推断来源。
自动兼容检查未通过时，准备并验证收据后，可使用
`bash pipeline/bump-version.sh --patch <patch> --skip-download` 重跑。
原始表必须已经对应目标版本，不能将上一版输入重新标记为新版。
此门禁只保证品质域的输入与输出；不使整个升级脚本成为跨域事务。

## 定向验证

```bash
cargo test -p pobr-data-adapter --bin pobr-data-adapter skills::quality::
cargo test -p pobr-build --test skills gem_quality
cargo test -p pobr-build --test skills spirit_reservation::
cargo test -p pobr-wasm --test contract_golden memory_backend_matches_dir_backend
python3 devs/scripts/test_workflows.py
python3 pipeline/test-data-update.py
node --test pipeline/test-query-patch-version.mjs
```

主机契约测试覆盖历史和活动数据版本、三份完整 Build 的 q0/q19/q20，比较文件和内存加载。
首次迁移已重建真实 WASM，并对三份 Build 的 q0/q19/q20 共九组完整输出进行检查：
迁移前后、原生与 WASM 均相等（只对无序诊断列表排序，不放宽数值比较）。
Web 回归用爆炸手雷验证 q0 → q20 导致重算、再改回 q0 恢复原值：

```bash
pnpm --dir web exec playwright test e2e/build-roundtrip.spec.ts -g 'gem quality edits' --workers=1
```

首次真实数据验收覆盖 `4.5.5.2`。后续离线测试使用未出现过的版本号和重排索引，
覆盖数值更新、未知机制拒绝、来源不匹配以及失败不推进活动版本；它们不代表已验收尚未发布的真实补丁。
Lua 提取器保留为历史重放 / 对照工具；对照时输出到临时目录，不覆盖当前正式产物。
