# sync-pob-catalog

从 vendor PoB2（Lua）抽取数据并做 parity 检查的离线工具。下面说明最常用的两组子命令；
在仓库根运行，先用 `mise run setup:oracle` 准备固定 vendor 与 LuaJIT。
完整列表（`extract-bases`、`check-buff-refs`、`gen-mirage-configs`、`gen-trigger-configs`、
`gen-stat-id-map`、`gen-skill-types`、`parser-rules-drift`）见不带参数运行时打印的用法。

## catalog 系（scan / check / diff / fixture-check）

从 PoB2 核心 Lua 文件（`BuildDisplayStats.lua` / `CalcSections.lua` / `Calc*.lua`）
抽取 output/display/breakdown catalog，并对已入库 fixture 做 drift 检查：

```bash
mise exec -- cargo run -p sync-pob-catalog -- scan  --pob-root <PoB2路径> [--out catalog.json]
mise exec -- cargo run -p sync-pob-catalog -- check --pob-root <PoB2路径> --catalog catalog.json
mise exec -- cargo run -p sync-pob-catalog -- diff  --pob-root <PoB2路径> --catalog catalog.json
```

## extract-lua（overlay 抽取通道）

用 luajit 在**最小 stub 环境**下执行 vendor 的 Lua 数据文件（不全量启动 PoB2，
只 stub `SkillType`/`ModFlag`/`KeywordFlag` 与 `mod`/`flag`/`skill` 注入函数），
把"不在 GGG .dat、由 PoB2 手工 Lua 维护"的人工策展层固化为确定性 JSON，落到
`data/<版本>/overlay/`；重新生成时保留其明确来源，产物不手改。

```bash
# 默认抽取 act_dex/act_int/act_str 三个玩家技能文件，输出到 stdout
mise exec -- cargo run -p sync-pob-catalog -- extract-lua --vendor-root "$PWD/vendor/PathOfBuilding-PoE2/src"

# 固化（再生成）入库 overlay 文件
mise exec -- cargo run -p sync-pob-catalog -- extract-lua \
    --vendor-root "$PWD/vendor/PathOfBuilding-PoE2/src" \
    --out "data/$(cat data/CURRENT)/overlay/skill_overrides.json"
```

参数：

| 参数 | 说明 |
|------|------|
| `--vendor-root <path>` | vendor PoB2 的 `src/` 目录（只读输入，必填） |
| `--what <kind>` | 抽取哪一类产物；缺省 `skill-overrides`。其余：`gem-quality`、`stat-map`、`stat-descriptions`、`gem-effects`、`stat-set-labels`、`config-options`、`curse-priority`、`minions`、`spectres`、`minion-list`、`mod-scalability`、`runes`、`uniques`、`catalysts`、`parser-rules`、`special-mods` |
| `--out <path>` | 输出文件；缺省写 stdout |
| `--files <a,b,c>` | 抽取的 `Data/Skills/<name>.lua` 列表；缺省 `act_dex,act_int,act_str` |
| `--luajit <path>` | LuaJIT 路径；mise 已设置 `POBR_LUAJIT` 指向项目准备的固定 runtime，可显式覆盖 |
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

运行时由 `pobr-gamedata` 加载技能数据时合并这份 overlay；本工具只负责抽取与固化。
