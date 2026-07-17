//! 击中伤害的分类型分量（DamageComponent）+ 伤害转换链 + gain-as-extra。
//!
//! 把单一物理桶扩展为按伤害类型（physical / fire / cold / lightning / chaos）拆分的
//! 分量向量：每个分量独立做 `base × (1 + Σinc/100) × Πmore` 聚合，求和为总（非暴击）击中伤害。
//! 这是后续伤害转换 / 分类型击中伤害 / 异常状态的基础。
//!
//! ## 伤害转换链（PoB2 `CalcOffence.lua` 转换段，逐字核对）
//!
//! - 固定顺序 `Physical → Lightning → Cold → Fire → Chaos`（`dmgTypeList` / [`DAMAGE_TYPES`]）。
//! - 两阶段：先**技能转换**（`Skill<From>DamageConvertTo<To>` / `SkillDamageConvertTo<To>`），
//!   再**全局转换**（`<From>DamageConvertTo<To>` / `DamageConvertTo<To>` /
//!   `ElementalDamageConvertTo<To>` / `NonChaosDamageConvertTo<To>`）。
//! - 同源转换总和 `> 100%` 等比归一化到 100%（`factor = 100 / total`）。
//! - **inc/more 仅按最终类型**（PoE2，无转换源 double-dip）：转换/gain 分量只吃**自身最终
//!   伤害类型**的 increased/more（+ Elemental 共享组），**不**累积转换源类型。一手依据：PoB2
//!   `calcDamage(...,damageType,0)`（CalcOffence.lua:3990，typeFlags 仅含最终类型）+ headless
//!   oracle 验证。PoE1 的"转换源 double-dip"在 PoE2 已移除。见 damage-scaling.md §转换分量口径。
//! - **gain-as-extra**（`<From>DamageGainAs<To>` / `DamageGainAs<To>` 等 BASE%）：
//!   额外伤害包，**不从来源扣减、不参与归一**；inc/more 同样只按目标（最终）类型。
//!
//! ## 向后兼容
//!
//! 无任何转换 / gain-as-extra modifier 时，[`calculate_components`] 走与历史完全一致的
//! 「分类型 base × scale」快速路径（[`scale_components_no_conversion`]），输出逐字不变。

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::round;

/// 单个伤害分量：聚合后的 min/max，含击中/持续区分、来源桶、转换沿途类型集合。
///
/// **`type_path`**（08-mechanics §2.3、damage-scaling.md §转换分量口径）：
/// 该分量在转换链上经过的所有伤害类型（去重、按 [`DAMAGE_TYPES`] 顺序）。
/// 例如物理 50% 转火焰后的火焰分量 `type_path = [Physical, Fire]`；**仅用于归因/展示**，
/// inc/more 聚合只按最终类型 `FireDamage`（+`ElementalDamage`），不含转换源 `PhysicalDamage`。
/// 未转换分量的 `type_path` 只含自身类型，与历史单类型聚合等价。
#[derive(Debug, Clone, PartialEq)]
pub struct DamageComponent {
    pub damage_type: DamageType,
    pub min: f64,
    pub max: f64,
    /// 击中 vs 持续（DoT 不暴击、不掷骰）。默认 [`DamageKind::Hit`]。
    pub kind: DamageKind,
    /// 来源桶（Attack / Spell / Secondary / …）。默认 [`DamageSource::Attack`]。
    pub source: DamageSource,
    /// 转换链上经过的伤害类型集合（去重、有序）；仅用于归因/展示，inc/more 只按最终类型。
    pub type_path: Vec<DamageType>,
}

impl DamageComponent {
    /// 向后兼容构造：Hit / Attack，`type_path` 仅含自身类型。
    pub fn new(damage_type: DamageType, min: f64, max: f64) -> Self {
        Self {
            damage_type,
            min,
            max,
            kind: DamageKind::Hit,
            source: DamageSource::Attack,
            type_path: vec![damage_type],
        }
    }

    /// 富化构造：显式指定 kind / source（`type_path` 仍仅含自身类型）。
    pub fn with_kind_source(
        damage_type: DamageType,
        min: f64,
        max: f64,
        kind: DamageKind,
        source: DamageSource,
    ) -> Self {
        Self {
            damage_type,
            min,
            max,
            kind,
            source,
            type_path: vec![damage_type],
        }
    }

    /// 覆盖 `type_path`（转换链产物用）。自动确保目标类型在路径内、去重、按链序排序。
    pub fn with_type_path(mut self, path: impl IntoIterator<Item = DamageType>) -> Self {
        let mut set: Vec<DamageType> = Vec::new();
        for ty in path {
            if !set.contains(&ty) {
                set.push(ty);
            }
        }
        if !set.contains(&self.damage_type) {
            set.push(self.damage_type);
        }
        set.sort_by_key(|ty| type_order_index(*ty));
        self.type_path = set;
        self
    }

    /// 该分量的平均击中伤害 `(min + max) / 2`。
    pub fn avg(&self) -> f64 {
        (self.min + self.max) / 2.0
    }

