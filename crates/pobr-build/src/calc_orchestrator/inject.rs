//! `calculate_with_data` 的注入阶段（主脉拆分：行为不变，纯分组）。
//!
//! 下列 `inject_*` 自由函数从 `calculate_with_data` 主脉逐段抽出，每个对应原主脉
//! 一个自包含注入阶段（仅依赖 session + build/data/options，无跨阶段中间状态），
//! 调用顺序与原内联顺序逐字一致 → 零行为变化（parity 门禁逐值兜底）。
//!
//! 与 `mod.rs` 的分工：`mod.rs` 留编排主干（`calculate*` 入口 + `stage_*` 家族），
//! 本模块承载「往 session 里灌词条」这一类同构工作。

// 编排主干（`mod.rs`）的类型与兄弟模块 helper 经 glob 引入——与同目录其余
// `calc_orchestrator::*` 子模块一致的风格。
use super::*;

/// 1d 阶段：装备基底防御（armour/evasion/ES）+ 盾基底格挡 + 件级 Spirit/Ward → BASE 词条。
pub(super) fn inject_defence_base(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
    // 1d. 装备基底防御（armour/evasion/ES）→ Item 归因的 BASE 词条（× 品质）。装备的
    //     `increased Armour/Evasion/EnergyShield` 词条经 add_item 注入 INC，于此 base 上缩放。
    session.add_modifiers(defence_base_modifiers(build, data));
    // 1d'. 盾牌基底格挡 → `ShieldBlockChance` BASE（13-G8）。
    //      PoB2 CalcDefence.lua:975-980 读 Weapon 2/3 `armourData.BlockChance`
    //      作为盾基底；catalog 值经 overlay/base_item_overrides merge 注入。
    session.add_modifiers(shield_block_modifiers(build, data));
    // 1d''. 件级 Spirit（权杖 rolled `Spirit:` 行 / catalog 基底 spirit）→
    //       `Spirit` BASE（13-G11；PoB2 CalcSetup.lua:1275-1277
    //       `item.spiritValue → NewMod("Spirit","BASE")` 等价）。
    session.add_modifiers(item_spirit_modifiers(build, data));
    // 1d'''. 件级 Ward（rolled `Ward:` 行 / catalog 基底 ward）→ `Ward` BASE
    //        （13-G14；PoB2 CalcDefence.lua:1158-1186 armourData.Ward
    //        per-slot 聚合等价）。
    session.add_modifiers(item_ward_modifiers(build, data));
}

/// 2b'' 阶段：激活态药剂/护符载荷注入（env_finalize 阶段 3 合并消费）。
pub(super) fn inject_flasks_charms(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
    // 2b''. 激活态药剂/护符（PoB `<Slot name="Flask N|Charm N" active="true">`，
    //       xml_build 已按 `active` 门控——vendor CalcSetup.lua:1014-1028 `slot.active`
    //       决定 env.flasks/charms）：经 `ingest_flask_charm` 打包为 FlaskBuff/
    //       CharmBuff 载荷注入 session（通道切换，替代旧「原值直注」路径），
    //       由 env_finalize 阶段 3 merge_flasks_charms 在 mode_combat 门控下按
    //       effect 乘区合并 + UsingFlask/UsingCharm 条件置位（vendor
    //       CalcPerform.lua:1429-1663）。charm 需 CharmLimit 来源（腰带 implicit
    //       等）方进预算（:1589）；不可解析行（触发/恢复行）skip-and-collect。
    for (slot_name, item) in &build.utility_slots {
        // charm 基底固有 buff（如 Ruby Charm `+25% to Fire Resistance`）**不在物品
        // 文本里**，是基底属性（vendor `Item.lua:838-844` 把 `base.charm.buff` 逐行
        // 并入 `buffModList`）。从 base_items 取 `charm_buff` 并入物品的 implicit
        // 文本流，使 `ingest_flask_charm` 一并打包进 CharmBuff 载荷（归因同 charm 槽，
        // merge 阶段一并 effect-scale）。无 buff（非 charm / 免疫类未建模）→ 直注原件。
        //
        // 名称匹配：magic charm 的 `item.base` 是物品全名（前缀+base+后缀单行，
        // `parse_base` 取唯一名称行），精确名查不到 base_items；charm base 名
        // （"Ruby Charm" 等 13 个）互不为子串，用「全名 contains base 名」可靠定位
        // （normal/rare 全名 = base 名亦命中）。
        let item_name = item.base.to_string();
        let base_buff: &[String] = data
            .base_items
            .values()
            .filter(|def| !def.charm_buff.is_empty())
            .find(|def| item_name.contains(def.name.as_str()))
            .map(|def| def.charm_buff.as_slice())
            .unwrap_or_default();
        if base_buff.is_empty() {
            session.add_flask_charm(slot_name, item);
        } else {
            let mut augmented = item.clone();
            augmented.implicit_texts.extend(base_buff.iter().cloned());
            session.add_flask_charm(slot_name, &augmented);
        }
    }
}

/// 4 阶段：技能宝石按 active/support 分类经各自归因入口注入。
pub(super) fn inject_skill_gems(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) -> Result<(), BuildError> {
    // 4. 技能宝石：按 active/support 分类，经各自归因入口注入。
    for gem in resolve_gems(build, data) {
        if gem.is_support {
            session
                .add_support_gem(&gem)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        } else {
            session
                .add_skill_gem(&gem)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }
    }
    Ok(())
}

