# 数据版本化与迭代模型（2026-06-18）

> 对照 vendor PoB2（`vendor/PathOfBuilding-PoE2/`，commit 见 `vendor/.pob2-version.txt`）综合而成，
> 用于把"数据如何随游戏版本迭代、平衡改动如何反映进计算"这件事钉成可执行契约。
> **权威性低于代码与一手数据**——执行时仍以 vendor Lua、`data/<ver>/` 实际入库 JSON、
> 现有 Rust 代码三者交叉验证为准。

## 0. 一句话结论

PoB2 把数据切成**两个版本制式**：唯一随版本"分档保存"的是**天赋树**（`TreeData/<ver>/`，
每个 build 记 `spec.treeVersion`），其余（技能 / 装备 / 词条 / `ModParser`）是**一份当前实时池**，
每次打补丁整体重生，旧 build 用当前数值重算。

PoBR 走的是**更强的解耦**：`data/<ver>/` 是**自包含整版快照**（base + overlay + generated），
多版本并存，运行时按 `data_version()` 选一份；calc 引擎是 100% 版本无关的纯 Rust 代码、零数据内嵌。
**保留 PoBR 的快照模型**——它比 PoB2 单池更适合本仓库明确要的目标：在多个版本数据上做可复现的
逻辑测试（PoB2 单池无法重算旧补丁的精确数值）。从 PoB2 对齐的是"让平衡改动可见、迭代快"的机制，
不是"退回单池"。

## 1. PoB2 参考模型（两制式）

| 维度 | 版本制式 | 机制 | 文件 |
|------|----------|------|------|
| 天赋树 | **版本分档** | `TreeData/<ver>/`；build 存 `spec.treeVersion` 并加载对应树；旧树常驻可读；改动节点 → `ignoredNodes`；"convert to latest" 迁移分配 | `src/TreeData/0_1../0_5/`、`src/Classes/PassiveSpec.lua`、`src/GameVersions.lua`（`treeVersionList`） |
| 技能 / 宝石 | 版本无关单池 | 整表从 GGG `.dat` 重生；旧 build 直接用新数值 | `src/Data/Gems.lua`（`-- automatically generated, do not edit!`） |
| 装备 base / 词条 | 版本无关单池 | 同上；删除的词条直接从池里消失 | `src/Data/ModItem.lua` / `Bases/` |
| 词条**解析器** | 版本无关手维 | 手写 Lua，按补丁措辞改动手改；旧措辞保留以兼容旧存档 | `src/Modules/ModParser.lua` → 生成 `src/Data/ModCache.lua` |

**更新循环（补丁掉落时）**：在 Dat View 跑 `src/Export/Scripts/{skills,bases,mods,passivetree}.lua`
重生 `Data/*.lua` + 新 `TreeData/<ver>/` → 手改 `ModParser.lua` 适配新措辞 → `Ctrl+F5` 重生 `ModCache.lua`
→ PR（含全部重生文件）+ CHANGELOG。**"技能被重做 / 词条被删"在 PoB2 不是迁移问题，只是重生一遍当前池。**

## 2. PoBR 当前模型

- **磁盘布局**：`data/<ver>/{base,overlay,generated,i18n}/` + `manifest.json`；`data/CURRENT` 选活动版本。
  - `base/`：GGG `.dat` → `pobr-data-adapter` 反范式化（自动生成，等价 PoB2 的 `-- do not edit`）。
  - `overlay/`：两类——**自动抽取**（vendor Lua → `sync-pob-catalog`，带 `_meta.vendor_commit` / `regen_command`）
    与**手工策展**（`special_mods` / `buff_definitions` / `local_mods` / `high_precision_mods` / `vendor_name_aliases`，
    无 `regen_command`，版本升级时从旧版**原样搬运**待人审）。
  - `generated/`：`precompile-mods` 的确定性产物（`parsed_mods.json` 等）。
- **运行时版本解析**：`pobr_gamedata::data_version()` = 环境 `POBR_DATA_VERSION` → `data/CURRENT` → `pobr_data::DATA_VERSION` 常量。
  `GameData::new(version_dir)` 按域懒加载，base 先入、overlay 确定性 merge（`overlay.rs`）。**切版本零代码改动。**
- **引擎**：纯函数 + 确定性，不含任何版本分支；这是 PoBR 相对 PoB2 的解耦优势。

## 3. 迭代循环（如何把一个新版本/平衡改动接进来）

```
1) pipeline/config.json 改 "patch"
2) pipeline/regen-all.sh                  # .dat 下载 → adapter → base → overlay 抽取 → precompile → data/<新版>/
3) python3 pipeline/diff-data.py <旧> <新> --semantic   # ★ 看清这版到底改了什么（见 §4）
4) 据 diff 决定手工 overlay / golden 测试是否要跟随（§5）
5) devs/scripts/version-bump-drill.sh     # 字节级可再生 + 编译 + parity 可运行门禁
```

云端约束（实测，见 `run-pobr` skill 的 Gotchas）：GGG CDN 对旧 pin 版本 404；`extract-lua` / `pob2-oracle`
与当前 vendor commit 不兼容。故云端把**已入库 data 当权威**，只跑 precompile（步骤 4 可字节复现）。

## 4. 语义 diff：让平衡改动可见（本轮新增，对齐 PoB2 的 export+CHANGELOG 可见性）