    /// 带 lucky 掷骰的平均击中伤害（M4-T3 W-C2，PoB2 `CalcOffence.lua:4043-4046`）。
    ///
    /// `avg = (min/2 + max/2) × (1 − p) + (min/3 + 2×max/3) × p`，其中 `p` 为 lucky
    /// 几率（分数 0..=1，越界 clamp）。lucky = 掷两次取高，均值偏向 max。
    /// `p = 0` 时与 [`avg`](Self::avg) 逐位一致（旧路径保留）。
    /// 几率求取见 [`lucky_hit_chance`]（按 pass × damage_type 在 crit_pass 内消费）。
    pub fn avg_with_lucky(&self, lucky_chance: f64) -> f64 {
        let p = lucky_chance.clamp(0.0, 1.0);
        let not_lucky = self.min / 2.0 + self.max / 2.0;
        let lucky = self.min / 3.0 + 2.0 * self.max / 3.0;
        not_lucky * (1.0 - p) + lucky * p
    }
}

/// 单分量的 lucky 几率（分数 0..=1；M4-T3 W-C2，PoB2 `CalcOffence.lua:4036-4042`）。
///
/// 全额 lucky（=1）的旗标来源（任一命中即 1）：
/// - `LuckyHits`：恒 lucky；
/// - `CritLucky`：仅暴击 pass（vendor `pass == 1`）——注意与 `crit.rs` 的
///   `CritChanceLucky`（暴击**几率**掷骰）是两个机制，不要合并；
/// - `LightningNoCritLucky`：仅非暴击 pass 且 Lightning 分量（vendor `pass == 2`）；
/// - `ElementalLuckHits`：三元素分量（Lightning/Cold/Fire）。
///
/// 否则 `p = min(Sum(BASE, "<Type>LuckyHitsChance", "LuckyHitsChance"), 100) / 100`。
pub fn lucky_hit_chance(
    db: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    is_crit_pass: bool,
) -> f64 {
    if db.flag(cfg, ModName::from("LuckyHits"))
        || (!is_crit_pass
            && damage_type == DamageType::Lightning
            && db.flag(cfg, ModName::from("LightningNoCritLucky")))
        || (is_crit_pass && db.flag(cfg, ModName::from("CritLucky")))
        || (damage_type.is_elemental() && db.flag(cfg, ModName::from("ElementalLuckHits")))
    {
        return 1.0;
    }
    let prefix = type_prefix(damage_type);
    let names = [
        ModName::from(format!("{prefix}LuckyHitsChance")),
        ModName::from("LuckyHitsChance"),
    ];
    db.sum(ModType::Base, cfg, &names).min(100.0) / 100.0
}

/// canDeal / `DealNo<Type>` 门控（M4-T3 W-C3，蓝图 §3.3 契约 3 冻结签名；
/// PoB2 `CalcOffence.lua:2226-2230`，消费点 `:3989/:4793/:5451`——hit / ailment / DoT 共用）。
///
/// `canDeal[type] = not Flag("DealNo"..type, "DealNoDamage")`；不能造成的类型分量
/// **就地清零**（min/max → 0，分量保留——下游求和/分桶语义等价 vendor 的跳过）。
///
/// **顺序关键**（蓝图 §2 W-C3）：转换先发生，清零的是**转换后残留**——本函数必须在
/// 转换链（[`calculate_components`] / [`convert_damage`]）之后调用。如 Avatar of Fire
/// 形态：物理转火后残留物理被 `DealNoPhysical` 清零、已转出的火焰保留。
///
/// 签名说明：蓝图契约 3 写作 `&mut Vec<DamageComponent>`；本实现取更通用的
/// `&mut [DamageComponent]`（clippy `ptr_arg`），调用方传 `&mut vec` 经 deref 直接兼容、
/// 调用形状不变。
pub fn apply_can_deal(components: &mut [DamageComponent], db: &ModDb, cfg: &CalcConfig) {
    let deal_no_damage = db.flag(cfg, ModName::from("DealNoDamage"));
    for component in components.iter_mut() {
        let gated = deal_no_damage
            || db.flag(
                cfg,
                ModName::from(format!("DealNo{}", type_prefix(component.damage_type))),
            );
        if gated {
            component.min = 0.0;
            component.max = 0.0;
        }
    }
}

/// 计算顺序固定的全部伤害类型，保证分量向量确定性排序。
///
/// **Bug#7 修正（damage-conversion-chain-order-wrong）**：
/// 须与 PoB2 转换链顺序一致：`Physical → Lightning → Cold → Fire → Chaos`
/// （PoB2 `CalcOffence.lua` `dmgTypeList`；damage-scaling.md §转换顺序与链式）。
pub const DAMAGE_TYPES: [DamageType; 5] = [
    DamageType::Physical,
    DamageType::Lightning,
    DamageType::Cold,
    DamageType::Fire,
    DamageType::Chaos,
];

/// `DamageType` 在 [`DAMAGE_TYPES`] 中的下标（用于 `type_path` 链序排序）。
fn type_order_index(damage_type: DamageType) -> usize {
    DAMAGE_TYPES
        .iter()
        .position(|t| *t == damage_type)
        .unwrap_or(usize::MAX)
}

