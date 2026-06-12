# M4-T1 W-A1 commit-3：ModFlags 位表双跑 diff 报告

> 蓝图节点：`blueprints/m4-offence-deep.md` §2-T1 W-A1 步骤 3。
> 结论先行：**diff=0**——切换前置条件满足；切换本身（翻默认 feature + 删旧
> 5 位常量 + 退役 `UsingMace` 类 condition 近似路径）属集成期独立 commit
> （蓝图步骤 4，依赖 T2 hand_pass 落地后的第二次双跑）。

## 1. 双跑配置

| 配置 | cargo 参数 | 位表 | 武器位双写通道 |
|---|---|---|---|
| legacy（默认） | （无） | 旧 5 位（ATTACK/SPELL/MELEE=1<<2/PROJECTILE=1<<3/AREA=1<<4） | 关（`weapon_flags` 恒 NONE） |
| pob2 | `--features pobr-build/modflags-pob2` | PoB2 全位表（位值逐位 == `Data/Global.lua:222-259`，MELEE/AREA/PROJECTILE 位值搬家 0x100/0x200/0x400） | 开（cfg 武器位 + mod 武器位 + `WeaponContribution::flags`） |

执行：`devs/scripts/modflags-dualrun.sh`（每配置 = `cargo nextest run
--workspace`（含 golden / dual-run 套件）+ `ninja_parity
parity_baseline_report --nocapture` 逐 build 逐值输出，滤构建噪声后 `diff -u`）。

## 2. 结果（2026-06-12，commit f7e1ef8 之后）

| 项 | legacy | pob2 | diff |
|---|---|---|---|
| workspace 测试 | 1677 passed / 2 skipped | 1681 passed / 2 skipped（+4 = feature-gated e2e/派生测试） | 全绿 |
| ninja_parity 防御 25 列 | 374/450 @5%、390/450 @10% | 同左 | **0** |
| 防御 core-8 | 130/144 @5%、133/144 @10% | 同左 | **0** |
| 进攻 | 26/80 @5%、35/80 @10% | 同左 | **0** |
| 逐 build 逐值输出（18 builds 全列） | — | — | **0 行 diff** |

## 3. diff=0 的结构性依据（为什么不是巧合）

1. **此时尚无人按新位消费**：双写产出的武器位只参与 `matches` 的子集判定
   `mod.flags ⊆ cfg.flags`；
2. mod 侧武器位与 cfg 侧武器位**同源**（同一张 `weapon_types.json` 经
   `ModFlags::weapon_flags` / vendor getWeaponFlags 派生）且 cfg 侧与
   `Using*` 条件**同 gating**（同一 cooldown-bound 守卫块）——对每条双写
   mod，bit 通道的判定结果蕴含于 condition 通道，两通道 AND ≡ 旧 condition
   单通道；
3. 既有 5 位（ATTACK/SPELL/MELEE/PROJECTILE/AREA）生产与消费两侧都引用命名
   常量，位值搬家对子集判定透明（全仓无 raw bits 构造点，`ModFlags` 无
   serde、fixture/build code 无落盘位值——蓝图步骤 5 检查项，实查 0 命中）。

## 4. 已知差异面（feature 下新增能力，不影响本次 diff）

- `with unarmed attacks` 短语仅 feature 下解析（vendor ModParser.lua:1006；
  legacy 维持 Unsupported）。18 个 parity build 与 golden 无此词条（实查 0
  命中），故 diff=0；切换 commit 时此短语将常驻解锁。
- vendor 同短语还并 `ModFlag.Hit`——Hit 位产出依赖 per-hand cfg 供位
  （T2 W-B2），与切换同期接线（mod_parser `weapon_suffix_bits` doc 已登记）。

## 5. 后续（非本 track）

- T2 hand_pass 落地（per-hand cfg 开始消费武器位）后再跑本脚本：parity
  只升不降 → 翻默认 feature → 删旧 5 位常量与 `UsingMace` 类 condition
  近似路径（退役放 M4 末单独 commit）。
