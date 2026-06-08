# pob2-oracle — PoB2 headless 计算 oracle

把 vendored 的 **PathOfBuilding-PoE2**（Lua）引导成 headless，加载一个 build 并 dump
PoB2 的完整计算分解（中间值 + 最终值）为 JSON。用于钉死 PoBR vs PoB2 的逐分量偏差，
并作为持续的 golden 中间值参照。

**不修改任何 vendor 源**——只是一个独立 wrapper 脚本，靠 `LUA_PATH` 注入纯 Lua 运行时库。

## 依赖 / 阻塞解决

`luajit -e "dofile('HeadlessWrapper.lua')"` 从 `src/` 直接跑会因 `require 'xml'` 失败。
**原因不是缺原生模块**——PoB2 仓库自带纯 Lua 版的 `xml` / `base64` / `sha1` / `dkjson` /
`lua-utf8`（在 `vendor/PathOfBuilding-PoE2/runtime/lua/`），只是不在默认 `package.path` 上。
PoB 自己的 `.busted` 配置就是靠 `lpath = "../runtime/lua/?.lua"` 解决的。

**解法**：把 `runtime/lua/` 加进 `LUA_PATH` 即可，无需任何 shim、luarocks 或 stub。
`HeadlessWrapper.lua` 已经把 `lcurl.safe` stub 成 no-op，所以不需要 lcurl。

需要：
- `luajit`（默认 `/opt/homebrew/bin/luajit`，可用 `LUAJIT=...` 覆盖）

## 用法

```bash
# 从仓库任意位置：
tools/pob2-oracle/run.sh <decoded.xml> [out.json]

# 例：deadeye build（meta.json golden 对照）
tools/pob2-oracle/run.sh \
  examples/demo-bd-test/builds/ranger-deadeye-explosive-grenade/decoded.xml \
  /tmp/deadeye_oracle.json
```

不给 `out.json` 时打到 stdout。

也可先把 PoB build code 解码成 XML 再跑（与 PoBR 测试同一份 build）：

```bash
cargo run -q -p pobr-cli -- decode-code "$(cat examples/demo-bd-test/ninja-bd-deadeye.txt)" > /tmp/x.xml
tools/pob2-oracle/run.sh /tmp/x.xml /tmp/x_oracle.json
```

## 输出字段（JSON）

- `mainOutput` — `build.calcsTab.mainEnv.player.output` 的全部 scalar（最终值，PoB2 面板口径）。
  含 `AverageDamage` / `TotalDPS` / `CritChance` / `Speed` / 各 `<Type>StoredCombinedAvg`
  （分类型平均击中）/ 抗性 / 防御等。
- `calcsOutput` — `calcsEnv.player.output` scalar（带 breakdown 的那份）。
- `intermediates` — main skill 的 `skillModList` 聚合查询：分类型 increased / more /
  conversion（`Convert_<From>To<To>`）/ gain-as-extra（`DamageGainAs_<To>` 等）/ 暴击。
- `components` — `mainOutput[<Type>Min/Max/HitAverage]`（按技能部件可能为空，看 StoredCombinedAvg）。
- `summedBase` / `damageTypeBreakdown` / `conversionTable` — 转换链中间值（部分仅 CALCS env 可见）。
- `skillInfo` — 主技能 active gem 名 / **有效等级** / quality / supports（核对 +N gem level）。

## 校验

对 deadeye build 跑，`mainOutput.AverageDamage` / `TotalDPS` 与该 build 的
`meta.json::player_stats` golden 完全一致（逐位）：

```
AverageDamage  108426.0294   (golden 108426.02939805)
TotalDPS        26672.80323  (golden  26672.803231921)
```

证明 oracle 引导出的 PoB2 引擎与 PoB2 自己导出的权威值一致。