/// 把 `DamageType` 映射到稳定的 modifier 名称前缀（与 PoB 命名一致）。
fn type_prefix(damage_type: DamageType) -> &'static str {
    match damage_type {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}

/// 单个伤害类型的非暴击击中分量基础值（flat），不含 inc/more / 转换。
///
/// - 物理：基础来自武器击中 `base_hit_min/max`（技能/武器自带，不受 AddedDamage MORE 影响），
///   再加 `PhysicalDamageMin/Max` Base 附加（受 AddedDamage MORE 效率影响）。
/// - 其余类型：来自 `<Type>DamageMin/Max` Base 附加（flat added damage，受 AddedDamage MORE 效率影响）。
///
/// **Bug#8 修正（added-damage-effectiveness-missing）**：
/// 附加伤害效率（`AddedDamage` 效率乘子）只乘外部 flat added，不乘技能/武器自带 base。
/// 出处：damage-scaling.md §Added Damage Effectiveness；
///       PoB2 CalcOffence.lua:3909 `addedMult = calcLib.mod(skillModList, cfg,
///       "Added"..damageType.."Damage", "AddedDamage")`
///       仅乘 `addedMin * addedMult`，不乘 `source[...]`（武器/技能自带伤害）。
///
/// **M4-m 补全（addedMult INC 腿）**：vendor `calcLib.mod`（CalcTools.lua:16-18）=
/// `(1 + Sum(INC, cfg, names...)/100) × More(cfg, names...)`——INC 与 MORE 两腿
/// 同名集（`Added<Type>Damage` + `AddedDamage`）。旧实现仅有 MORE 腿；现按 vendor
/// 名序单次多名查询补 INC 腿（多名 `more` 的逐名取整语义与 vendor 单次调用一致）。
fn base_flat(
    db: &ModDb,
    cfg: &CalcConfig,
    damage_type: DamageType,
    base_hit_min: f64,
    base_hit_max: f64,
) -> (f64, f64) {
    let prefix = type_prefix(damage_type);
    let min_name = ModName::from(format!("{prefix}DamageMin"));
    let max_name = ModName::from(format!("{prefix}DamageMax"));
    let added_min = db.sum(ModType::Base, cfg, &[min_name]);
    let added_max = db.sum(ModType::Base, cfg, &[max_name]);

    // 附加伤害效率（addedMult = (1 + ΣINC/100) × ΠMORE）：仅作用于外部 flat added，
    // 不乘技能/武器自带 base。名集 = `Added<Type>Damage` + `AddedDamage`（vendor 名序）。
    let eff_names = [
        ModName::from(format!("Added{prefix}Damage")),
        ModName::from("AddedDamage"),
    ];
    let eff = (1.0 + db.sum(ModType::Inc, cfg, &eff_names) / 100.0) * db.more(cfg, &eff_names);

    match damage_type {
        DamageType::Physical => {
            // 武器/技能自带 base 不受效率影响；flat added 受效率影响
            (
                base_hit_min + added_min * eff,
                base_hit_max + added_max * eff,
            )
        }
        _ => (added_min * eff, added_max * eff),
    }
}

/// 计算全部伤害类型的击中分量向量（含转换链 + gain-as-extra；inc/more 仅按最终类型）。
///
/// 管线：
/// 1. 按 [`DAMAGE_TYPES`] 顺序求各类型 flat base（[`base_flat`]，含 added effectiveness）。
/// 2. 读 [`ConversionRules`]（技能 + 全局 convert + gain-as-extra）。**全为空时**走
///    [`scale_components_no_conversion`] 快速路径，输出与历史逐字一致。
/// 3. 否则跑 [`apply_conversion_chain`]：产出带 `type_path` 的「转换后基础分量」，
///    再各自按**最终伤害类型**聚合 inc/more（PoE2 无转换源 double-dip）。
///
/// **Bug#6 修正（missing-elemental-damage-modname-group）**：火/冰/电分量的 inc/more
/// 必须包含 `ElementalDamage` 共享组（在 [`aggregate_inc_more`] 内统一展开）。
///
/// 仅当分量 base（min 或 max）非零时纳入向量；物理分量始终纳入（武器击中基线），
/// 以保证纯物理路径与旧实现完全一致。
pub(crate) fn calculate_components(
    db: &ModDb,
    cfg: &CalcConfig,
    base_hit_min: f64,
    base_hit_max: f64,
) -> Vec<DamageComponent> {
    // 第一步：各类型 flat base。
    let mut base: Vec<(DamageType, f64, f64)> = Vec::with_capacity(DAMAGE_TYPES.len());
    for &damage_type in DAMAGE_TYPES.iter() {
        let type_cfg = cfg.clone().with_damage_type(damage_type);
        let (min, max) = base_flat(db, &type_cfg, damage_type, base_hit_min, base_hit_max);
        base.push((damage_type, min, max));
    }

    // 第二步：转换规则。无任何转换/gain → 快速路径（向后兼容、逐字一致）。
    let rules = ConversionRules::from_mod_db(db, cfg);
    if rules.is_empty() {
        return scale_components_no_conversion(db, cfg, &base);
    }

    // 第三步：跑转换链，产出带 type_path 的「转换后基础分量」。
    let converted = apply_conversion_chain(&base, &rules);
    if std::env::var("POBR_DBG_BASES").is_ok() {
        for (t, mn, mx) in &base {
            eprintln!("[POBR_BASE] {t:?} {mn:.2}/{mx:.2}");
        }
        for c in &converted {
            eprintln!(
                "[POBR_SUMMED] {:?} {:.2}/{:.2}",
                c.damage_type, c.min, c.max
            );
        }
    }

    // 第四步：各自按最终伤害类型聚合 inc/more（PoE2 无转换源 double-dip）。
    converted
        .into_iter()
        .filter(|comp| {
            comp.damage_type == DamageType::Physical || comp.min != 0.0 || comp.max != 0.0
        })
        .map(|comp| scale_with_path(db, cfg, comp))
        .collect()
}