/// 4b/4b'/4b'' 阶段：光环/诅咒 BuffSpec + support 授予 buff + herald 在场计数/条件注入。
pub(super) fn inject_buffs_and_heralds(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
    // 4b. 光环/诅咒技能 → BuffSpec 经 `session.add_buff_skill` 注入（§2.4 契约），
    //     消费在 pobr-core buff_pass（env_finalize 阶段 4；上方已置 cfg.mode_buffs）：
    //     aura 防御 buff（Discipline→EnergyShield、Purity of Fire→FireResistance…）
    //     吃 AuraEffect 系乘区（CalcPerform.lua:2102-2105）后并入 player db；curse
    //     走 priority/limit/分槽（:2829-2896）。C5-2 切换前的 `aura_buff_modifiers`
    //     静态直注已关。
    for spec in buff_skill_specs(build, data) {
        // buff 载荷中的 `Multiplier:<X>` BASE → cfg.multipliers 桥
        // （vendor GetMultiplier 对 modDB `Multiplier:<X>` 全局求和取数，
        // ModStore.lua:369；PoBR 的 ModTag::Multiplier 读 cfg.multipliers
        // 预灌表，需在此显式回填）。首个消费方 = Sigil of Power
        // `Multiplier:SigilOfPowerMaxStages` BASE 4（per-stage MORE 的
        // limitVar 分母）。
        for m in &spec.mods {
            if let Some(var) = m.name.as_str().strip_prefix("Multiplier:")
                && m.mod_type == ModType::Base
                && let Some(v) = m.value.as_number()
            {
                session.set_multiplier(var, v);
            }
        }
        session.add_buff_skill(spec);
    }
    // 4b'.support 授予的玩家侧 buff（Precision I/II → Accuracy INC，
    //     sup_dex.lua:4181-4250）→ BuffSpec(kind=Buff)，buff_pass Buff 分支
    //     （CalcPerform.lua:1949-1962）施 BuffEffect 乘区后并入 player db。
    for spec in support_buff_specs(build, data) {
        session.add_buff_skill(spec);
    }

    // 4b'''.（存量 #9）warcry 技能 → WarcrySpec 经 `session.add_warcry_skill` 注入，
    //     消费在 pobr-core `calc::warcry`（perform 的 hand pass 之前）：按
    //     `min((賦能次数/主技能Speed)/(冷却+喊叫时间), 1)` 折算 uptime 后把 warcry
    //     进攻效果（Infernal `DamageGainAsFire`）缩放注入（CalcOffence.lua:3203-3256）。
    for spec in warcry_skill_specs(build, data) {
        session.add_warcry_skill(spec);
    }

    // 4b''.herald 在场计数/条件（vendor CalcPerform.lua:1792-1805，
    //     mode_buffs 段——本编排路径恒置 mode_buffs=true）：已启用组中
    //     skill_types 含 Herald 的主动技能按显示名去重 → `Multiplier:Herald`
    //     = 数量 + `Condition:AffectedByHerald`；并逐 herald 置
    //     `AffectedBy<名去空格>`（vendor buff 分支命名 `buff.name:gsub(" ","")`，
    //     "Herald of Plague" → AffectedByHeraldofPlague——of 保持小写）。
    //     消费方 = mod_parser 的 herald 条件后缀族（ModParser.lua:1826/:6326-6328）。
    let heralds = herald_skill_names(build, data);
    if !heralds.is_empty() {
        session.set_multiplier("Herald", heralds.len() as f64);
        session.set_condition("AffectedByHerald", true);
        for name in &heralds {
            session.set_condition(format!("AffectedBy{}", name.replace(' ', "")), true);
        }
    }
}

/// 6b 阶段：PoE2 属性派生（最终 Str/Dex/Int → Life/Mana/Accuracy 增量），须在全部来源注入后。
pub(super) fn inject_attribute_derivation(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) {
    // 6b. 属性派生（PoE2）：life/mana/accuracy 须用**最终**属性（职业基础 + 装备/树/珠宝
    //     的 +Strength/Dex/Int，并经 `N% increased <Attr>` 缩放——PoB2
    //     `calculateAttributes`，CalcPerform.lua:381-388
    //     `output[stat] = m_max(round(calcLib.val(modDB, stat)), 0)`）。
    //     character_base 已注入「未经 INC 缩放的职业起始」派生部分；此处补注入
    //     `最终总量 − 职业起始` 的增量（2 life/力量、2 mana/智力、6 accuracy/敏捷，
    //     vendor :424-441 Life/Accuracy/Mana from Str/Dex/Int），须在全部来源注入后。
    if options.inject_character_base {
        // PoE2 属性派生系数（每点力量 +2 生命、每点智力 +2 魔力、每点敏捷 +6 精准）：
        // 起自注入的 character_constants 域读取，与 CharacterBase 派生同一来源。
        let cc = &data.constants.character_constants;
        // 职业起始属性（CharacterBase 烘焙部分；未知职业 = 未注入 CharacterBase → 0）。
        let cls = character_base(build, data);
        let (cls_str, cls_dex, cls_int) = cls
            .map(|c| (c.strength, c.dexterity, c.intelligence))
            .unwrap_or((0.0, 0.0, 0.0));
        let str_total = session.attribute_total("Strength", cls_str);
        let dex_total = session.attribute_total("Dexterity", cls_dex);
        let int_total = session.attribute_total("Intelligence", cls_int);
        // （存量 #7-4）Giant's Blood 键石「Inherent Life granted by Strength is
        // halved」（vendor CalcPerform.lua:500-505：flag HalvesLifeFromStrength →
        // `Life BASE = Str × 1` 而非 ×2）。CharacterBase 已烘焙职业起始段
        // `cls_str × life_per_strength`，此处增量按「目标总量 − 烘焙段」注入，
        // 使 Str 派生生命总量 = str_total × 减半后系数（wolf-pack 802→401，
        // oracle Life 逐源钉值）。
        let life_per_str = if session.has_flag("HalvesLifeFromStrength") {
            cc.life_per_strength / 2.0
        } else {
            cc.life_per_strength
        };
        let mk = |stat: &str, value: f64| {
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::CharacterBase,
                "base.attr_derived",
            ))
            .with_raw_text(format!("{stat} from attributes"));
            Modifier::number(stat, ModType::Base, value).with_origin(origin)
        };
        session.add_modifiers([
            mk(
                "MaximumLife",
                str_total * life_per_str - cls_str * cc.life_per_strength,
            ),
            mk(
                "MaximumMana",
                cc.mana_per_intelligence * (int_total - cls_int),
            ),
            mk(
                "Accuracy",
                cc.accuracy_per_dexterity * (dex_total - cls_dex),
            ),
        ]);
    }
}

