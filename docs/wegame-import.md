# WeGame 分享导入

在顶栏“BD 库”的导入框粘贴
`https://www.wegame.com.cn/helper/poe2/#/share/<code>`。
导入后可使用现有装备编辑器、技能组、天赋树、物品库和规划器，也可导出 PoBR 存档或 PoB2 构筑码。
各格式携带的信息见[导入格式](import-formats.md)。

## 请求与部署

页面向同源的 `POST /api/import/wegame` 发送 `{ "url": "..." }`。
服务只读取分享页使用的五个公开 Profile 接口：`GetRoleInfo`、`GetEquipments`、
`GetTalentTree`、`GetJewels`、`GetSkills`。请求不需要登录，不转发 Cookie 或凭据。
返回包不包含账号 id、角色名或分享令牌，响应不缓存。

分享过期、任一接口失败或数据格式错误都会使导入失败，当前 BD 不会被残缺结果替换。
返回的 `{ "format": "wegame", "version": 1, ... }` JSON 也可保存到本地，再从 JSON／文件入口导入。

`web/public/_worker.js` 是 Cloudflare Pages 的 advanced-mode Worker。
Vite 会把它复制到 `dist/`；本地 `pnpm --dir web dev` 和 `pnpm --dir web preview`
也使用同一处理器。其他静态托管服务需另行提供这个接口。计算仍由浏览器中的 WASM 执行。

## 保留的输入

Rust 导入器与 `.build` 共用 `decode_build_file_json` 入口，读取：

- 角色等级、职业和升华。
- 装备基底、词条、已掷出的防御值、品质和符文插槽。
- 技能组及宝石等级、品质、腐化状态；带注释的等级取开头数字。
- 天赋属性选择、两组武器及专属天赋、树上的已插珠宝。
- 实际任务奖励；这些奖励替换默认剧情奖励，避免重复计入。

中文文本通过现有字典转为引擎使用的英文文本。宝石只有简中名；解析时同时检查是否辅助，
同名时优先选普通宝石。无法消除歧义的宝石、未知天赋和未支持栏位写入 `[import]` 笔记。
`PassiveJewel_<n>` 按游戏插槽表的行号映射，不能按第 n 个插槽猜测。

## 导入后要检查什么

WeGame 不提供 PoB 战斗配置、主技能选择或技能与武器组的绑定。
请确认主技能、抗性惩罚和战斗条件，并在技能页分配武器组。
第二组武器与专属天赋会保留，可在页面切换；来源顺序不能证明技能属于哪一组。

萨满专用的羁绊符文效果和插在装备中的珠宝仍需人工核对。
词典缺少的宝石名会在解析失败时报告；未支持的词条保留现有计算诊断。
导入成功不代表每个游戏机制都已支持。旧存档若曾丢失另一组武器或物品结构，需要重新导入来源。

## 旧国服 `.build` 与 PoBR 存档

旧国服 `.build` JSON 必须含 `passives`、`items`、`skills` 三个数组，可以为空。
名称和额外元数据可选。无关 JSON、缺少数组或类型错误会在修改当前 BD、阶段或物品库前失败。
显式 `format` 字段仍选择 WeGame 解码器，再执行原有的版本与角色检查。

PoBR 单阶段存档、全部阶段文件和工作区备份请使用“从文件导入”，不要粘贴到 `.build` 输入框。
完整工作区备份恢复前会确认替换范围。

## 箭袋换装示例

先选定弓技能，再把候选箭袋按副手（`weapon2`）存入物品库，以 DPS 为目标运行物品优化。
优化器会实际装备每件候选并完整重算。主手与副手候选分开保存，导入弓不会使它成为箭袋栏候选。

提升页的加权市集链接用于初步筛选。它不自动抓取商品，也不能证明在预算内找到了整个市场的最优解。

## 依据与验证范围

协议参考为公开的 [WeGame PoE2 助手](https://www.wegame.com.cn/helper/poe2/) 和
[PoB2 ImportTab.lua](https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2/blob/ce566eac45ea8a86477f513c7ee65a1ebe60014e/src/Classes/ImportTab.lua)，原核对日期为 2026-09-11。
实现见 `apps/pobr-wasm/src/build_api/wegame.rs` 与 `web/public/_worker.js`。
测试见 `apps/pobr-wasm/tests/wegame.rs`、`web/e2e/wegame-import.spec.ts`，使用合成数据。
被忽略的本地 smoke 测试可通过 `POBR_WEGAME_FILE` 读取真实样本；玩家导出不提交到仓库。