/// 无转换快速路径：逐类型 `base × (1 + Σinc/100) × Πmore`，等价于历史实现。
///
/// 与转换路径分离，保证纯物理 / 仅附加分类型 flat 的输出**逐字不变**（回归安全）。
fn scale_components_no_conversion(
    db: &ModDb,
    cfg: &CalcConfig,
    base: &[(DamageType, f64, f64)],
) -> Vec<DamageComponent> {
    base.iter()
        .filter_map(|&(damage_type, base_min, base_max)| {
            let is_physical = damage_type == DamageType::Physical;
            if !is_physical && base_min == 0.0 && base_max == 0.0 {
                return None;
            }
            let comp = DamageComponent::new(damage_type, base_min, base_max);
            Some(scale_with_path(db, cfg, comp))
        })
        .collect()
}

/// 按分量 `type_path` 展开所有相关 ModName，聚合 inc/more 并缩放 min/max。
///
/// 一个 `type_path = [Physical, Fire]` 的分量会同时吃 `PhysicalDamage` 与 `FireDamage`
/// （+ 元素 `ElementalDamage`）以及通用 `AttackDamage`/`Damage` 的 inc/more —— double-dip。
/// `type_path` 仅含单类型时退化为历史逐类型聚合。
fn scale_with_path(db: &ModDb, cfg: &CalcConfig, comp: DamageComponent) -> DamageComponent {
    let (inc, more) = aggregate_inc_more(db, cfg, comp.damage_type);
    let scale = (1.0 + inc / 100.0) * more;
    if std::env::var("POBR_DBG_BASES").is_ok() {
        eprintln!(
            "[POBR_POOL] {:?} inc={inc:.2} more={more:.4} scale={scale:.4}",
            comp.damage_type
        );
    }
    // PoB2 `calcDamage`（CalcOffence.lua:138-139,153-154）：min 与 max 各额外乘一个独立 MORE
    // 乘区 `Min<Type>Damage` / `Max<Type>Damage`（如「more maximum Lightning Damage」「less
    // minimum Physical Attack Damage」），仅缩放区间一端。这些 ModName 类型编码在名字里、不带
    // DamageType tag，按名用 cfg（携带技能 flag）命中；无此类词条时 more 返回 1.0、输出不变。
    let prefix = type_prefix(comp.damage_type);
    let more_min = db.more(cfg, &[ModName::from(format!("Min{prefix}Damage"))]);
    let more_max = db.more(cfg, &[ModName::from(format!("Max{prefix}Damage"))]);
    DamageComponent {
        min: round(comp.min * scale * more_min),
        max: round(comp.max * scale * more_max),
        ..comp
    }
}

/// 按一组类型路径聚合 inc（求和）与 more（连乘）。
///
/// 每个路径类型贡献 `<Type>Damage`（type-scoped cfg 命中其 `DamageType` tag）；
/// 元素类型额外贡献共享 `ElementalDamage`（去重，多个元素只算一次）。通用
/// `AttackDamage`/`Damage` 只算一次（与 type-scoped 无关）。
pub(crate) fn aggregate_inc_more(
    db: &ModDb,
    cfg: &CalcConfig,
    final_type: DamageType,
) -> (f64, f64) {
    // 通用桶（不限伤害类型）：`Damage` 始终；攻击/法术/技能类别（投射物/范围/近战）按
    // cfg flag——使 `increased <Attack|Spell|Projectile|Area|Melee> Damage` 对该技能生效。
    let mut generic_names = vec![ModName::from("Damage")];
    for (flag, name) in [
        (ModFlags::ATTACK, "AttackDamage"),
        (ModFlags::SPELL, "SpellDamage"),
        (ModFlags::PROJECTILE, "ProjectileDamage"),
        (ModFlags::AREA, "AreaDamage"),
        (ModFlags::MELEE, "MeleeDamage"),
    ] {
        if cfg.flags.intersects(flag) {
            generic_names.push(ModName::from(name));
        }
    }
    // 关键词 / 武器类别派生的伤害缩放名（GrenadeDamage / CrossbowDamage 等）。
    for kw in &cfg.damage_keywords {
        generic_names.push(ModName::from(kw.clone()));
    }
    let elemental_name = ModName::from("ElementalDamage");

    // 通用桶只算一次（不限定伤害类型）。
    let mut inc = db.sum(ModType::Inc, cfg, &generic_names);
    let mut more = db.more(cfg, &generic_names);
    if std::env::var("POBR_DBG_BASES").is_ok() && final_type == DamageType::Physical {
        eprintln!("[POBR_POOL_NAMES] {generic_names:?}");
        for c in db.contributions(ModType::Inc, cfg, &generic_names) {
            eprintln!(
                "[POBR_POOL_SRC] {:?} {} src={:?}",
                c.name, c.value, c.raw_text
            );
        }
        for c in db.contributions(ModType::More, cfg, &generic_names) {
            eprintln!(
                "[POBR_POOL_MORE] {:?} {} src={:?}",
                c.name, c.value, c.raw_text
            );
        }
    }

    // PoB2-PoE2：类型化 inc/more 只按分量**最终伤害类型**聚合，**不**叠加转换源类型
    // （PoE1 的「按转换源 increased 双重 dip」在 PoE2 已移除——PoB2 headless oracle 逐分量一手
    // 验证：`calcDamage` 的 typeFlags 仅含最终 damageType）。转换/gain-as-extra 产物同此口径。
    // 必须用分量的最终 `damage_type`（而非 type_path 末位）——type_path 已按链序排序，末位
    // 可能并非分量自身类型（如 Cold 分量 path=[Physical,Lightning,Cold,Fire] 末位是 Fire）。
    let type_cfg = cfg.clone().with_damage_type(final_type);
    let type_name = [ModName::from(format!("{}Damage", type_prefix(final_type)))];
    inc += db.sum(ModType::Inc, &type_cfg, &type_name);
    more *= db.more(&type_cfg, &type_name);

    if final_type.is_elemental() {
        let elem = [elemental_name];
        inc += db.sum(ModType::Inc, &type_cfg, &elem);
        more *= db.more(&type_cfg, &elem);
    }
    (inc, more)
}