/// 6c 阶段：per-X 资源/属性缩放量回填（PoB2 PerStat 分母变量），须在全部来源注入后、perform 前。
/// 已装辅助宝石按颜色计数 → `Red/Green/BlueSupportGems` multipliers（PoB2
/// CalcSetup.lua:2015-2044：遍历 **enabled** socket group，按辅助宝石
/// `grantedEffect.color`（1=R/2=G/3=B，与 GGG `gem_colour` 同枚举）计数后写
/// `env.modDB.multipliers`）。消费方 = a2-real-gaps 钉值条目的
/// `MultiplierThreshold{<Color>SupportGems, 10}`（下界盲产，缺键=不生效——本
/// 注入接通后自动激活）。vendor 同址的 `Majority<Color>SocketedSupports`
/// conditions 暂无 PoBR 数据消费者，不注入（YAGNI，接需求时同函数补）。
pub(super) fn inject_support_gem_counts(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
    let (mut r, mut g, mut b) = (0.0_f64, 0.0_f64, 0.0_f64);
    for group in build.enabled_socket_groups() {
        for gem_id in &group.gem_ids {
            let Some(def) = data.skill_gems.get(gem_id) else {
                continue;
            };
            if !def.is_support {
                continue;
            }
            match def.gem_colour {
                Some(1) => r += 1.0,
                Some(2) => g += 1.0,
                Some(3) => b += 1.0,
                _ => {}
            }
        }
    }
    session.set_multiplier("RedSupportGems", r);
    session.set_multiplier("GreenSupportGems", g);
    session.set_multiplier("BlueSupportGems", b);
}

