# 技能速度 (Skill Speed)

技能速度是一个通用属性，改变执行技能动画所需的时间。增加技能速度的修饰词（如 Acceleration）会缩短完成技能动画所需的时间[^mobalytics-skill-speed]。

**重要**：技能速度**不应与行动速度 (Action Speed)** 混淆，这两个属性的计算方式不同。

## 技能速度应用

技能速度修饰词是通用的，影响任何技能。技能速度与任何针对特定类型技能的修饰词**加法叠加**。

例如：拥有 `Practiced Signs` 提供"6% 增加施法速度"和 `Flow State` 提供"5% 增加技能速度"，你总共有"11% 增加施法速度"用于法术使用。

技能特定速度属性的示例：
- 战吼速度 (Warcry Speed)
- 攻击速度 (Attack Speed)
- 装填速度 (Reload Speed)
- 图腾放置速度 (Totem Placement Speed)

## 计算技能速度

所有适用修饰词的总和可以用来计算最终使用技能所需的时间。步骤如下：

1. 将技能的基准速度转换为每秒使用次数：
   - 基准使用时间 1.2 秒的技能 = 1 / 1.2 = 0.833 次/秒

2. 应用技能速度修饰词：
```
最终每秒使用次数 = UPS * (1 + MSS%)
```

其中：
- **UPS** = 技能每秒使用次数
- **MSS%** = 技能速度总修饰词

### 计算示例

使用 `Dreaming Quarterstaff`，基准 1.5 次/秒攻击，拥有 30% 总增加技能速度：
```
最终每秒使用次数 = 1.5 * 1.3 = 1.95 次/秒
最终使用时间 = 1 / 1.95 = 0.51 秒
```

### 特殊技能

某些技能有额外的总动画时间，如 `Rolling Slam`。这些技能的基准使用速度可以被修改，但**额外的动画时间不能被修改**。这个固定的额外时间必须加到技能的最终使用时间来计算完整动画时间。

## 服务器限制

不需要引导的技能每个服务器帧只能执行一次（每 0.033 秒一次）[^poe2wiki-cast-speed]。

如果你的最终使用时间小于 **0.033 秒**，你将因错过帧而实际损失 DPS，即使技能的工具提示信息中不会反映这种损失。

这意味着你每秒最多可以执行 **30.3** 个被完全计入的动作。

## 行动速度 vs 技能速度

- **技能速度 (Skill Speed)**：仅影响技能动画时间
- **行动速度 (Action Speed)**：影响所有动作的整体速度（包括移动、动画等）

两者是分别计算的，不应混淆。

## 相关机制

- **攻击速度 (Attack Speed)**：特定于攻击技能的速度
- **施法速度 (Cast Speed)**：特定于法术技能的速度
- **行动速度 (Action Speed)**：通用行动速度
- **每秒使用次数 (Uses Per Second)**：技能速度的最终输出指标

---

## 参考来源

[^mobalytics-skill-speed]: Mobalytics — PoE 2 Guide: Skill Speed Explained. https://mobalytics.gg/poe-2/guides/skill-speed
[^poe2wiki-cast-speed]: PoE Wiki — Cast speed. https://www.poewiki.net/wiki/Cast_speed