/// 一组伤害转换 / gain-as-extra 规则（从 [`ModDb`] 读取，对齐 PoB2 `processDamageConversion`
/// / `buildGainTable`）。
///
/// - `convert[from]`：归一化后的 `to → fraction`（同源总和 ≤ 1）。
/// - `gain[from]`：gain-as-extra 的 `to → fraction`（不归一、不扣源）。
#[derive(Debug, Clone)]
pub struct ConversionRules {
    /// `convert[from][to] = fraction`（已按源归一到 ≤ 1）。
    pub convert: [[f64; 5]; 5],
    /// `convert_path[from][to]` = 该转换流经的中间类型集合（不含 from / to 本身）。
    /// 用于把链式转换的全部沿途类型累入 `type_path`（double-dip）。
    pub convert_path: Vec<Vec<Vec<DamageType>>>,
    /// `gain[from][to] = fraction`（gain-as-extra，不参与归一）。
    pub gain: [[f64; 5]; 5],
}

impl Default for ConversionRules {
    fn default() -> Self {
        let n = DAMAGE_TYPES.len();
        Self {
            convert: [[0.0; 5]; 5],
            convert_path: vec![vec![Vec::new(); n]; n],
            gain: [[0.0; 5]; 5],
        }
    }
}

impl ConversionRules {
    /// 从 [`ModDb`] 读取技能 + 全局转换 + gain-as-extra 规则。
    ///
    /// 转换两阶段（PoB2 `processDamageConversion`，skill 先于 global，各自源内归一）：
    /// 技能转换扣减源后，全局转换作用于「未转换剩余」**与**「技能已转出部分」。
    /// 为保持纯函数 + 确定性，这里把两阶段折叠成单一 `convert[from][to]` 矩阵
    /// （等价于 PoB2 `conversionTable` 折叠结果），并记录链式沿途类型 `convert_path`。
    pub fn from_mod_db(db: &ModDb, cfg: &CalcConfig) -> Self {
        let mut rules = Self::default();
        // 先各类型独立算技能/全局转换比例（源内归一）。
        let skill = build_conversion_matrix(db, cfg, true);
        let global = build_conversion_matrix(db, cfg, false);
        let (convert, convert_path) = fold_conversion_stages(&skill, &global);
        rules.convert = convert;
        rules.convert_path = convert_path;
        rules.gain = build_gain_matrix(db, cfg);
        rules
    }

    fn is_empty(&self) -> bool {
        let any = |m: &[[f64; 5]; 5]| m.iter().flatten().any(|v| *v != 0.0);
        !any(&self.convert) && !any(&self.gain)
    }
}