pub(super) fn inject_per_x_multipliers(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
) {
    // 6c. per-X 资源/属性缩放量回填（PoB2 PerStat 分母变量）：把全部来源注入后的属性 /
    //     Spirit BASE 总量与角色等级写入 cfg.multipliers，使 `+N to <stat> per M <resource>`
    //     这类词条（解析为 ModTag::Multiplier{var, div}）在 perform 查询时按 count/div 展开。
    //     须在全部来源注入后、perform 之前；属性/Spirit 不参与 per-X 自缩放，base_sum 取值稳定。
    //     Life/Mana 分母 = **全管线池值**（OVERRIDE → base×(1+inc)×more，
    //     `CalculationSession::pool_total`，与 perform 内 offence 池计算同源）——vendor
    //     PerStat 读 actor **output**（ModStore.lua:440-460 GetStat → output.Mana/Life），
    //     BASE-only 会把「3% increased Spell Damage per 100 maximum Mana」（druid
    //     ember-fusillade Tree:19044，vendor 档位 234 = 3×floor(7889/100)）严重欠算。
    let str_total = session.base_sum("Strength");
    let dex_total = session.base_sum("Dexterity");
    let int_total = session.base_sum("Intelligence");
    // （存量 #7-4）Spirit 分母 = **最终池值**（calc_spirit_pool，含 INC/MORE 与
    // 转换扣减）——vendor PerStat 读 output.Spirit；BASE-only 会把 wolf-pack
    // Perfidy「+2 Armour per 1 Spirit」欠算 72 base（Spirit 336 vs base 300）。
    let spirit_total = session.spirit_total();
    let mana_total = session.pool_total("MaximumMana");
    let life_total = session.pool_total("MaximumLife");
    session.set_multiplier("Strength", str_total);
    session.set_multiplier("Dexterity", dex_total);
    session.set_multiplier("Intelligence", int_total);
    session.set_multiplier("Spirit", spirit_total);
    session.set_multiplier("Mana", mana_total);
    session.set_multiplier("Life", life_total);
    session.set_multiplier("Level", f64::from(build.character.level));
    // cfg.stats 快照回填（同值镜像）：PerStat/PercentStat（EvalContext::stat 回退
    // cfg.stats）与 StatThreshold（matches gate）的取数通道，键空间与 multiplier
    // 侧一致（special_mod::normalize_stat_name 归一后对齐）。仅回填 perform 前
    // 可算的子集；perform 内才算出的全局 Armour/ES 等留 0（见 CalcConfig::stats doc）。
    session.set_stat("Strength", str_total);
    session.set_stat("Dexterity", dex_total);
    session.set_stat("Intelligence", int_total);
    session.set_stat("Spirit", spirit_total);
    session.set_stat("Mana", mana_total);
    session.set_stat("Life", life_total);
    // 主技能 Life 消耗快照（vendor output.LifeCost）：per-life-cost 词条
    // （PerStat stat=LifeCost，如 Atalui's Bloodletting gain-as-physical）的取数源。
    // 消耗先于伤害结算，与 vendor CalcOffence 顺序一致。
    let life_cost = session.life_cost_snapshot();
    if life_cost > 0.0 {
        session.set_stat("LifeCost", life_cost);
        session.set_multiplier("LifeCost", life_cost);
    }
    // per-槽位防御缩放（`<Stat>On<Slot>`）：使 `+N to Armour per M Item Energy Shield on
    // Equipped Boots` 这类按某件装备防御值缩放的词条生效（PoB2 PerStat `<Stat>On<Slot>`）。
    for (var, value) in per_slot_defence_multipliers(build, data) {
        session.set_stat(var.clone(), value);
        session.set_multiplier(var, value);
    }
    // per-槽位已填充 socket 数（`RunesSocketedIn<slot>`）：使 `+N to <stat> per Socket
    // filled` 这类按本件已镶嵌符文/魂核数缩放的词条生效（PoB2 ModParser.lua:1477-1478）。
    for (var, value) in per_slot_socket_multipliers(build) {
        session.set_multiplier(var, value);
    }
    // GrenadeTypes（vendor CalcPerform.lua:1238-1242：去重统计已启用
    // 主动技能中 `SkillType.Grenade` 的不同授予效果数）——Demolitionist
    // 「… for every different Grenade fired …」的 Multiplier limitVar 分母。
    session.set_multiplier("GrenadeTypes", grenade_type_count(build, data));
    // Gemling 升华 Virtuous Barrier 的 per-Attribute-Mote 计数（vendor
    // CalcSetup.lua:1396,1766-1781）：base {Str,Dex,Int}=3，每个启用的非辅助技能
    // 宝石按其必需属性（str/dex/int_pct>0）计——单属性 +2、多属性各 +1。仅
    // Virtuous Barrier 的 `<res> INC ×<Attr>MoteSkillCount` 消费（本仓库唯一来源），
    // 非该升华的 build 这三个 multiplier 无人引用 → 零行为。
    // ponytail: 当前未按 vendor 排除 fromNode/fromItem 授予技能；现无授予技能带属性
    // 需求会污染计数。将来出现相关 build 时，可据 SocketGroup::source 精确排除。
    let (str_mote, dex_mote, int_mote) = virtuous_mote_counts(build, data);
    session.set_multiplier("StrengthMoteSkillCount", str_mote);
    session.set_multiplier("DexterityMoteSkillCount", dex_mote);
    session.set_multiplier("IntelligenceMoteSkillCount", int_mote);
    // Smith of Kitava 身甲连接 notable 计数（vendor CalcSetup.lua:840-841：
    // 已分配且 tree.lua `applyToArmour=true` 的 notable 数 →
    // `Multiplier:AllocatedConnectedNotable`）。消费方 = Masterwork
    // 『+200 to Armour for each Connected Notable Passive Skill Allocated』。
    let connected_notables = build
        .tree
        .allocated_nodes
        .iter()
        .filter(|id| {
            data.passive_nodes
                .get(&id.0)
                .is_some_and(|n| n.apply_to_armour)
        })
        .count();
    if connected_notables > 0 {
        session.set_multiplier("AllocatedConnectedNotable", connected_notables as f64);
    }
    // 装备属性需求快照（vendor CalcPerform.lua:1848-1857：
    // `output[attr.."RequirementsOn"..slot] = floor(itemReq × reqMult)`）——
    // 『Gain Armour equal to N% of total Strength Requirements of Equipped
    // Boots, Gloves and Helmet』（PercentStat `StrRequirementsOn<slot>`）取数源。
    // ponytail: reqMult（GlobalAttributeRequirements 词条族）恒按 1；出现带
    // 「reduced attribute requirements」的相关 build 时再接乘子。
    for (var, value) in per_slot_attribute_requirements(build, data) {
        session.set_stat(var, value);
    }
}

/// 每槽位装备的属性需求（`{Str,Dex,Int}RequirementsOn<slot>` → 值）。
/// 槽位词根与 PercentStat tag 的 stat 名对齐（`StrRequirementsOnboots` 等，
/// 小写槽名 = 引擎解析产物）；无需求/空槽不产出。
pub(super) fn per_slot_attribute_requirements(
    build: &Build,
    data: &BuildData,
) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for (slot, item) in build.equipped_items() {
        let Some(def) = data.base_items.get(&item.base.to_string()) else {
            continue;
        };
        let slot_key = slot.id();
        for (attr, req) in [
            ("Str", def.req_str),
            ("Dex", def.req_dex),
            ("Int", def.req_int),
        ] {
            if req > 0 {
                out.push((format!("{attr}RequirementsOn{slot_key}"), f64::from(req)));
            }
        }
    }
    out
}

/// Attribute-Mote 计数（Gemling Virtuous Barrier）：base 3/3/3 + 每个启用非辅助
/// 技能宝石按必需属性数计（单属性 +2、多属性各 +1）。返回 `(Str, Dex, Int)`。
pub(super) fn virtuous_mote_counts(build: &Build, data: &BuildData) -> (f64, f64, f64) {
    let (mut s, mut d, mut i) = (3.0, 3.0, 3.0);
    for group in build.enabled_socket_groups() {
        for gem_id in &group.gem_ids {
            let Some(def) = data.skill_gems.get(gem_id) else {
                continue;
            };
            if def.is_support {
                continue;
            }
            let req = [def.str_pct > 0, def.dex_pct > 0, def.int_pct > 0];
            let n_attr = req.iter().filter(|&&r| r).count();
            if n_attr == 0 {
                continue;
            }
            let mote = if n_attr == 1 { 2.0 } else { 1.0 };
            if req[0] {
                s += mote;
            }
            if req[1] {
                d += mote;
            }
            if req[2] {
                i += mote;
            }
        }
    }
    (s, d, i)
}

