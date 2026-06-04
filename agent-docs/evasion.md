# 闪避机制 (Evasion)

闪避是一种防御属性，提供闪避所有 incoming 击中（除了不可避免的 Boss 预警技能）的机会。当一次击中被闪避时，该击中被完全丢弃，不会进入正常的伤害防御计算顺序[^mobalytics-evasion]。

## 基本机制

闪避仅对**击中 (Hits)** 有效，不 interacts with 持续伤害。例如，你不能闪避地面持续伤害效果。

**注意**：通过翻滚 (Dodge Roll) 躲避击中**不被视为闪避**。

一次击中必须连接到目标才能施加 damaging ailment（如**流血 (Bleeding)**），因此闪避可以防止某些持续伤害效果的施加。

## 闪避几率公式

```
闪避几率 = (0.95 * DE) / (DE + 4 * AA) * 100
```

> **与 POB2 的核对**：Path of Building for POE2 的 `calcs.monsterHitChance`（`CalcDefence.lua`）计算怪物击中几率为 `(1 - (0.95 * evasion) / (evasion + 4 * accuracy)) * 100`，闪避几率即为其补数[^pob2-deepwiki-evasion]。旧版 POE1 公式为 `accuracy / (accuracy + (evasion / 5) ^ 0.9) * 125`，POE2 已完全重新设计。

其中：
- **DE** = 防御者的总闪避值 (Evasion Rating)
- **AA** = 攻击者的总精准值 (Accuracy Rating)

**闪避几率硬上限为 95%**（对应最低 5% 命中几率），因此攻击者的命中几率不能低于 5%。

## 熵值机制 (Entropy Mechanic)

熵是一个确保不会出现连续幸运击中或连续闪避的 streak 的系统。

当一个实体在 100 个服务器 tick（约 3.333 秒）内第一次拦截到可以被闪避的击中时，会在 0 到 99 之间随机滚动一个熵值。攻击者的命中几率被加到熵值上：
- 如果熵值达到或超过 100，击中连接，并从熵值中减去 100
- 如果熵值小于 100，击中被闪避，不减去任何值

**注意**：熵是一个角色属性，因此所有击中该角色的实体共享同一个熵值。

### 熵值机制示例

1. 一个怪物向玩家施放火球
2. 这是玩家角色在至少 100 个服务器 tick 内的第一次拦截击中，因此熵值在 0-99 之间随机滚动，结果为 68
3. 怪物的精准值对玩家的闪避值，火球有 23% 的命中几率
4. 命中几率加到熵值上：68 + 23 = 91
5. 熵值小于 100，击中被闪避
6. 另一个怪物也向玩家施放火球
7. 该怪物也有相同的精准值，同样 23% 命中几率
8. 加到熵值上：91 + 23 = 114
9. 熵值达到 100，击中连接，减去 100 后变为 14

**注意**：如果实体在超过 100 个服务器 tick 内没有拦截到可以被闪避的击中，现有熵值将被丢弃。

## 闪避与暴击

闪避还提供固有的防止 incoming 可闪避击中成为**暴击 (Critical Hit)** 的机会。

如果一个将成为暴击的击中了通过初始命中检查并确认不会被闪避，它需要通过**二次检查**来确认暴击。此时，在 1 到 100 之间滚动一个随机数（完全独立于熵值机制）。如果攻击者的命中几率高于该随机数，暴击被确认；如果等于或低于，暴击被降级为普通击中。

**注意**：即使二次检查失败，击中仍然会连接，因为它已经通过了初始检查。

## 偏转 (Deflection)

偏转属性通常与闪避一起获得[^mobalytics-evasion]。偏转使用类似闪避的计算方式额外滚动一次，在成功时提供伤害减免。

> **0.5.0 更新**：偏转公式已调整[^maxroll-050-patchnotes]。
>
> **与 POB2 的核对**：POB2 的 `calcs.deflectChance` 计算偏转几率为 `100 - clamp((accuracy * 0.9) / (accuracy + deflection * 0.2) * 100, 0, 100)`，即：
> ```
> 偏转几率 = (0.1 * A + 0.2 * D) / (A + 0.2 * D) * 100
> ```
> 其中 A = 攻击者精准值，D = 防御者偏转值。当偏转为 0 时基础偏转几率为 10%。
>
> 某些社区来源给出的公式 `150 * (1 - A / (A + 0.12 * D))` 与 POB2 提取的数据不同，请以实际游戏为准。

## 绕过闪避

某些修饰词会使击中完全绕过命中计算。
- **"Your Hits can't be Evaded"**：强制任何通常需要精准检查的攻击完全忽略命中几率计算
- **"Cannot Evade"**：使攻击者的精准值和防御者的闪避值在命中计算之外无效

## 相关机制

- **精准 (Accuracy)**：与闪避相对
- **护甲 (Armour)**：另一种主要防御层
- **盲 (Blind)**：降低目标闪避
- **钢铁反射 (Iron Reflexes)**：将闪避转换为护甲

---

## 参考来源

[^mobalytics-evasion]: Mobalytics — PoE 2 Guide: Evasion Explained. https://mobalytics.gg/poe-2/guides/evasion
[^poewiki-evasion]: PoE Wiki — Evasion. https://www.poewiki.net/wiki/Evasion
[^maxroll-050-patchnotes]: Maxroll — 0.5.0 Patch Notes – Return of the Ancients. https://maxroll.gg/poe2/news/0-5-0-patch-notes-return-of-the-ancients
[^pob2-deepwiki-evasion]: Path of Building for PoE2 DeepWiki — CalcDefence / monsterHitChance & deflectChance. https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
