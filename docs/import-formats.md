# 构筑导入格式

PoBR 把不同来源的角色方案（BD）转换为同一个 `Build`。各格式携带的信息不同。
导入成功只表示文件可读，不表示装备、技能和战斗条件都完整，也不表示所有效果都能计算。
本文说明各格式读到了什么、缺少什么，以及导入器怎样报告问题。

导入器把跳过的物品和无模型栏位写入 `ImportReport`，把计算不能应用的词条保留在
`unsupported_modifier_texts`（Web 响应为 `unsupported_modifiers`）。WeGame 的额外提示写入
笔记，以 `[import]` 开头。存档与导出规则见[构筑模型](build-model.md)。

<a id="where-each-format-is-decided"></a>
## 怎样识别格式

| 输入 | 识别条件 | 读取入口 |
|---|---|---|
| PoB2 分享码 | 可解码为压缩的 XML（zlib + URL-safe Base64） | `pobr-build` 的 `decode_pob_code`、`xml_build` |
| 分享链接 | `web/public/build-import.mjs` 的固定站点与路径白名单 | Pages Worker 取回原始码，再交给普通 PoB2 解码器 |
| WeGame 分享链接 | `www.wegame.com.cn/helper/poe2/#/share/<key>` | Worker 合并五个公开接口，`build_api/wegame.rs` 读取结果 |
| WeGame 分享 JSON | 对象含 `format` 字段，再校验格式、版本和角色 | 同上 |
| 游戏 Build Planner v1 `.build` | 没有 `items`，有 `name`，再校验 Planner 结构 | `build_api/planner_file.rs` |
| 旧国服 `.build` | 有 `items`，且 `passives`、`items`、`skills` 都是数组 | `build_api/decode.rs` |
| PoBR 单阶段存档、全部阶段文件、工作区备份 | 由存档加载器校验版本及 `state`／`workspace` 等结构 | 从文件导入，不交给 `.build` 解码器猜测 |

分享链接支持 `pobb.in`、`poe.ninja/poe2/pob`、`pastebin.com`、`rentry.co`、
`poe2db.tw/pob` 和 `maxroll.gg/poe2/pob`。poe.ninja 的角色页面没有可直接读取的构筑码；
导入器会报 `ninja_profile`，提示改用 `/pob/` 分享链接或复制构筑码。

无关 JSON、缺少数组或数组类型错误的旧 `.build` 会在修改当前 BD、阶段或物品库前失败。
数组可以为空。PoBR 存档和备份请使用“从文件导入”。

<a id="what-each-one-carries"></a>
## 各格式携带什么

| 内容 | PoB2 码（含上述分享站点） | WeGame 分享 | Planner v1 `.build` | 旧国服 `.build` |
|---|---|---|---|---|
| 角色等级、职业、升华 | 有 | 有；升华角色的 `class_name` 是升华名 | 等级固定默认 1；可带升华 | 等级从名称的 `Lv.N` 读取，可带升华 |
| 天赋树 | 节点 id、树版本、武器组专属点、属性选择 | 节点 hash、`specialisations` 专属点、`skill_overrides` 属性选择 | 游戏节点 id、武器组；识别属性提示 | 节点 slug，再映射为 id |
| 装备 | 完整文本，含变体、数值范围和符文 | 按栏位的 JSON，由导入器重建文本 | 只有提示，不生成物品 | 缺少基底名和稀有度，以占位基底和 RARE 构造 |
| 技能和辅助 | 稳定 id、等级、品质、启用状态、效果序号、配置组 | 只有简中名；等级和品质来自属性字符串 | 游戏宝石 id；默认 1 级、0 品质，精确的 `Level N, Q% Quality` 提示可恢复数值 | 宝石 id；默认 20 级、0 品质 |
| 主技能选择 | 有 | 无 | 无 | 无 |
| 天赋／物品授予技能的来源 | `Tree:<node>`、物品来源、`enableGlobal` 等 | 无来源标记，作为普通组出现 | 不携带 | 不携带 |
| 技能的武器组绑定 | 有 | 无 | 可携带 | 跳过第二组 |
| 战斗配置和参考值 | 有配置及 `PlayerStat` 等参考值 | 只有任务奖励 | 无 | 无 |
| 树上珠宝 | 插槽节点 id | `PassiveJewel_<n>` 或节点 id | 无 | 无 |