/// 6d 阶段：来源授予的条件 flag → cfg 条件桥接（Bonded modifiers / Arcane Surge）。
pub(super) fn inject_condition_bridges(session: &mut CalculationSession) {
    // 6d. 来源授予的条件 flag → cfg 条件桥接：如「Gain the benefits of Bonded modifiers on
    //     Runes and Idols」授予 `Condition:CanUseBondedModifiers` flag 后，符文 `Bonded: <mod>`
    //     词条（挂 Condition tag）才生效（PoB2 ModParser `["^bonded: "]` 语义）。
    if session.has_flag("Condition:CanUseBondedModifiers") {
        session.set_condition("CanUseBondedModifiers", true);
    }
    // 奥术涌动桥（vendor CalcDefence.lua:1580-1582：`Condition:ArcaneSurge` flag →
    // `AffectedByArcaneSurge` 条件）：树/词条授予的「chance to Gain Arcane Surge …」
    // FLAG（含 CritRecently 等触发条件 tag，按当前 cfg 求值）为真时，使
    // 「while you have Arcane Surge」族词条（Condition:AffectedByArcaneSurge tag）
    // 生效。druid ember-fusillade：Tree:27388 激活源 → Tree:16940 +30 INC。
    if session.has_flag("Condition:ArcaneSurge") {
        session.set_condition("AffectedByArcaneSurge", true);
    }
    // Chaos Inoculation → FullLife 桥（vendor CalcDefence.lua:123-126：CI 时 `output.Life=1`
    // 且 `condList["FullLife"]=true`——CI build 恒视为满生命）。PoBR 既有 CI 接线只建模
    // Life=1 / 混沌免疫（perform.rs:320-334 EhpOptions），未把 FullLife 条件桥到 cfg，
    // 致「while on Full Life」族增伤（如 Tree:56453 +40% Attack Damage）在 CI build 上失效。
    // 仅 CI build 触发（flicker：AvgDamage 0.90x→0.99x）；非 CI build（含满生命的普通 build）
    // 不受影响——FullLife 在 PoB 由实际生命态决定，普通满生命 build 的判定属另一档（未建模），
    // 此处只补 vendor 明文的 CI 分支，避免全局置真对 deadeye 等的过量（实测全局置真 off −2）。
    if session.has_flag("ChaosInoculation") {
        session.set_condition("FullLife", true);
    }
}

/// 5/5a/5b 阶段：敌人配置（setup_enemy）+ config 解释器 enemy 桶 + 玩家施加的元素曝光。
pub(super) fn inject_enemy(
    session: &mut CalculationSession,
    build: &Build,
    options: &DataOrchestratorOptions,
    enemy_tier: EnemyTier,
    resolved_config: &crate::config_resolve::ResolvedConfig,
) {
    // 5. 敌人 + 有效 DPS：setup_enemy 写 enemy 缩放/抗性/减伤；mode_effective 已在 cfg。
    //    敌人等级解析对齐 vendor（CalcSetup.lua:529 `env.enemyLevel =
    //    build.configTab.enemyLevel or m_min(data.misc.MaxEnemyLevel, charLevel)`）：
    //    调用方显式等级（编排选项 ≠0）优先；否则 build XML Config 的 `enemyLevel`
    //    标量；两者皆缺回落 0 → setup_enemy 内部按 min(MaxEnemyLevel, 角色等级) 推导。
    let enemy_level = if options.enemy_level != 0 {
        options.enemy_level
    } else {
        config_enemy_level(build).unwrap_or(0)
    };
    session.setup_enemy(enemy_level, enemy_tier);

    // 5a'. config 解释器的 enemy 桶产物：enemy 条件 actor 化
    //      条目（vendor `enemyModList:NewMod("Condition:<X>", FLAG, ...)`，带
    //      `Condition:Effective` tag + EnemyConfig 归因）。`mode_effective=false`
    //      下天然惰性；cfg 侧 `Enemy<X>` 条件由 `config_resolve` 反桥维持既有语义。
    if !resolved_config.enemy_mods.is_empty() {
        session.add_enemy_modifiers(resolved_config.enemy_mods.clone());
    }

    // 5b. 玩家施加的元素曝光（build config `conditionEnemy*Exposure`）→ enemy 抗性减项
    //     （PoB2 config 默认每点 -20%）。仅有效口径生效，须在 setup_enemy 后。
    if options.mode_effective {
        let exposure = [
            resolved_config
                .config
                .conditions
                .get("EnemyFireExposure")
                .copied(),
            resolved_config
                .config
                .conditions
                .get("EnemyColdExposure")
                .copied(),
            resolved_config
                .config
                .conditions
                .get("EnemyLightningExposure")
                .copied(),
        ]
        .map(|c| c.unwrap_or(false));
        if exposure.iter().any(|&on| on) {
            session.apply_enemy_exposure(exposure, EXPOSURE_MAGNITUDE);
        }
    }
}

