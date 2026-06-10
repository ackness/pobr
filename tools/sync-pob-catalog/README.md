# sync-pob-catalog

从 vendor PoB2（Lua）抽取数据并做 parity 检查的离线工具。两组子命令：

## catalog 系（scan / check / diff / fixture-check）

从 PoB2 核心 Lua 文件（`BuildDisplayStats.lua` / `CalcSections.lua` / `Calc*.lua`）
抽取 output/display/breakdown catalog，并对已入库 fixture 做 drift 检查：

```bash
cargo run -p sync-pob-catalog -- scan  --pob-root <PoB2路径> [--out catalog.json]
cargo run -p sync-pob-catalog -- check --pob-root <PoB2路径> --catalog catalog.json
cargo run -p sync-pob-catalog -- diff  --pob-root <PoB2路径> --catalog catalog.json
```

## extract-lua（overlay 抽取通道，架构裁决 P13）

用 luajit 在**最小 stub 环境**下执行 vendor 的 Lua 数据文件（不全量启动 PoB2，
只 stub `SkillType`/`ModFlag`/`KeywordFlag` 与 `mod`/`flag`/`skill` 注入函数），
把"不在 GGG .dat、由 PoB2 手工 Lua 维护"的人工策展层固化为确定性 JSON，落到
`data/<版本>/overlay/`。这替代了以前"绕过适配器手改产物 JSON"的一次性补丁
（15-data-pipeline Gap3）：适配器重跑不再丢失这些值。

```bash
# 默认抽取 act_dex/act_int/act_str 三个玩家技能文件，输出到 stdout
cargo run -p sync-pob-catalog -- extract-lua --vendor-root vendor/PathOfBuilding-PoE2/src

# 固化（再生成）入库 overlay 文件
cargo run -p sync-pob-catalog -- extract-lua \
    --vendor-root vendor/PathOfBuilding-PoE2/src \
    --out data/4.5.0.3.4/overlay/skill_overrides.json
```

参数：

| 参数 | 说明 |
|------|------|
| `--vendor-root <path>` | vendor PoB2 的 `src/` 目录（只读输入，必填） |
| `--out <path>` | 输出文件；缺省写 stdout |
| `--files <a,b,c>` | 抽取的 `Data/Skills/<name>.lua` 列表；缺省 `act_dex,act_int,act_str` |
| `--luajit <path>` | luajit 路径；缺省依次取 `POBR_LUAJIT` 环境变量、`/opt/homebrew/bin/luajit`、PATH |
| `--version-file <path>` | vendor 版本记录文件；缺省 `<vendor-root>/../../.pob2-version.txt` |

### 产物：`overlay/skill_overrides.json`

当前覆盖三类 per-skill 值（schema `skill_overrides/v1`，后续按需扩列）：

| stat | vendor 来源 |
|------|-------------|
| `crit_chance` | `levels[*].critChance` |
| `attack_speed_multiplier` | `levels[*].attackSpeedMultiplier` |
| `skill_attack_speed_more` | `statSets[*].baseMods` 中 `mod("Speed", "MORE", <n>, ...)`（如 Flicker Strike 的 285） |

条目形如 `{skill, stat, stat_set?, value? | per_level?}`：全等级同值压缩为
`value`，否则保留 `per_level: [[level, value], ...]`。文件头部 `_meta` 记录
vendor commit（读自 `.pob2-version.txt`）与 `regen_command`。

确定性约定：Lua 引导脚本只负责忠实抽取（JSONL）；排序（`skill, stat, stat_set`）、
数字格式与文档序列化统一在 Rust 侧完成——同输入重跑 **byte-stable**，产物禁手改、
只许工具再生。

注意：overlay 的 merge 消费侧（pobr-gamedata / adapter）由后续 wave 接入；
本工具只负责抽取与固化。