<a id="per-field-differences-the-importer-normalises"></a>
## 导入器怎样处理写法差异

<a id="wegame"></a>
### WeGame

- 等级、品质可以带注释。导入器读取开头的数字：`20(最高等级)` 为 20 级，`+20%` 为 20 品质。
- 宝石按“简中名、是否辅助”查找。同名的普通宝石优先于物品授予的变体和 `Unique` 前缀效果。
  例如 Coming Calamity 授予的版本不会挤掉普通 Herald of Ice。同名仍有歧义时
  （如各武器的默认攻击），导入器报告 `Unknown or ambiguous gem`。
- `PassiveJewel_<n>` 的 `n` 是游戏 `PassiveJewelSlots` 表的行号，不是插槽顺序或节点号。
  该行经 `Slot` 指向天赋节点；映射位于 `data/overlay-common/passive_jewel_slots.json`。
  找不到有效插槽时报告问题，不放入该珠宝。
- 羁绊（`Bonded`）符文行依赖特定职业，导入器不应用，并在笔记中说明。
- 任务奖励按文本读取；`N Broken Boss Faces` 是进度计数，标为未建模，不作为普通词条。
- 来源没有主技能、命名技能集和技能武器组绑定。解码器先取第一组，并写入提醒。
  用户应在技能页确认主技能与武器组。曾核对的一个真实分享首组是默认 Bow Shot，并非主要输出技能。

完整的请求边界和限制见 [WeGame 导入](wegame-import.md)。

<a id="pob2-codes-poeninja-and-the-others"></a>
### PoB2 码

从游戏导出的授予技能有时又列为普通技能组。导入器按组首效果、授予等级等信息识别它，
让已有组优先于自动生成的 `Tree:` 组，避免重复。找不到对应启用组的授予技能仍保留未支持提示。

`<PlayerStat>` 是导出工具在导出时得到的参考值，可能来自旧版工具。
它不是游戏真值；差异排查用 `mise run diagnose` 和 `mise run compare`。

物品读取支持变体标签、范围数值及换行的游戏词条。`Implicits: N` 是序列化后的行边界，
其中的附魔和符文行也消耗计数。导入器仍把它们放入 `enchant_texts`，再正确区分后面的外延词条。
只理解一部分的词条行保留原文并报告，不应用已读懂的片段。

<a id="planner-v1-and-older-cn-build"></a>
### Planner v1 与旧国服 `.build`

Planner 的装备栏只保存建议，不能还原装备、药剂或珠宝。旧国服格式虽能重建词条，
也缺少可靠的基底和稀有度。两者的宝石等级还可能使用默认值；导入后应补齐信息再看数值。
Planner 保留作者、链接、说明、等级区间和提示，供本地保存与再导出；等级区间不参与计算。
导出会跳过 meta 宝石、无游戏映射的条目和无栏位映射的珠宝／肢体，并列出警告。
历史树要先转换为当前树。格式测试使用合成数据，尚未在游戏内验证导入。

<a id="rules-for-a-new-or-changed-format"></a>
## 新增或修改导入格式时

1. 在导入器中统一平台写法，例如把 `20(最高等级)` 转为 20，不让后续计算识别平台格式。
2. 优先使用游戏表。无法解析 id 时先补表和来源记录，不猜映射关系。
3. 无法放入模型的内容写入笔记或 `ImportReport`，不能静默丢失。
4. 回归测试覆盖数据形状，使用合成数据。`apps/pobr-wasm/tests/wegame.rs` 不包含账号、分享令牌或玩家导出。
5. 来源没有主技能或授予标记时明确提示，不能把默认选择说成来源提供的信息。