/// 读单阶段（技能或全局）转换比例矩阵 `m[from][to]`，每个源类型总和 >100% 归一到 100%。
///
/// 出处：PoB2 `processDamageConversion`（`SkillDamageConvertTo<To>` /
/// `<From>DamageConvertTo<To>` / `ElementalDamageConvertTo<To>` / `NonChaosDamageConvertTo<To>`）。
fn build_conversion_matrix(db: &ModDb, cfg: &CalcConfig, skill: bool) -> [[f64; 5]; 5] {
    let mut matrix = [[0.0_f64; 5]; 5];
    for (fi, &from) in DAMAGE_TYPES.iter().enumerate() {
        let from_prefix = type_prefix(from);
        let mut total = 0.0;
        for (ti, &to) in DAMAGE_TYPES.iter().enumerate() {
            let to_prefix = type_prefix(to);
            let mut names = Vec::new();
            if skill {
                names.push(ModName::from(format!("SkillDamageConvertTo{to_prefix}")));
                names.push(ModName::from(format!(
                    "Skill{from_prefix}DamageConvertTo{to_prefix}"
                )));
            } else {
                names.push(ModName::from(format!("DamageConvertTo{to_prefix}")));
                names.push(ModName::from(format!(
                    "{from_prefix}DamageConvertTo{to_prefix}"
                )));
                if from.is_elemental() {
                    names.push(ModName::from(format!(
                        "ElementalDamageConvertTo{to_prefix}"
                    )));
                }
                if from != DamageType::Chaos {
                    names.push(ModName::from(format!("NonChaosDamageConvertTo{to_prefix}")));
                }
            }
            let pct = db.sum(ModType::Base, cfg, &names).max(0.0);
            matrix[fi][ti] = pct / 100.0;
            total += pct;
        }
        // 源内 >100% 归一（PoB2: factor = 100 / total）。
        if total > 100.0 {
            let factor = 100.0 / total;
            for cell in matrix[fi].iter_mut() {
                *cell *= factor;
            }
        }
    }
    matrix
}

/// 读 gain-as-extra 比例矩阵 `g[from][to]`（不归一、不扣源）。
///
/// 出处：PoB2 `buildGainTable`（`DamageGainAs<To>` / `<From>DamageGainAs<To>` /
/// `ElementalDamageGainAs<To>` / `NonChaosDamageGainAs<To>` / `Skill*DamageGainAs<To>`）。
fn build_gain_matrix(db: &ModDb, cfg: &CalcConfig) -> [[f64; 5]; 5] {
    let mut matrix = [[0.0_f64; 5]; 5];
    for (fi, &from) in DAMAGE_TYPES.iter().enumerate() {
        let from_prefix = type_prefix(from);
        for (ti, &to) in DAMAGE_TYPES.iter().enumerate() {
            let to_prefix = type_prefix(to);
            let mut names = vec![
                ModName::from(format!("DamageGainAs{to_prefix}")),
                ModName::from(format!("{from_prefix}DamageGainAs{to_prefix}")),
                ModName::from(format!("SkillDamageGainAs{to_prefix}")),
                ModName::from(format!("Skill{from_prefix}DamageGainAs{to_prefix}")),
            ];
            if from.is_elemental() {
                names.push(ModName::from(format!("ElementalDamageGainAs{to_prefix}")));
                names.push(ModName::from(format!(
                    "SkillElementalDamageGainAs{to_prefix}"
                )));
            }
            if from != DamageType::Chaos {
                names.push(ModName::from(format!("NonChaosDamageGainAs{to_prefix}")));
                names.push(ModName::from(format!(
                    "SkillNonChaosDamageGainAs{to_prefix}"
                )));
            }
            let pct = db.sum(ModType::Base, cfg, &names).max(0.0);
            matrix[fi][ti] = pct / 100.0;
        }
    }
    // random element 档（vendor CalcOffence.lua:1175-1200）：`DamageGainAsRandom`
    // 按 physMode 展开为三元素——PoBR 折叠 AVERAGE 档（vendor configInput.physMode
    // 缺省 "AVERAGE"）：每元素 n/3。`DamageGainAsRandom` 作用于全部源类型（vendor
    // 展开成 `DamageGainAs<Elem>`，buildGainTable 对每个 from 行求和）；
    // `PhysicalDamageGainAsRandom` 仅物理源行（展开成 `PhysicalDamageGainAs<Elem>`）。
    let generic_random = db
        .sum(ModType::Base, cfg, &[ModName::from("DamageGainAsRandom")])
        .max(0.0);
    let phys_random = db
        .sum(
            ModType::Base,
            cfg,
            &[ModName::from("PhysicalDamageGainAsRandom")],
        )
        .max(0.0);
    if generic_random > 0.0 || phys_random > 0.0 {
        for (fi, &from) in DAMAGE_TYPES.iter().enumerate() {
            let pct = generic_random
                + if from == DamageType::Physical {
                    phys_random
                } else {
                    0.0
                };
            if pct == 0.0 {
                continue;
            }
            for (ti, &to) in DAMAGE_TYPES.iter().enumerate() {
                if to.is_elemental() {
                    matrix[fi][ti] += pct / 3.0 / 100.0;
                }
            }
        }
    }
    matrix
}

