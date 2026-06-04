# 性能策略

---

## 1. 优化顺序

性能优化服务计算正确性。早期实现优先选择清晰、可测试的数据结构；当 benchmark 显示瓶颈后再做专项优化。

推荐顺序：

1. 建立 benchmark。
2. 确认 hot path。
3. 写最小优化实验。
4. 对比正确性和性能。
5. 合并可维护的优化。

---

## 2. 重点 Hot Path

| 区域 | 风险 | 观测方式 |
|------|------|----------|
| ModDB 查询 | `sum`/`more`/`flag` 高频调用 | `criterion` + flamegraph |
| Modifier matching | condition/tag/flag 判断复杂 | per-tag benchmark |
| Damage conversion | 递归和重复计算 | conversion fixture |
| Active skill setup | support gem 组合多 | skill setup benchmark |
| Minion/trigger | 多 actor、多 skill | full build benchmark |
| Breakdown | 详细输出可能增加分配 | allocation profile |

---

## 3. ModDB 优化路线

第一版：

- `HashMap<ModName, Vec<Modifier>>`
- 清晰的 `Modifier::matches`
- 单元测试覆盖语义

第二版：

- 按 `ModType` 分桶。
- 按 `ModFlags` / `KeywordFlags` 做快速预筛。
- 将常见 `CalcConfig` 查询缓存为短生命周期只读 query context。

第三版：

- compact arena 或 SoA。
- SIMD 批量过滤。
- 预编译 tag matcher。

每一版都需要保持相同测试和 golden 输出。

---

## 4. 并行计算边界

并行只发生在只读快照阶段。

可并行：

- 多主动技能的 offence 计算。
- 多召唤物 actor 计算。
- 多配置/多 variant 对比。
- UI 中的候选装备批量评分。

串行：

- Env 初始化。
- condition 写入。
- charge/reservation 等顺序敏感阶段。
- breakdown 合并。

---

## 5. Benchmark 目录

```
crates/pobr-core/benches/
├── mod_db.rs
├── mod_parser.rs
├── calc_minimal.rs
├── active_skill.rs
└── full_build.rs
```

基础命令：

```bash
cargo bench -p pobr-core
cargo bench -p pobr-build
```

---

## 6. 性能验收

早期目标：

- ModDB hot query 有稳定 benchmark。
- 最小计算闭环有稳定 benchmark。
- 完整 fixture build 有端到端耗时记录。

中期目标：

- 常见完整 build 计算低于 50ms。
- 多技能/召唤物场景能用只读快照并行。
- golden regression 与 benchmark 同时运行。

长期目标：

- 支持 UI 实时预览。
- 支持批量装备评分。
- 支持 WASM 环境稳定运行。
