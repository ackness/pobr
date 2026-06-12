# perform_bench 基线流程（M4 T0/W-F1）

> 预算（蓝图 m4-offence-deep.md §2-T0）：M4 结束时 `perform_attack_flicker`/
> `perform_spell_comet` 耗时 ≤ 本文记录基线的 **2.5×**；traced/归因路径 ≤4×
> （超出记录不阻塞，归因非热路径）。超预算 = T2 必须做惰性短路
> （非双持跳 OffHand pass、无暴击词条短路 crit pass）且短路带等价性测试。

## 操作

```bash
# 记录/对比基线（criterion 自动与上次运行对比）
cargo bench -p pobr-build --bench perform_bench
cargo bench -p pobr-core  --bench mod_db_bench   # 聚合内核吞吐不得回归

# criterion 历史数据在 target/criterion/<case>/，多 worktree 场景注意
# CARGO_TARGET_DIR 隔离会让基线分散——M4 门禁对比一律在主仓库默认 target 跑。
```

CI 不跑 criterion（时长）；门禁为合并前手动跑 + 结果贴 PR。

## M4 起点基线（2026-06-12，master，commit 见 git log 本文件引入点）

| case | 基线（中位） | 2.5× 预算上限 |
|------|--------------|----------------|
| perform_attack_flicker | **2.0951 ms** | 5.24 ms |
| perform_spell_comet | **1.8758 ms** | 4.69 ms |
| mod_db_bench::sum_inc/sum_base/more @5000 | 9.13µs / 9.59µs / 15.25µs（M3-T5 实测） | 无回归 |

## 语料偏离说明

蓝图要求「1 个双持攻击 build」；ninja 18-build 集中无严格双持——以
monk-martial-artist-flicker-strike（攻击主路径）+ sorceress-stormweaver-comet
（法术对照，隔离 crit pass 贡献）替代；W-B2 双持 fixture 落地后补真双持 case。
