# sync-pob-catalog

从 vendor PoB2（Lua）抽取数据并做 parity 检查的离线工具。下面说明最常用的两组子命令；
在仓库根运行，先用 `mise run setup:oracle` 准备固定 vendor 与 LuaJIT。
完整列表（`extract-bases`、`check-buff-refs`、`gen-mirage-configs`、`gen-trigger-configs`、
`gen-stat-id-map`、`gen-skill-types`、`parser-rules-drift`）见不带参数运行时打印的用法。

`check-buff-refs` 比较固定行段的 FNV-1a 漂移信号，FNV 不是来源认证；行段移动时应先核对固定旧、新源码的实际字节及上下文，再决定是否更新引用。`--write` 只刷新现有行段的 hash，不会定位或审查新位置；任何引用越界时拒绝写回。
`parser-rules-drift` 成功只说明重新抽取的文件与指定文件逐字节相同；抽取中的闭包推断失败、handler 回退和固定计数警告仍须单独审查，不能据此认定语义已认证。

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

当前保留表的旧来源 `ce8bffaba31f8e68cfce70579e1c96465e7c133c` 已能从上游固定 Git 提交获取。
对其 [`CalcPerform.lua`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce8bffaba31f8e68cfce70579e1c96465e7c133c/src/Modules/CalcPerform.lua)
及 [`CalcDefence.lua`](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce8bffaba31f8e68cfce70579e1c96465e7c133c/src/Modules/CalcDefence.lua)
逐条按各自 `vendor_ref.file` 核对，15 个原行段的旧 hash 均匹配；在当前固定
`bb52d6b368307457eb9c54bb13f1829993d390b1` 中各有唯一、逐字节相同的行段。
这消除了“旧源码无法取得”的证据缺口，但外部保留表仍记录旧来源／旧行号，默认漂移检查仍报告失败。
引用元数据的外部修订、完整上下文适用和运行身份更新须分别处理；未改效果值，也未据此认证官方 buff 或 charm 语义。

RePoE 的[固定来源对账](https://github.com/ackness/repoe/blob/e3cabf9bd008240e12a173c7233c2c48e56eb822/RePoE/docs/buff_source_reconciliation.md)
进一步核对了七个完整上下文：`doActorMisc` 仅插入类型注释，Arcane 分支／前序相同，
但调用者、defence 和 reservation 的上游条件已有变化，整体等价未确立。
独立验证器保留 `guard_eligible=false`，默认退出 2；`--evidence-only` 退出 0 仅确认固定证据，
不能作为兼容性通过或替代本工具的失败守卫。运行定义、数值和引用没有因此更新。