/// 1b/1b-ii/1c 阶段：主技能 base mod / 品质 / 未选 set / DoT flag / 尸爆 / 弩 reload /
/// support / trigger 注入 + 技能伤害倍率 MORE + 武器基底暴击。
pub(super) fn inject_main_skill_mods(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    main_skill: &Option<(ResolvedSkillLevel, &SocketGroup, &str)>,
    weapon: Option<&WeaponContribution>,
    dmg_mult: f64,
) {
    // 1b. 主技能 cost / cooldown / 基础伤害 + 该组 support 宝石倍率 → 归因 modifier。
    // 攻速/施法速度全部走通用链路（充能 / support more / 技能 quality / attackSpeedMultiplier），
    // 不再有单技能硬编码。
    if let Some((skill, group, skill_id)) = main_skill {
        // 选中 statSet 的 per-set 覆盖键（statSetIndex 显式选择接进引擎 set_key）。
        let main_set_key = group
            .gem_skills
            .iter()
            .find(|g| g.skill_id == *skill_id)
            .and_then(|g| data.selected_set_key(skill_id, g.stat_set_index));
        session.add_modifiers(skill_base_modifiers(
            skill,
            skill_id,
            main_set_key.as_deref(),
        ));
        // 1b-i-q. 主技能宝石品质 stat（T1.7）：quality 段经 stat-map 映射注入，
        //         SourceKind::GemQuality 归因（id 前缀 gem.<效果 id>.q<Q>）。
        session.add_modifiers(main_skill_quality_modifiers(group, data, skill_id));
        // 1b-i-g. 主技能未选 statSet 的 global-only merge（CalcActiveSkill.lua:124-140）。
        session.add_modifiers(unselected_set_global_modifiers(group, data, skill_id));
        // 1b-i-d. 选中 statSet 的 dotIs* 旗标 → `DotIs<X>` FLAG（
        //         statSet baseMods 直挂布尔，calc::skill_dot 据此保留 dotCfg 位）。
        session.add_modifiers(dot_flag_modifiers(group, data, skill_id));
        // 1b-i-c. 尸体爆炸基伤：explodeCorpse 门控 statSet 的
        //         `monsterLife × corpseExplosionLifeMultiplier` → Physical
        //         BASE（vendor CalcOffence.lua:2211-2217；如 Detonate Dead）。
        session.add_modifiers(corpse_explosion_modifiers(
            build, data, options, group, skill, skill_id,
        ));
        // 1b-i-x. 弩 reload 数据通道：CrossbowReloadTimeBase（武器
        //         reload_time_ms）+ CrossbowBoltCount（ammo 兄弟技能 stat），
        //         perform `fill_crossbow_reload` 消费。非弩/grenade 返回空。
        session.add_modifiers(crossbow_reload_modifiers(build, data, group, skill_id));
        session.add_modifiers(support_modifiers(group, data, skill_id));

        // 1b-iii. 触发链路：
        // ① 数据驱动识别（trigger_configs.json 四级 key → 组内宝石/主技能 id 匹配）；
        // ② 内建触发（`skill_types` 含 `Triggered`/`InbuiltTrigger`，PoB2 `isTriggered`）。
        // 注入触发冷却 + 触发源**子计算**统计（计算后攻速/命中/暴击）BASE，驱动 perform
        // `fill_trigger` 写出非占位 trigger_rate_cap / skill_trigger_rate。无触发关系时
        // 返回空、面板保持 0（向后兼容）。
        session.add_modifiers(trigger_modifiers(
            build, data, options, skill, group, skill_id,
        ));
    }

    // 1b-ii. 技能伤害倍率 → `AddedDamage` MORE，使**附加 flat 伤害**（武器+装备 added）
    //        同武器击中一并按 baseMultiplier 放大（武器击中已在 base_input × dmg_mult）。
    if (dmg_mult - 1.0).abs() > f64::EPSILON {
        let origin = ModifierSource::new(SourceId::new(SourceKind::SkillGem, "skill.damageMult"))
            .with_raw_text(format!("skill damage multiplier {dmg_mult:.2}"));
        session.add_modifiers(vec![
            Modifier::number("AddedDamage", ModType::More, (dmg_mult - 1.0) * 100.0)
                .with_origin(origin),
        ]);
    }

    // 1c. 武器基底暴击率 → Weapon1 归因的 BASE SkillBaseCritChance（**仅攻击技能**；
    //     底材桶，区别于词条桶——见 skill_base_modifiers 同名注释）。法术技能用自身
    //     基础暴击（skill_base_modifiers 注入），不吃武器暴击——故主技能自带 crit_chance 时跳过。
    let main_skill_has_own_crit = main_skill
        .as_ref()
        .map(|(s, _, _)| s.crit_chance.is_some_and(|c| c > 0.0))
        .unwrap_or(false);
    if let Some(w) = weapon
        && w.crit_chance > 0.0
        && !main_skill_has_own_crit
    {
        let origin = ModifierSource::new(SourceId::new(SourceKind::Item, "weapon1.base"))
            .with_raw_text(format!("weapon base crit {}%", w.crit_chance));
        session.add_modifiers(vec![
            Modifier::number("SkillBaseCritChance", ModType::Base, w.crit_chance)
                .with_origin(origin),
        ]);
    }
}

