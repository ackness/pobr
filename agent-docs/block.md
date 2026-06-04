# 格挡机制 (Blocking)

格挡是一种防御机制，可以**完全防止一次击中 (Hit) 的伤害**[^mobalytics-block]。格挡分为被动格挡、主动格挡（含招架）两种方式。

> **PoE2 关键差异**：PoE2 **取消了攻击 (Attack) 与法术 (Spell) 在防御上的区分**——对防御而言只有「Hit vs DoT」「AoE vs Projectile vs Strike」等标签有意义。因此**格挡作用于「打击 (Strike) 与投射物 (Projectile)」类击中，无论该击中在旧体系里算法术还是攻击**[^poe-forum-noblock][^mmojugg-monster]。**法术压制 (Spell Suppression) 与偏转 (Deflection) 在 PoE2 已被移除**（见下文）。

## 可被格挡的击中

- **默认所有非 Boss 击中均可格挡**[^mobalytics-block]。
- 可格挡的伤害形态为**打击 (Strike) 与投射物 (Projectile)**；大型范围 (AoE) 效果、以及部分 Boss 技能**不可格挡**（通常有红色闪光 + 音效预警，需要走位躲避）。
- 不可格挡的击中同样不可闪避——这是 PoE2 用来强制「该躲就躲」的设计。

## 格挡类型

### 被动格挡 (Passive Block)

- 基于装备/天赋的**格挡几率 (Block Chance)**，主要来自盾牌与天赋树左/下侧。
- **按每次击中独立滚动**，完全随机（与闪避的熵值机制不同）；滚成功则该次打击/投射物伤害**全部防止**，滚失败则不防止任何伤害。
- 格挡几率硬上限 **90%**（PoB2 确认：`data.misc.BlockChanceCap = 90`）[^pob2-deepwiki-block]。

### 主动格挡 (Active Block)

- 由部分力量系**盾牌技能**（如 `Raise Shield` 等）提供：举盾时，**保证格挡来自面朝方向的所有非 Boss 击中**。
- **招架 (Parry)**：圆盾 (Buckler) 提供的防御技能，每次招架只格挡**单次**可格挡击中。
- 主动格挡 / 招架与被动格挡共享其余机制；举盾/招架期间被动格挡**仍然生效**[^mobalytics-block]。

#### 主动格挡与重眩晕条 (Heavy Stun)

用主动格挡 / 招架挡下击中时，会按**被防止的伤害量**积累**重眩晕条**；条满则立即**结束举盾/招架并被重眩晕 3 秒**（默认）。不举盾时重眩晕条自然衰减，衰减速度可用 `Defender's Resolve` 等天赋改善[^mobalytics-block]。

> **0.5.0 更新**：招架 (Parry)、盾牌格挡 (Shield Block) 与共振盾 (Resonating Shield) 不再让重眩晕条的衰减延迟超出预期[^poe2wiki-050]。

## 格挡在计算中的位置与效果

### 计算位置：减伤之后 (post-mitigation)

格挡在伤害计算的**步骤 7** 发生（在减伤、抗性、伤害承受修饰词之后；见 [damage-defence-order.md](damage-defence-order.md)）。

```
防御计算顺序（节选）：
1. 闪避 (Evasion) / 命中检定
2. 减伤 (Armour / 抗性 / Mitigation)
3. 伤害承受修饰词 (Damage Taken)
4. 眩晕 (Stun) 判定
5. 格挡 (Block) ← 此处（post-mitigation）
```

> 「格挡在减伤之后」这一点很重要：基于「被格挡伤害」的效果（如 `Alkem Eira` 把被格挡的伤害以魔力 Recoup 返还）在**对该伤害类型防御越弱时收益越高**，因为返还量基于「若未格挡本会承受的伤害」[^mobalytics-block]。

### 格挡效果

当一次击中被格挡时：
- **默认防止该击中的所有伤害**；
- 但**击中本身仍然发生**（伤害视为 0，但仍「连接」）——格挡与闪避不同，被格挡的击中**已经走完伤害计算**；
- 因此**格挡不会阻止击中触发的效果**，且这些效果会**基于「被防止的伤害」结算**，包括：
  - 眩晕 / 重眩晕积累 (Stun / Heavy Stun buildup)
  - 冰冻 / 感电等异常积累 (Freeze / Shock buildup)
  - 其它 on-Hit 触发的 debuff