`pipeline/diff-data.py` 升级为两档：
- 默认（文件级）：文件增删 + 条目数 / 字节变化（向后兼容）。
- `--semantic`：按域逐条 **keyed-diff**——天赋节点 / 技能 / 词条 / unique 的**新增·删除·数值改动**，
  改动直接打印 `stats` 文本 old→new。这正是"某节点被移动、某技能伤害 A→B、某词条被删"的可视化，
  PoB2 靠人工 export + CHANGELOG 获得，PoBR 现在一条命令拿到。

```bash
python3 pipeline/diff-data.py data/4.5.0.3.4 data/4.5.2.1.3 --semantic            # 全域
python3 pipeline/diff-data.py A B --semantic --domain tree --limit 40            # 单域
python3 pipeline/diff-data.py A B --semantic --json /tmp/diff.json               # 机读
```

覆盖域：`tree` / `mods` / `bases` / `skill_levels` / `skill_stats` / `special_mods` / `uniques`。
实跑 `4.5.0.3.4 → 4.5.2.1.3` 即暴露真实平衡改动（如树节点 `deflect43` 的 `5%→4%`）与
**手工 overlay 漂移**（`special_mods` 86→78：fork-a 在旧版手补的 8 条整行未传播到新版——见 §6 gap C）。

## 5. 测试哲学：逻辑不变量为主，黄金数值为版本参考（含本轮约定）

两层互补、职责分明：

1. **逻辑不变量套件（主门禁，版本无关）**——在 `data_version()` 解析到的**任意**版本上跑，断言
   *关系与性质* 而非 PoB2 精确数字：抗性钳到上限、more 连乘（非相加）、转换守恒、life>0 /
   命中率∈[0,1]、跨版本确定性等。这是"引擎逻辑正确"的门。`crates/pobr-build/tests/parity/multi_version.rs`
   是版本无关性的活证；`load_special_mods.rs` 已从"精确计数/快照"改为不变量（id 唯一 / provenance
   非空 / 产物形态）——**数据增长不再误红**。
2. **黄金参考套件（次级，显式钉版本）**——对 PoB2 / oracle / 自快照**录制数值**的比对，全部按
   `pobr_data::GOLDEN_PARITY_DATA_VERSION` 加载数据（与活动 `DATA_VERSION` 解耦）。它是**某个 PoB2
   版本的参考快照**，不是逻辑门；推进到新版本需**重录 golden**，而非改数据/改测试去凑数。
   聚合二进制 `crates/pobr-build/tests/parity.rs` 已把两类显式分组：**A 组**=版本钉定 golden
   （`ninja_parity` / `defence_panels_golden` / `coc_trigger_golden` / `crossbow_reload_golden` /
   `skill_dot_golden` / `pob2_parity` / `stored_hand_output`，统一钉 `GOLDEN_PARITY_DATA_VERSION`）；
   **B 组**=版本无关（`multi_version` 遍历全版本、`e2e_real_build` 只验范围/确定性/导入、
   `golden_regression` 用 `calculate()` 零数据加载是纯 calc-CODE 回归锁、`special_oracle_differential`
   实时差分 skip-guarded）。物理目录不分（`#[path]` 已解耦位置，且多文件含 file-relative
   `include_str!`，移动易碎），分类以注释分组 + 加载口径为准。

> 黄金值来源：build 导出 XML 内 `<PlayerStat>` = 该 PoB2 版本 Lua 侧算出的数值，随版本快照。
> 故"精确数值测试"天然版本相关，必须隔离在版本档里；主测试验逻辑是否合理，不单一追数字。

## 6. 已核实的差距与状态

| 编号 | 差距 | 状态 |
|------|------|------|
| A | **语义跨版本 diff 缺失**——旧 `diff-data.py` 仅文件/计数/字节级，看不出"改了什么" | ✅ 本轮解决（§4） |
| B | **per-build 天赋树版本失配**——节点跨版本移动/删除后，calc 对已分配的未知 id **静默丢弃**（`pobr_tree` node.rs `filter_map`），无任何告警 | 🟡 本轮部分：`<Spec treeVersion>` 不再丢弃（挂 `Build::tree_version`）；`diagnose_tree_version`→`TreeVersionReport` 显性检出"已分配但不在已加载树"的节点（非致命、纯只读）。**剩余**：按 build 树版本加载对应树 + 迁移、把诊断接入 calc 输出/CLI 自动提示——待续（需树版本↔数据版本映射 + 多树版本数据集） |
| C | **手工 overlay 搬运静默过期**——版本升级时 `special_mods` 等从旧版原样搬，无"vendor 锚点在新版是否仍存在"校验 | ⏳ 部分可见（§4 的 `--semantic` 能看出条目漂移）；锚点存活校验需 vendor Lua，单列 |

> **实测发现（`diagnose_tree_version`）**：活动 golden 版本 `4.5.0.3.4` 的 `passive_tree`
> 漏掉真实 0_5 build 分配的若干节点（skill `35387`/`17044`/`6554` = Cold Damage 类，
> 存在于 `4.5.2.1.3` 却缺于 `4.5.0.3.4`）——calc 现状静默丢其贡献。未破 golden 容差，
> 但属真实数据缺口；data 再生在云端不可复现，单列跟进。

gap C + gap B 剩余项不在本轮范围。落地顺序与设计待后续单独评估。
