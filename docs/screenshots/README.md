# 线上界面截图

本页列出的新截图于 **2026-10-30（UTC）**，用 ego-lite 实际访问
[线上网站](https://pobr.win)拍摄。应用版本为 **v0.0.43**，游戏数据为 **0.5.5.4.2**，
界面使用简体中文、深色主题和默认 16px 字号。桌面截图为 1686 × 839 像素。

没有替换页面内容或伪造计算结果。导入、导航、显示设置和换装比较都通过界面操作完成。
截图保留了「未参与计算」和「部分计算」提示；示例数值不代表完整机制已获验证。

## 页面

| 截图 | 页面和状态 |
| --- | --- |
| [装备总览](equipment.png) | `/items`，默认分组详解，展示装备位置与物品库 |
| [技能与辅助](support-upgrades.png) | `/skills`，已选择寒霜爆技能组，展示宝石、等级、品质及辅助入口；没有运行辅助寻优 |
| [天赋树](passive-tree.png) | `/tree`，已分配节点可见，热力图关闭；没有应用新的升级路线 |
| [换装比较](item-comparison.png) | `/trade`，粘贴下方示例戒指，对比两个戒指位置；没有应用换装 |
| [显示设置](settings-live.png) | 顶栏「设置」，展示三种全局显示方式、主题、语言和字号 |
| [装备游戏卡片](equipment-game-live.png) | `/items`，全局选择「游戏卡片」，查看胸甲 |
| [计算页](calcs-live.png) | `/calcs`，游戏卡片模式 |
| [配置页](config-live.png) | `/config`，游戏卡片模式 |

根目录中英 README 使用前四张图。`display-modes/`、`item-display/` 和 `../images/`
中已有的截图属于先前改动的记录，没有冒充为本次线上实拍。

## 输入来源

角色来自仓库公开回归样本
[`monk-invoker-frost-bomb`](../../examples/demo-bd-test/builds/monk-invoker-frost-bomb/meta.json)，
使用其 [`code.txt`](../../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt) 在「BD 库」中导入。
样本最初于 2026-06-07 从 poe.ninja 抓取，角色为 98 级祈求者，当前选中的技能是寒霜爆。
这不是最新赛季的配装建议。

换装比较使用人为编写的演示物品，名称为 `Documentation Example`。
它不代表实际市集商品，也不证明这些词缀能按所示数值同时生成。截图演示的是完整文本解析和换装计算：

```text
Rarity: Rare
Documentation Example
Gold Ring
--------
Item Level: 80
--------
30% increased Rarity of Items found
--------
+100 to maximum Mana
+40% to Fire Resistance
+40% to Cold Resistance
+40% to Lightning Resistance
40% increased Energy Shield
```

## 重新拍摄

1. 用 ego-lite 打开 `https://pobr.win`，确认顶栏版本和已加载的数据版本。
2. 在「BD 库」粘贴样本代码，按「导入为新 BD」，等计算完成。
3. 按上表打开页面；截图前等待页面切换动画结束。换装页要等结果出现。
4. 记录实际截图尺寸、日期、应用版本、数据版本、语言和输入来源，再更新 README。

两个域名的浏览器存档各自独立。在 `pobr.win` 导入样本，不会更新
`pobr-web.pages.dev` 中的 BD 库。