/// 1 阶段：角色基础（等级 + 职业派生属性 → BASE）+ 元素抗性惩罚（战役进度档位）。
pub(super) fn inject_character_base(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    resolved_config: &crate::config_resolve::ResolvedConfig,
) {
    // 1. 角色基础（等级 + 职业派生属性）→ CharacterBase 归因的 BASE modifier。
    if options.inject_character_base
        && let Some(base) = character_base(build, data)
    {
        //  派生系数自注入的 character_constants 域读取（与 Default 逐值相等）。
        session.add_modifiers(base.modifiers(&data.constants.character_constants));
        // 元素抗性惩罚（火/冰/电；混沌无惩罚）：XML Config `resistancePenalty` 显式档位
        // 优先；省略时按 PoB2 CalcSetup.lua `configInput.resistancePenalty or -60`（即
        // Endgame）。档位 → 惩罚 modifier 走 [`CampaignProgress`] 既有表（带
        // `campaign.resistance_penalty` 归因；Act1 惩罚为 0、不产生 modifier）。
        let progress = resolved_config
            .config
            .campaign_progress
            .unwrap_or(CampaignProgress::Endgame);
        session.add_modifiers(progress.modifiers());
    }
}

/// 2 阶段：装备归因路径注入——逐件 filter / Kalandra 镜射 / 局部词条（武器·防御·Spirit）
/// 剔除 / add_item / 槽位加成效果数值副本。`off_weapon_active` = 副手武器源是否被消费；
/// `main_weapon_active` = 主技能以 Weapon1 为伤害源（持武攻击）。
pub(super) fn inject_items(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    off_weapon_active: bool,
    main_weapon_active: bool,
) -> Result<(), BuildError> {
    // 槽位加成效果（『N% increased bonuses gained from Equipped Rings and Amulets』，
    // Ritualist 等）：对应槽位物品词条按 scale 追加缩放副本（PoB2 CalcPerform.lua:
    // 1326-1370 `EffectOfBonusesFrom<Slot>` ScaleAddMod 语义；仅 scale>0 生效）。
    let bonus_scales = slot_bonus_effect_scales(build, data);
    for (slot, item) in build.equipped_items() {
        // Kalandra's Touch『Reflects opposite Ring』：镜射对侧戒指的全部词条
        // （vendor CalcSetup.lua:1221-1243），来源仍归 Kalandra 所在槽。
        let item = kalandra_reflected_ring(build, slot, item).unwrap_or(item);
        let mut filtered = filter_item_parseable(item, engine_ctx(data));
        // 主手武器：剔除局部物理增伤/附加（已作为武器 source 独立乘区 × baseMultiplier 计入
        // weapon_contribution）；留在全局会重复且错误地并入加法桶（PoB 是独立乘区）。
        // 双持副手：Weapon2 作为 off-hand 武器源消费时同样剔除——其局部词条
        // 已折入 off-hand WeaponContribution（未消费时维持现状，不动全局注入）。
        if slot == EquipmentSlot::Weapon1 || (slot == EquipmentSlot::Weapon2 && off_weapon_active) {
            let drop_local = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| !is_weapon_local_mod(t, &data.local_mods.weapon))
                    .collect()
            };
            filtered.implicit_texts = drop_local(filtered.implicit_texts);
            filtered.modifier_texts = drop_local(filtered.modifier_texts);
            filtered.enchant_texts = drop_local(filtered.enchant_texts);
        }
        // 非伤害源武器的裸「Adds N to M <type> Damage」剔除（#10-3，titan/smith
        // 高估根因）：vendor Item.lua:1923-1928 把武器上全类型裸 adds 折入
        // weaponData（局部，只随该武器攻击生效）。主技能不以该武器为伤害源
        // （非武器攻击如 Shield Wall / 法术 / 未消费副手）时，这些词条不得进
        // 全局加法桶（titan Nebuloch『Adds 30 to 52 Chaos damage』经 added
        // effectiveness 放大 → TotalDPS 1.05x）。该武器**是**伤害源时维持现状：
        // 裸元素/混沌 adds 走全局注入近似（与 vendor weaponData 折算数值等价，
        // deadeye/twister 1.00x 钉住）。
        let weapon_source_inactive = (slot == EquipmentSlot::Weapon1 && !main_weapon_active)
            || (slot == EquipmentSlot::Weapon2 && !off_weapon_active);
        if weapon_source_inactive && data.weapon_base(&item.base.to_string()).is_some() {
            const TYPED_ADDS_SUFFIXES: [&str; 5] = [
                "physical damage",
                "fire damage",
                "cold damage",
                "lightning damage",
                "chaos damage",
            ];
            let drop_typed_adds = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| {
                        let clean = clean_item_text(t);
                        !TYPED_ADDS_SUFFIXES
                            .iter()
                            .any(|s| parse_adds_with_suffix(&clean, s).is_some())
                    })
                    .collect()
            };
            filtered.implicit_texts = drop_typed_adds(filtered.implicit_texts);
            filtered.modifier_texts = drop_typed_adds(filtered.modifier_texts);
            filtered.enchant_texts = drop_typed_adds(filtered.enchant_texts);
        }
        // 护甲件：剔除局部「increased / +flat Armour/Evasion/ES」（已折入 rolled 件级底值 /
        // 基底兜底乘区，见 defence_base_modifiers）；留在全局会重复（且错误地变成全局加法）。
        // 判定护甲件：有基底护甲项 **或** 文本给出 rolled 防御行（兜底覆盖无 catalog 的 unique）。
        let rd = &item.rolled_defence;
        // per-level 防御件（如纯 implicit 唯一手套）也算护甲件——其 `Has +N per level` 已折入
        // 件级底值（item_rolled_defence），须从全局路径剔除，避免重复/错误全局注入。
        let has_per_level_def = item_per_level_defence(item).iter().any(|&v| v > 0.0);
        let is_armour_piece = data.armour_base(&item.base.to_string()).is_some()
            || rd.armour.is_some()
            || rd.evasion.is_some()
            || rd.energy_shield.is_some()
            || has_per_level_def;
        if is_armour_piece {
            let drop_def = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| {
                        let c = clean_item_text(t);
                        parse_local_defence_inc(&c).is_none()
                            && parse_local_defence_flat(&c).is_none()
                            && parse_has_per_level_defence(&c).is_none()
                    })
                    .collect()
            };
            filtered.implicit_texts = drop_def(filtered.implicit_texts);
            filtered.modifier_texts = drop_def(filtered.modifier_texts);
            filtered.enchant_texts = drop_def(filtered.enchant_texts);
        }
        // 带 Spirit 基底的件（权杖）：剔除局部 `increased Spirit` / `+N to Spirit`
        // ——已折入 rolled `Spirit:` 行（Item.lua:1724-1727 calcLocal 折算）或由
        // item_spirit_modifiers 按基底重算；留在全局会双计（13-G11）。
        let has_spirit_base = item.rolled_defence.spirit.is_some()
            || data
                .base_items
                .get(&item.base.to_string())
                .and_then(|b| b.spirit)
                .is_some();
        if has_spirit_base {
            let drop_spirit = |texts: Vec<String>| -> Vec<String> {
                texts
                    .into_iter()
                    .filter(|t| !is_local_spirit_mod(&clean_item_text(t)))
                    .collect()
            };
            filtered.implicit_texts = drop_spirit(filtered.implicit_texts);
            filtered.modifier_texts = drop_spirit(filtered.modifier_texts);
            filtered.enchant_texts = drop_spirit(filtered.enchant_texts);
        }
        // 武器件走 add_weapon_item：无 flag 爆伤词条转按手条件
        // （vendor Item.lua:1954-1961，0.22.0 把 CritMultiplier 加进转换清单；
        // 仅武器基底转换——Weapon2 的盾/箭袋/法器等非武器件不转）。
        let is_weapon_item = matches!(slot, EquipmentSlot::Weapon1 | EquipmentSlot::Weapon2)
            && data.weapon_base(&item.base.to_string()).is_some();
        if is_weapon_item {
            session
                .add_weapon_item(slot, &filtered)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        } else {
            session
                .add_item(slot, &filtered)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }

        // 槽位加成效果副本：该槽位有 `EffectOfBonusesFrom<Slot>` INC 时，把本件已
        // 注入词条的**数值差额副本** 追加注入（vendor CalcPerform.lua:1347-1369
        // 把 BASE/INC 数值 mod 分组后 `ScaleAddMod(mod, slotEffectMod)`——数值
        // 缩放为截尾语义 [`vendor_scale_mod_value`]，差额 = trunc(round(v×(1+s),2))−v；
        // flag 副本为无操作，跳过）。Kalandra 镜射已在上方顶替 `filtered`，与 vendor
        // :1328-1334 的对侧取词条一致。负向 scale（focus -50%，CalcSetup.lua:
        // 1209-1220）同路径：全值 + 负副本 = 净 ×(1+scale)，与 vendor
        // combinedList+ScaleAddList 合并等价（vendor 对缩放副本取
        // `m_modf(round(v*scale,2))` 截断，此处保留浮点，逐件 ≤0.5 偏差）。
        if let Some(&(_, scale)) = bonus_scales
            .iter()
            .find(|(s, scale)| *s == slot && *scale != 0.0)
        {
            let ingest = pobr_core::ingest_item_with_ctx(slot, &filtered, engine_ctx(data))
                .map_err(|e| BuildError::Parse(e.to_string()))?;
            let scaled: Vec<Modifier> = ingest
                .modifiers
                .into_iter()
                .filter_map(|m| match m.value {
                    pobr_core::ModValue::Number(v) => {
                        let delta = vendor_scale_mod_value(v, 1.0 + scale) - v;
                        (delta != 0.0).then_some(Modifier {
                            value: pobr_core::ModValue::Number(delta),
                            ..m
                        })
                    }
                    _ => None,
                })
                .collect();
            session.add_modifiers(scaled);
        }
    }
    Ok(())
}

