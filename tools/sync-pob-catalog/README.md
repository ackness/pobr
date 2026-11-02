# sync-pob-catalog

PoB2 显示／输出 catalog 的测试参考工具：扫描固定版本 vendor PoB2 的核心 Lua 文件，
与 PoBR 的显示 catalog 或已提交的 fixture 对照。它不生成运行数据；活动数据中由 PoB2 转换的
overlay 以固定字节随 RePoE owner 包导入，见[数据管线](../../pipeline/README.md)。
在仓库根运行，先用 `mise run vendor` 准备固定 vendor。

## 子命令（scan / check / diff / fixture-check）

从 PoB2 核心 Lua 文件（`BuildDisplayStats.lua` / `CalcSections.lua` / `Calc*.lua`）
抽取 output/display/breakdown catalog，并对已提交的 fixture 做 drift 检查：

```bash
mise exec -- cargo run -p sync-pob-catalog -- scan  --pob-root <PoB2路径> [--out catalog.json]
mise exec -- cargo run -p sync-pob-catalog -- check --pob-root <PoB2路径> --catalog catalog.json
mise exec -- cargo run -p sync-pob-catalog -- diff  --pob-root <PoB2路径> --catalog catalog.json
mise exec -- cargo run -p sync-pob-catalog -- fixture-check --pob-root <PoB2路径> --catalog <fixture.json>
```

`check` 只报告是否一致；`diff` 与 `fixture-check` 逐项列出差异，有差异时以非零状态退出。

## 测试目标

- `catalog`：catalog 扫描，以及 `special_mods.json`（共享层与版本层）的正则编译检查。
- `gate`：catalog 差异与自包含 fixture 往返、显示 catalog 的 parity 门禁。
