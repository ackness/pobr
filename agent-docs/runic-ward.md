# 符咒护佑 (Runic Ward)

符咒护佑是 POE2 在 Return of the Ancients (0.5.0) 更新中引入的新防御机制，作为一个可以吸收伤害的 hit pool，位于生命 (Life) 之后[^mobalytics-runic-ward][^maxroll-runes-of-aldur]。

## 基本机制

当生命降至 1 时，任何符咒护佑开始通过承受伤害而损失。如果符咒护佑降至 0，而最后一点伤害施加于剩余的 1 点生命，角色死亡。

符咒护佑通过基础再生效果随时间恢复，该恢复独立于生命恢复。

符咒护佑显示为生命球体上方的符文符号。

**注意**：总符咒护佑值也会加到进入 Sekhemas 试炼时的起始**荣耀 (Honour)** 上[^maxroll-050-patchnotes]。

## 获取符咒护佑

### 符文锻造 (Runeforging) — 0.5.0

符咒护佑主要通过**符文锻造台 (Runeforging bench)** 对装备进行改造后获得：

- **55 级及以下**：符咒护佑被添加到装备的现有属性中（纯增益）
- **55 级以上**：装备会交换部分基础防御（护甲、闪避或能量护盾）以换取更高数量的符咒护佑
- 改造后的物品会有"Runeforged"前缀名称，表示不能再次被同一锻造台改造

在 Runes of Aldur 联盟机制中，由 Remnant 召唤的怪物有时会掉落 **Verisium** 矿物。这种矿物可用于符文锻造 (Runeforging) 来为物品赋予符咒护佑，或重铸传奇武器[^maxroll-runes-of-aldur]。

### 独特物品交互

某些传奇物品与符文锻造台有特殊交互：
- `The Brass Dome`：其护甲被完全替换为大量符咒护佑

## 提升符咒护佑

Runes of Aldur 引入了多种方式提升符咒护佑：
- 增加可获得的符咒护佑数量
- 直接修改符咒护佑机制

### 0.5.0 中的符文与合金

**符文 (Runes)**[^maxroll-runes-of-aldur]：
- **Ward Rune**：为插槽的护甲物品增加固定符咒护佑
- **Body Rune**（0.5.0 更新）：现在提供 +30/45/60 最大生命（护甲）、+30/40/50 最大能量护盾（法杖/魔杖）、或 3/4/5% 物理伤害偷取为生命（武器）[^maxroll-050-patchnotes]

**合金 (Alloys)**：
- 由 Remnants 掉落的另一种通货
- 允许为物品添加一个**保证的修饰词**，随机替换其他修饰词之一
- 例如 Expansive Alloy 可为头盔添加"增加法力消耗效率"，或为胸甲增加"增加存在范围效果"[^maxroll-runes-of-aldur]

**Meta Crafting**：
- 许多 Runsmithing 配方可创建 **Augment Runes**，插入物品以赋予额外修饰词
- **Cadigan's Epiphany**：摧毁物品中所有 augment 插槽并创建一个珠宝插槽
- **Aldur's Legacy**：摧毁任何 Kalguuran 或 Ezomyte 传奇物品，创建一个符文，以该传奇物品的力量强化同类型物品[^maxroll-runes-of-aldur]

## 符咒技能 (Kalguuran Skills) — 0.5.0

0.5.0 引入了超过 **40 种 Kalguuran Skills**，这些是特殊类型的技能宝石，由**符咒护佑 (Ward)** 而非法力 (Mana) 驱动[^maxroll-runes-of-aldur]：
- 例如 **Repulsion**：在伤害目标时击退周围敌人
- 这些技能可使用任何伤害类型
- **不需要属性要求**
- 符咒护佑随时间缓慢再生，以便在被用于技能或承受伤害后恢复

## 在防御体系中的位置

> **0.5.0 重要说明**："Defences" 关键词不再使用。现有用法现在明确指代 "Armour, Evasion and Energy Shield"，以明确这些修饰词**不适用于**符咒护佑、抗性、格挡或其他形式的保护[^maxroll-050-patchnotes]。

符咒护佑在伤害承受顺序中位于：
1. 能量护盾 (Energy Shield)
2. 生命 (Life)
3. **符咒护佑 (Runic Ward)** ← 此处
4. 死亡

## 相关机制

- **Runeforging**：符文锻造台，用于在装备上添加符咒护佑
- **Runes of Aldur**：联盟机制，可获得各种符文
- **生命 (Life)**：符咒护佑之前的资源层
- **能量护盾 (Energy Shield)**：符咒护佑之前的资源层
- **Sekhemas 试炼**：符咒护佑加到起始决心上

---

## 参考来源

[^mobalytics-runic-ward]: Mobalytics — PoE 2 Guide: Runic Ward Explained. https://mobalytics.gg/poe-2/guides/runic-ward
[^maxroll-runes-of-aldur]: Maxroll — Runes Of Aldur Overview. https://maxroll.gg/poe2/resources/runes-of-aldur-overview
[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
