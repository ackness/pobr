# 可选 Rust 性能测量

仅在调查性能或验证性能改动时运行，不是每个 PR 的固定门槛：

```bash
mise exec -- cargo bench -p pobr-build --features benchmarks --bench perform_bench
mise exec -- cargo bench -p pobr-core --features benchmarks --bench mod_db_bench
```

Criterion 将同一 checkout 的历史结果保存在 `target/criterion/`。比较时保留相同工具链、
profile、features、输入和机器条件；计时与编译分开观察，不为测量清空构建缓存。
行为保持的优化还应使用相关断言或完整输出比较，耗时下降不能证明结果等价。

真实 WASM 与构筑语料测量见[开发工作流](../../docs/development-workflow.md#慢构建与按需报告)，
包体积测量见[WASM 包](../../docs/wasm-package.md)。原始样本和本机报告保存在忽略的 `.cache/`。