## 法术压制与偏转：PoE2 已移除

- **法术压制 (Spell Suppression)**：PoE1 中给闪避系提供对法术的减伤层。PoE2 因取消攻击/法术防御区分而**移除**——常规构建不再有法术压制[^mmojugg-monster][^poe-forum-noblock]。PoB2 中相关 stat 仅作为遗留代码（如时光珠宝路径）残留，**不应按 PoE1 语义理解**。
- **偏转 (Deflection)**：早期曾作为「攻击侧的法术压制对应物」试验，随攻击/法术区分取消而**移除**；后续是否以新形态回归以一手数据为准（参见 [active-defences.md](active-defences.md) 与 [evasion.md](evasion.md)）。

> 替代关系：在 PoE2 中，对法术形态的投射物/打击伤害，由**闪避**与**格挡**统一承接；对大型 AoE / Boss 技能则主要靠**走位规避**。

## 格挡恢复 (Block Recovery)

格挡后有短暂恢复时间，期间可能无法执行某些动作。可通过「增加格挡恢复」或「格挡时立即恢复」类修饰词缩短或消除。

## 获取格挡

| 来源 | 说明 |
|------|------|
| 盾牌 (Shield) | 主要格挡几率来源 |
| 天赋树 | 左/下侧的格挡几率、格挡恢复、特定武器/盾牌节点 |
| 升华职业 | 部分升华提供格挡几率、格挡时特殊效果、格挡恢复改进 |
| 传奇物品 | 可能提供特殊格挡效果（如被格挡伤害 Recoup） |

## 相关机制

- **闪避 (Evasion)**：命中前的随机防御层；与格挡一样作用于打击/投射物（见 [evasion.md](evasion.md)）
- **护甲 (Armour) / 抗性 (Resistances)**：减伤层，先于格挡结算
- **眩晕 / 重眩晕 (Stun)**：格挡不能阻止其积累，且主动格挡自身会积累重眩晕条（见 [stun.md](stun.md)）
- **走位规避**：应对不可格挡的大型 AoE / Boss 技能

## 对 pobr 实现的启示

- **格挡只是「按几率把一次 Hit 的伤害归零」**，在防御管线中位于减伤之后（post-mitigation）；pobr 的 defence 计算应把 block 作为 `(1 - blockChance)` 期望乘子放在减伤之后，而非命中前。
- **不要按攻击/法术二分建模防御**：PoE2 只需 `Hit/DoT` 与 `AoE/Projectile/Strike` 等标签；格挡/闪避的适用性由这些 tag 决定，对应 pobr 的 `flags`/`keyword_flags`。
- **被格挡仍触发 on-Hit 效果且基于被防止伤害**：眩晕/异常积累的计算输入是「未格挡时本会承受的伤害」，与伤害归零相互独立。
- **不要实现法术压制/偏转**：除非作为遗留兼容；默认 PoE2 无此两项。
- 主动格挡的重眩晕条是独立子系统，初版 minimal calc 可暂不建模，但需在数据/flags 上预留。

---

## 参考来源

[^mobalytics-block]: Mobalytics — PoE 2 Guide: Block Explained. https://mobalytics.gg/poe-2/guides/block
[^poe-forum-noblock]: Path of Exile 官方论坛 — "No spell block or suppress, what is it now?"（开发者/社区确认：格挡作用于命中法术，防御不再区分攻击/法术，只看 AoE/Projectile 与 Hit/DoT）。https://www.pathofexile.com/forum/view-thread/3655172
[^mmojugg-monster]: MMOJUGG — Path of Exile 2 Monster Damage System（攻击/法术区分移除；法术压制与偏转均移除；格挡/闪避作用于 Projectile 与 Strike）。https://www.mmojugg.com/news/path-of-exile-2-monster-damage-system.html
[^poe2wiki-050]: PoE2 Wiki — Version 0.5.0（Parry / Shield Block / Resonating Shield 重眩晕衰减修正）。https://www.poe2wiki.net/wiki/Version_0.5.0
[^pob2-deepwiki-block]: Path of Building for PoE2 DeepWiki — CalcDefence / Block Mechanics（`data.misc.BlockChanceCap = 90`）。https://deepwiki.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
</content>
</invoke>