/// 把技能转换与全局转换两阶段折叠成单一 `convert[from][to]` 矩阵 + 沿途类型 `convert_path`。
///
/// 对齐 PoB2 `conversionTable` 两步语义：
/// 1. 技能转换：源 `from` 留存 `skill_mult = 1 - Σskill[from][*]`；转出 `skill[from][to]`。
/// 2. 全局转换：作用于「`from` 留存部分」与「技能转出的每个中间类型」。每个被作用类型
///    保留 `1 - Σglobal[mid][*]`，其余按 `global[mid][to]` 再分配到目标。
///
/// 返回 `(convert, convert_path)`：
/// - `convert[from][to]` = 「`from` 的 base 最终落到 `to` 的总比例」（含留存到自身）。
/// - `convert_path[from][to]` = 该 from→to 流动经过的中间类型集合（不含 from / to），
///   用于 double-dip 沿途 increased 累积（如 Phys→Cold→Fire 的火焰带 Cold 标签）。
#[allow(clippy::type_complexity)]
fn fold_conversion_stages(
    skill: &[[f64; 5]; 5],
    global: &[[f64; 5]; 5],
) -> ([[f64; 5]; 5], Vec<Vec<Vec<DamageType>>>) {
    let n = DAMAGE_TYPES.len();
    let mut result = [[0.0_f64; 5]; 5];
    let mut path: Vec<Vec<Vec<DamageType>>> = vec![vec![Vec::new(); n]; n];

    for from in 0..n {
        // 技能转换后，各「中间类型」携带的来自 from 的比例。
        let mut intermediate = [0.0_f64; 5];
        let skill_total: f64 = skill[from].iter().sum();
        let skill_mult = (1.0 - skill_total).max(0.0);
        intermediate[from] += skill_mult; // 留存到自身
        for to in 0..n {
            if skill[from][to] > 0.0 {
                intermediate[to] += skill[from][to];
            }
        }

        // 全局转换作用于每个中间类型。
        for (mid, &carried) in intermediate.iter().enumerate() {
            if carried == 0.0 {
                continue;
            }
            let global_total: f64 = global[mid].iter().sum();
            let global_mult = (1.0 - global_total).max(0.0);
            // 中间类型留存部分仍落到 mid（mid 即目标，不算沿途中间类型）。
            if global_mult > 0.0 {
                result[from][mid] += carried * global_mult;
            }
            // 全局转出部分落到各目标（mid 是沿途中间类型）。
            for to in 0..n {
                if global[mid][to] > 0.0 {
                    result[from][to] += carried * global[mid][to];
                    record_mid(&mut path[from][to], mid, from, to);
                }
            }
        }
    }
    (result, path)
}

/// 把中间类型 `mid` 记入 `from→to` 的沿途集合（排除 from 与 to 自身）。
fn record_mid(slot: &mut Vec<DamageType>, mid: usize, from: usize, to: usize) {
    if mid != from && mid != to {
        push_unique(slot, DAMAGE_TYPES[mid]);
    }
}

/// 跑转换链：把各类型 flat base 按 [`ConversionRules`] 重组为带 `type_path` 的转换后分量。
///
/// 对每个目标类型 `to`：
/// - 累加来自各源 `from` 的 `base[from] * convert[from][to]`（含 from==to 的留存），
///   并把所有有贡献的 `from` 累入该分量的 `type_path`（double-dip 关键）。
/// - gain-as-extra：基于**转换后**的每个中间类型量再追加 `* gain[mid][to]`，
///   同样累入 `type_path`（来源 + 目标双重 dip），但不扣减来源。
///
/// 出处：PoB2 `calcConvertedDamage` + `calcGainedDamage`（gain 基于转换后伤害）。
fn apply_conversion_chain(
    base: &[(DamageType, f64, f64)],
    rules: &ConversionRules,
) -> Vec<DamageComponent> {
    let n = DAMAGE_TYPES.len();
    // 索引化 base。
    let base_min: Vec<f64> = base.iter().map(|b| b.1).collect();
    let base_max: Vec<f64> = base.iter().map(|b| b.2).collect();

    // 转换后各类型的 min/max，以及 type_path 来源集合。
    let mut conv_min = vec![0.0_f64; n];
    let mut conv_max = vec![0.0_f64; n];
    let mut paths: Vec<Vec<DamageType>> = vec![Vec::new(); n];

    for to in 0..n {
        for from in 0..n {
            let frac = rules.convert[from][to];
            if frac <= 0.0 {
                continue;
            }
            let add_min = base_min[from] * frac;
            let add_max = base_max[from] * frac;
            if add_min == 0.0 && add_max == 0.0 {
                continue;
            }
            conv_min[to] += add_min;
            conv_max[to] += add_max;
            push_unique(&mut paths[to], DAMAGE_TYPES[from]);
            // 链式转换沿途的中间类型也累入 type_path（double-dip）。
            for ty in &rules.convert_path[from][to] {
                push_unique(&mut paths[to], *ty);
            }
        }
    }

    // gain-as-extra：基于转换后的中间类型量再追加。
    let mut gain_min = vec![0.0_f64; n];
    let mut gain_max = vec![0.0_f64; n];
    let mut gain_paths: Vec<Vec<DamageType>> = vec![Vec::new(); n];
    for to in 0..n {
        for mid in 0..n {
            let frac = rules.gain[mid][to];
            if frac <= 0.0 {
                continue;
            }
            // gain 来源恒为转换后的中间类型量（对角线留存 + 转入量），对齐 PoB2
            // calcGainedDamage：源量 = MinBase*conversionTable[mid].mult + convertedMin。
            // convert[mid][mid] 已含留存 mult，故纯 gain 无转换时 conv_min[mid] ==
            // base_min[mid]；mid 被 100% 转走且无转入时源量为 0——**不回退原始 base**。
            // （旧 fallback 会在 fire 100%→lightning 场景凭空用 raw fire base 产 gain，
            // deadeye hit_avg +13% 高估根因之一；vendor 侧 FireSummedBase=basis×gain
            // 数值验证 basis 不含被转走的 fire。04-offence-core 04-02 处方落地。）
            let add_min = conv_min[mid] * frac;
            let add_max = conv_max[mid] * frac;
            if add_min == 0.0 && add_max == 0.0 {
                continue;
            }
            gain_min[to] += add_min;
            gain_max[to] += add_max;
            // gain 分量的 type_path 含中间类型沿途集合 + 目标类型。
            for ty in &paths[mid] {
                push_unique(&mut gain_paths[to], *ty);
            }
            push_unique(&mut gain_paths[to], DAMAGE_TYPES[mid]);
        }
    }

    // 组装最终分量。
    let mut result = Vec::with_capacity(n);
    for to in 0..n {
        let damage_type = DAMAGE_TYPES[to];
        let min = round(conv_min[to] + gain_min[to]);
        let max = round(conv_max[to] + gain_max[to]);
        // 合并转换 + gain 的 type_path。
        let mut path = paths[to].clone();
        for ty in &gain_paths[to] {
            push_unique(&mut path, *ty);
        }
        push_unique(&mut path, damage_type);
        path.sort_by_key(|ty| type_order_index(*ty));

        result.push(DamageComponent {
            damage_type,
            min,
            max,
            kind: DamageKind::Hit,
            source: DamageSource::Attack,
            type_path: path,
        });
    }
    result
}