/// 4c/4c'/4d 阶段：Mark 自身进攻 buff（gain-as-extra）+ 非主组曝光 support + Spirit 预留聚合。
pub(super) fn inject_self_buff_exposure_spirit(
    session: &mut CalculationSession,
    build: &Build,
    data: &BuildData,
    main_skill_group: Option<&SocketGroup>,
) {
    // 4c. Mark 激活授予玩家的**进攻自身 buff**（gain-as-extra）→ SkillGem 归因 modifier。
    //     数据驱动：已启用宝石的 stat 含 `*_damage_buff_damage_%_to_gain_as_<type>`（Freezing
    //     Mark→Cold、Voltaic Mark→Lightning），映射 `DamageGainAs<Type>` BASE，注入 gain 矩阵。
    session.add_modifiers(self_buff_offensive_modifiers(build, data));
    // 4c'.非主组曝光效果 support：曝光源所在副组的兼容 support 的
    //     `<El>ExposureEffect` INC 全局注入。主组 support 已由 support_modifiers
    //     全量注入，函数内跳过防双注入。
    session.add_modifiers(exposure_support_modifiers(build, data, main_skill_group));
    // 4d.持续保留型效果的 Spirit 预留聚合 → `SkillSpiritReservationBase` BASE，
    //     perform fill 落 OutputTable::spirit_reserved（超载只报告不拦截）。
    //     db 传只读视图取树/装备的 ReservationEfficiency 词条（此时点树/装备已
    //     ingest）；先算后注避免同语句可变/不可变借用冲突。
    let spirit_mods = spirit_reservation_modifiers(build, data, session.mod_db());
    session.add_modifiers(spirit_mods);
}