/// 把 `ty` 加入 `path`（去重）。
fn push_unique(path: &mut Vec<DamageType>, ty: DamageType) {
    if !path.contains(&ty) {
        path.push(ty);
    }
}

/// 伤害转换 / 额外获得 / double-dip 辅助（08-mechanics §2.2、damage-defence-order §2.2）。
///
/// 这些是**纯函数**层，作用在已聚合的 [`DamageComponent`] 向量上，供测试 / 上层简单
/// 转换场景复用。完整管线见 [`calculate_components`] → [`apply_conversion_chain`]。
///
/// 把 `from` 类型的一部分**转换**为 `to` 类型（source 减少、target 增加）。
pub fn convert_damage(
    components: &[DamageComponent],
    from: DamageType,
    to: DamageType,
    fraction: f64,
) -> Vec<DamageComponent> {
    apply_shift(components, from, to, fraction, true)
}

/// 把 `from` 类型的一部分作为**额外**伤害加到 `to` 类型（source 不减少）。
pub fn gain_as_extra(
    components: &[DamageComponent],
    from: DamageType,
    to: DamageType,
    fraction: f64,
) -> Vec<DamageComponent> {
    apply_shift(components, from, to, fraction, false)
}

/// 把多个转换 fraction 之和归一化到 <= 1.0（PoB / PoE2：总转换超过 100% 等比缩放）。
pub fn normalize_conversion(fractions: &[f64]) -> Vec<f64> {
    let total: f64 = fractions.iter().filter(|f| **f > 0.0).sum();
    if total <= 1.0 {
        return fractions.to_vec();
    }
    fractions.iter().map(|f| f / total).collect()
}

/// 把伤害分量按伤害类型求和（avg）；用于 ailment magnitude double-dip 源。
pub fn sum_avg(components: &[DamageComponent]) -> f64 {
    components.iter().map(DamageComponent::avg).sum()
}

/// 取某类型分量的 (min, max)，无则 (0, 0)。
fn type_range(components: &[DamageComponent], damage_type: DamageType) -> (f64, f64) {
    components
        .iter()
        .find(|component| component.damage_type == damage_type)
        .map_or((0.0, 0.0), |component| (component.min, component.max))
}

/// 共享的转换 / 额外获得实现。`remove_from_source` 区分 convert（true）与 gain（false）。
///
/// 目标分量累积 `from` 进 `type_path`（double-dip），保留既有 kind/source。
fn apply_shift(
    components: &[DamageComponent],
    from: DamageType,
    to: DamageType,
    fraction: f64,
    remove_from_source: bool,
) -> Vec<DamageComponent> {
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction == 0.0 || from == to {
        return components.to_vec();
    }
    let (from_min, from_max) = type_range(components, from);
    let shift_min = from_min * fraction;
    let shift_max = from_max * fraction;

    let mut result: Vec<DamageComponent> = components.to_vec();
    let mut target_idx: Option<usize> = None;
    for (idx, component) in result.iter_mut().enumerate() {
        if component.damage_type == from && remove_from_source {
            component.min = round(component.min - shift_min);
            component.max = round(component.max - shift_max);
        }
        if component.damage_type == to {
            component.min = round(component.min + shift_min);
            component.max = round(component.max + shift_max);
            push_unique(&mut component.type_path, from);
            component.type_path.sort_by_key(|ty| type_order_index(*ty));
            target_idx = Some(idx);
        }
    }
    if target_idx.is_none() && (shift_min != 0.0 || shift_max != 0.0) {
        let comp =
            DamageComponent::new(to, round(shift_min), round(shift_max)).with_type_path([from, to]);
        result.push(comp);
    }
    result
}
