use std::collections::BTreeMap;
use std::sync::Arc;

use pobr_data::catalog::buffs::BuffDef;
use pobr_data::prelude::*;

use crate::item::ingest_item_with_ctx;
use crate::mod_parser::{ParseCtx, ParseError, ParseStatus};
use crate::passive::{AllocatedNode, ingest_passive_nodes_with_ctx};
use crate::rules::HandlerRegistry;
use crate::skill_source::{GemModSource, ingest_gem_with_ctx};
use crate::{CalcConfig, Modifier};

use super::{Actor, ActorBaseStats, Env, MinimalInput, MinimalOutput, OutputTable, perform};

/// buff 技能九类分发类别（M3 T0 接口契约，蓝图 m3-orchestration.md §2.4）。
///
/// 对应 PoB2 CalcPerform.lua:1831-2984 的 buff 分发九类。M3 实际实现
/// Aura/Curse/Debuff 三类的消费（T3 buff_pass），其余 kind 进框架但暂走
/// 「原值直注」兼容路径（行为与现状一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuffKind {
    Buff,
    Guard,
    Warcry,
    Aura,
    AuraDebuff,
    Debuff,
    Curse,
    CurseBuff,
    Link,
}

/// 一个 buff 技能的注入规格（M3 T0 接口契约，蓝图 §2.4；字段语义对照 PoB2 buff 表）。
///
/// pobr-build（T3）从 granted_effects 数据构造；分类规则：`skill_types` 含 Aura→Aura、
/// 含 Mark→Curse(is_mark)、granted_effect 的 buff 语义列（M1 statmap 边车）→其余类。
#[derive(Debug, Clone)]
pub struct BuffSpec {
    /// buff 名（PoB2 buff.name，`AffectedBy<名>` 条件用）。
    pub name: String,
    pub kind: BuffKind,
    /// 来源技能（归因 + curse priority socket 计算）。
    pub skill_id: String,
    /// buff 携带词条（granted_effect stat 经 statmap/映射产物）。
    pub mods: Vec<Modifier>,
    /// 默认 1.0（PoB2 calcLib.mod Magnitude 的来源值）。
    pub magnitude: f64,
    /// socket group 槽名（curse priority）。
    pub slot: Option<String>,
    /// 组内宝石序（curse priority，cap 8）。
    pub socket_index: u32,
    pub is_mark: bool,
    pub ignore_curse_limit: bool,
    /// （存量 #7-1）技能局部效果乘区 INC 增量（vendor curse 分支
    /// CalcPerform.lua:2423 `skillModList:Sum("INC", skillCfg, "CurseEffect")`
    /// 的 PoBR 对位——curse 宝石自身品质 `curse_effect_+%` + 组内兼容 support
    /// （Heightened Curse +25）payload；编排层构造，默认 0）。
    pub local_effect_inc: f64,
    /// 同上 MORE 因子（:2427 `skillModList:More(skillCfg, "CurseEffect")`，
    /// 如 Atziri's Allure -20% final；默认 1）。
    pub local_effect_more: f64,
    /// 来源效果的技能类型位（vendor per-skill `skillCfg`——buff_pass 乘区对
    /// 域限定词条（「Banner Skills have N% increased Aura Magnitudes」的
    /// SkillTypes tag）按此匹配；默认 NONE = 旧行为，域词条不命中）。
    pub skill_types: pobr_data::skill::SkillTypes,
}

#[derive(Debug, Clone)]
pub struct CalculationSession {
    env: Env,
    unsupported_modifier_texts: Vec<String>,
    /// 数据驱动 parser 引擎规则：注入后全部 ingest（item/passive/gem/flask/
    /// `add_modifier_texts`）解析走 [`parse_mod_engine`]（唯一解析器，M6 收尾
    /// 已删 legacy）。`None` = 未注入规则，所有词条文本按整行 Unsupported 处理
    /// （进入 [`unsupported_modifier_texts`] 收集面，不生效也不静默丢失）。
    ///
    /// 由编排层（pobr-build orchestrator）经 [`set_parser_rules`] 恒注入。
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    /// [`set_parser_rules`]: CalculationSession::set_parser_rules
    /// [`unsupported_modifier_texts`]: CalculationSession::unsupported_modifier_texts
    parser_rules: Option<Arc<crate::mod_parser::CompiledParserRules>>,
}

impl CalculationSession {
    pub fn new(input: MinimalInput) -> Self {
        let enemy_evasion = input.enemy_evasion;
        let mut env = Env::new(Actor::new(1, ActorBaseStats::from(input)));
        env.enemy.base.evasion = enemy_evasion;

        Self {
            env,
            unsupported_modifier_texts: Vec::new(),
            parser_rules: None,
        }
    }

    /// 注入数据驱动 parser 引擎规则（编排层注入契约面）：之后全部 ingest
    /// （[`add_item`](Self::add_item) / [`add_passive_nodes`](Self::add_passive_nodes)
    /// / [`add_gem`](Self::add_gem) / [`add_flask_charm`](Self::add_flask_charm) /
    /// [`add_modifier_texts`](Self::add_modifier_texts)）的词条解析走
    /// [`parse_mod_engine`]（special 通道已编译进
    /// [`CompiledParserRules::special`]）。须在 ingest 调用**之前**注入。
    ///
    /// orchestrator 经 pobr-gamedata 恒 load `mod_parser_rules.json` 编译
    /// `CompiledParserRules` 后调用本 setter。**不调用时**没有回退解析器：全部
    /// 词条文本按整行 Unsupported 收集（见 [`ParseCtx::parse`]），不生效也不
    /// 静默丢失。
    ///
    /// [`parse_mod_engine`]: crate::mod_parser::parse_mod_engine
    /// [`CompiledParserRules::special`]: crate::mod_parser::CompiledParserRules
    pub fn set_parser_rules(&mut self, rules: Arc<crate::mod_parser::CompiledParserRules>) {
        self.parser_rules = Some(rules);
    }

    /// 当前 ingest 用的解析上下文：注入了引擎规则 → engine 路径；未注入 →
    /// 空上下文（一切按 Unsupported 收集）。
    fn parse_ctx(&self) -> ParseCtx<'_> {
        match &self.parser_rules {
            Some(rules) => ParseCtx::with_engine(rules),
            None => ParseCtx::none(),
        }
    }

    pub fn with_config(mut self, cfg: CalcConfig) -> Self {
        self.env.cfg = cfg;
        self
    }

    /// 注入运行时常量包（M0-W3 注入管道）：写入 `env.cfg.constants`，随 cfg 线程化
    /// 到全部 calc 函数。未调用时为 `Default`（fallback，与入库 JSON 逐值相等）。
    ///
    /// **顺序约束**：[`with_config`](Self::with_config) 会整体覆盖 cfg（含本字段），
    /// 故须在其**之后**调用；编排层（pobr-build `calculate_with_data`）遵守此序。
    pub fn set_constants(&mut self, constants: RuntimeConstants) {
        self.env.cfg.constants = constants;
    }

    /// 在已注入全部来源后，向计算上下文写入一个资源/属性缩放量（PoB2 PerStat 的分母变量
    /// 的总量，如 `Spirit`/`Strength`/`Level`）。供 `+N to <stat> per M <resource>` 这类
    /// 词条经 [`crate::ModTag::Multiplier`] 在 `perform` 查询时按 `value / div` 展开。
    ///
    /// 须在 [`perform_minimal`](Self::perform_minimal) 之前调用；资源量通常由编排层在全部
    /// 来源注入后用 [`base_sum`](Self::base_sum) 读取（属性/Spirit 的 BASE 总量）后回填。
    pub fn set_multiplier(&mut self, name: impl Into<String>, value: f64) {
        self.env.cfg.multipliers.insert(name.into(), value);
    }

    /// 在已注入全部来源后，向计算上下文写入一个布尔条件（供 `ModTag::Condition` 词条
    /// 在 `perform` 查询时判定）。与 [`set_multiplier`](Self::set_multiplier) 同为
    /// 编排层回填入口，须在 [`perform_minimal`](Self::perform_minimal) 之前调用。
    pub fn set_condition(&mut self, name: impl Into<String>, value: bool) {
        self.env.cfg.conditions.insert(name.into(), value);
    }

    /// 在已注入全部来源后，向计算上下文写入一个已算出 stat 快照值（PoB2 GetStat 读
    /// actor **output** 的 PoBR 落点，见 [`crate::CalcConfig::stats`]）。供
    /// `ModTag::PerStat`/`PercentStat`（求值缩放）与 `StatThreshold`（matches gate）
    /// 词条取数。与 [`set_multiplier`](Self::set_multiplier) 同为编排层回填入口，
    /// 须在 [`perform_minimal`](Self::perform_minimal) 之前调用。
    pub fn set_stat(&mut self, name: impl Into<String>, value: f64) {
        self.env.cfg.stats.insert(name.into(), value);
    }

    /// 查询玩家 modDB 中某 FLAG modifier 是否为真（按当前 cfg）。供编排层把来源授予的
    /// `Condition:<X>` flag（如 Bonded 激活源）桥接为 cfg 条件。
    pub fn has_flag(&self, name: &str) -> bool {
        self.env
            .player
            .mod_db
            .flag(&self.env.cfg, ModName::from(name))
    }

    /// 低生命自动条件桥（vendor CalcDefence.lua:335-350：`(max − reserved)/max ≤
    /// LowPoolThreshold(0.35)` → `condList["LowLife"] = true`）。0.5.4b 起
    /// Spirit→Life 保留转换（Atziri's Communion）使重保留 build 自动进入 Low Life，
    /// 解锁「while on Low Life」族词条（tree + Direstrike 支援 buff）。
    ///
    /// 须在全部来源注入后、[`perform_minimal`](Self::perform_minimal) 之前调用
    /// （预留 mod 已注入，条件在 perform 的聚合查询期生效——vendor 同序：
    /// doActorLifeManaSpiritReservation 先于 calcs.offence）。预留聚合口径与
    /// perform 预留段逐式相同（ReservationMultiplier floor4 + efficiency 除数）。
    /// 显式 config 条件（`conditionLowLife`）优先，不被覆盖。
    // ponytail: 只桥 Life（唯一有 fixture 佐证的池）；LowMana/LowSpirit 同一
    // vendor 循环，出现消费 build 时按本函数样板扩展。LowLifePercentage 覆盖
    // stat（vendor :337）全 fixture 无来源，未建模。
    pub fn bridge_low_pool_conditions(&mut self) {
        if self.env.cfg.conditions.contains_key("LowLife") {
            return;
        }
        let life = self.pool_total("MaximumLife");
        if life <= 0.0 {
            return;
        }
        let db = &self.env.player.mod_db;
        let cfg = &self.env.cfg;
        let mult =
            (db.more(cfg, &[ModName::from("ReservationMultiplier")]) * 10_000.0).floor() / 10_000.0;
        let eff_names = [
            ModName::from("LifeReservationEfficiency"),
            ModName::from("ReservationEfficiency"),
        ];
        let eff_inc = db.sum(ModType::Inc, cfg, &eff_names).max(-100.0);
        let divisor = ((1.0 + eff_inc / 100.0) * db.more(cfg, &eff_names)).max(1e-12);
        let factor = mult / divisor;
        let flat = db.sum(ModType::Base, cfg, &[ModName::from("LifeReserved")]) * factor;
        let percent = db.sum(ModType::Inc, cfg, &[ModName::from("LifeReservedPercent")]) * factor;
        let reserved = super::survivability::reservation(life, flat, percent).reserved;
        if (life - reserved) / life
            <= self
                .env
                .cfg
                .constants
                .game_constants
                .game
                .low_pool_threshold
        {
            self.env.cfg.conditions.insert("LowLife".into(), true);
        }
    }

    pub fn add_modifier_texts(
        &mut self,
        texts: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), ParseError> {
        // A2 穿线：经 `parse_ctx` 走引擎/special/legacy 三态分发（注入引擎规则时走
        // 数据驱动引擎，否则与历史 `parse_mod` 逐值相等）。`parse_ctx` 不可变
        // 借 self，与下游 `self.env` 可变写入冲突——先收集 outcome 再消费。
        let mut outcomes = Vec::new();
        {
            let ctx = self.parse_ctx();
            for text in texts {
                let text = text.as_ref();
                outcomes.push((ctx.parse(text)?, text.to_string()));
            }
        }
        for (outcome, original) in outcomes {
            // Parsed（含带残留的部分解析）注入，Unsupported 收集。曾对「Parsed+残留」
            // 整行降级以防 converted-to 族误出 `A Base X` 错值，但那会连合法的 tag 后缀
            // 从句一并丢弃（`... if you have at least N Red Support Gems Socketed` 的
            // MultiplierThreshold 阈值行残留 `Max`）；且危险的 converted-to 变体已由
            // special_mods 数据修复成**干净解析（无残留）**，闸门遂冗余，还原直注。
            match outcome.status {
                ParseStatus::Parsed => self.env.player.mod_db.add_list(outcome.mods),
                ParseStatus::Unsupported => self.unsupported_modifier_texts.push(original),
            }
        }

        Ok(())
    }

    /// 直接注入已构造好的 modifier（角色基础值 / 战役奖励 / 抗性惩罚等入口的产物），
    /// 保留其 `SourceId` 归因。
    pub fn add_modifiers(&mut self, modifiers: impl IntoIterator<Item = Modifier>) {
        self.env.player.mod_db.add_list(modifiers);
    }

    /// 玩家 ModDb 只读视图——编排层的注入阶段查询已 ingest 的词条
    /// （如 Spirit 预留按 per-gem cfg 取 `ReservationEfficiency` INC）。
    /// 反映**调用时点**的库内容：树/装备词条在早期注入步 ingest，之后的
    /// 注入函数可据此取数。
    pub fn mod_db(&self) -> &crate::ModDb {
        &self.env.player.mod_db
    }

    /// 接入一件装备：按 section（implicit / explicit / enchant）解析其词条文本为
    /// 带槽位 + 来源类别归因的 modifier 并注入计算，无法解析的词条收集进
    /// `unsupported_modifier_texts`。
    pub fn add_item(&mut self, slot: EquipmentSlot, item: &Item) -> Result<(), ParseError> {
        let ingest = ingest_item_with_ctx(slot, item, self.parse_ctx())?;
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
        Ok(())
    }

    /// 接入一件**武器**（调用方判定基底为武器）：同 [`Self::add_item`]，另按 vendor
    /// `Item.lua:1954-1961` 把无 flag 的爆伤词条转为按手条件
    /// （见 [`crate::item::apply_weapon_hand_conditions`]）。
    pub fn add_weapon_item(&mut self, slot: EquipmentSlot, item: &Item) -> Result<(), ParseError> {
        let mut ingest = ingest_item_with_ctx(slot, item, self.parse_ctx())?;
        crate::item::apply_weapon_hand_conditions(&mut ingest.modifiers, slot);
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
        Ok(())
    }

    /// 接入一件**激活态**药剂/护符（M3-T4 通道切换）：词条经
    /// [`crate::item::ingest_flask_charm`] 打包为 `FlaskBuff`/`CharmBuff` 载荷
    /// List mod 注入（List 不参与 sum/more/flag 聚合 → 未合并前零直接影响），由
    /// env_finalize 阶段 3 `merge_flasks_charms` 在 `mode_combat` 门控下按 effect
    /// 乘区合并进计算（vendor CalcPerform.lua:1429-1663）。激活态语义由调用方
    /// 槽位 `active` 门控承担（vendor CalcSetup.lua:1014-1028）；不可解析词条
    /// 收集进 `unsupported_modifier_texts`。
    pub fn add_flask_charm(&mut self, slot_name: &str, item: &Item) {
        let ingest = crate::item::ingest_flask_charm_with_ctx(slot_name, item, self.parse_ctx());
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
    }

    /// 接入一组已分配天赋节点：解析每个节点的词条文本为带节点归因
    /// （[`SourceKind::PassiveNode`] / [`SourceKind::AscendancyNode`]）的 modifier 并注入计算，
    /// 无法解析的词条收集进 `unsupported_modifier_texts`。
    ///
    /// [`SourceKind::PassiveNode`]: pobr_data::source::SourceKind::PassiveNode
    /// [`SourceKind::AscendancyNode`]: pobr_data::source::SourceKind::AscendancyNode
    pub fn add_passive_nodes(&mut self, nodes: &[AllocatedNode]) -> Result<(), ParseError> {
        let ingest = ingest_passive_nodes_with_ctx(nodes, self.parse_ctx())?;
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
        Ok(())
    }

    /// 接入一颗宝石：解析其词条文本为带宝石归因的 modifier 并注入计算，
    /// 无法解析的词条收集进 `unsupported_modifier_texts`。
    ///
    /// 主动宝石归因到 `SourceKind::SkillGem`，辅助宝石归因到 `SourceKind::SupportGem`
    /// （并链接到被支援主动技能的 source，若 `supported_gem_id` 可得）。
    pub fn add_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        let ingest = ingest_gem_with_ctx(gem, self.parse_ctx())?;
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
        Ok(())
    }

    /// 接入一颗主动技能宝石（`SourceKind::SkillGem` 归因）。`add_gem` 的便捷封装。
    pub fn add_skill_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        self.add_gem(gem)
    }

    /// 接入一颗辅助宝石（`SourceKind::SupportGem` 归因）。`add_gem` 的便捷封装。
    pub fn add_support_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        self.add_gem(gem)
    }

    /// 注入一个 buff 技能规格（M3 T0-4 接口契约，蓝图 §2.4）。
    ///
    /// **本阶段只存不消费**：spec 入 `Env::buff_skills`，等 T3 的 `buff_pass`
    /// （env_finalize 阶段 4）落地后才参与计算——在此之前调用本 API 对输出逐值无影响。
    pub fn add_buff_skill(&mut self, spec: BuffSpec) {
        self.env.buff_skills.push(spec);
    }

    /// 接入一个召唤物（M5a-B2）：从 [`MinionDef`](super::MinionDef) 真实底材 + 召唤
    /// 宝石等级 + 数量上限派生召唤物 `Actor` 接入 `Env.minions`，并把上限写为玩家
    /// `Multiplier:SummonedMinion` / `Multiplier:MinionPresenceCount`（供「per Minion」
    /// 族词条引用）。`Env::add_minion_from_def` 的 session 直通封装。
    ///
    /// 由编排层在识别召唤宝石（`effect_minion_list` 非空）后调用；不识别召唤物的
    /// build 永不调用，故对既有非召唤 build 零行为影响。`perform_minimal` 在末尾对
    /// 每个召唤物跑同一套 offence/defence，结果落 `OutputTable.minions`。
    #[allow(clippy::too_many_arguments)]
    pub fn add_minion_from_def(
        &mut self,
        def: &super::MinionDef,
        gem_level: u32,
        limit: u32,
        minion_modifiers: Vec<super::MinionModifierEntry>,
        ally_buff_mods: Vec<Modifier>,
        infusion: super::AttributeInfusion,
    ) {
        self.env.add_minion_from_def(
            def,
            gem_level,
            limit,
            minion_modifiers,
            ally_buff_mods,
            infusion,
        );
    }

    /// 注入「keystone 名 → 该 keystone 的 modifier 列表」映射（M3 T0-4 接口契约，
    /// 蓝图 §2.4，T5 mergeKeystones 消费）。
    ///
    /// **本阶段只存不消费**：map 入 `Env::keystone_mods`，等 T5 的 `merge_keystones`
    /// （env_finalize 阶段 1/5）落地后，词条授予的 keystone 才据此注入 modDB。
    pub fn set_keystone_mods(&mut self, map: BTreeMap<String, Vec<Modifier>>) {
        self.env.keystone_mods = map;
    }

    /// 注入 curse 优先级数据表（M3-T3 C3；`overlay/curse_priority.json` 经
    /// pobr-gamedata 加载后由编排层喂入，照 [`set_buff_definitions`] 先例）。
    /// env_finalize 阶段 4 `buff_pass` 的 curse priority 计算消费——整段吃
    /// `cfg.mode_buffs` 门控（默认 false），故未显式置位时注入与否输出逐值不变。
    /// 未注入/缺表 = 权重全 0 回退（R7 缺表容忍）。
    ///
    /// [`set_buff_definitions`]: Self::set_buff_definitions
    pub fn set_curse_priority(
        &mut self,
        def: pobr_data::catalog::curse_priority::CursePriorityDef,
    ) {
        self.env.curse_priority = Some(def);
    }

    /// 注入内建 buff 定义表（M3-T2 B3；`overlay/buff_definitions.json` 经
    /// pobr-gamedata 加载后由编排层喂入）。env_finalize 阶段 6
    /// `expand_misc_buffs` 消费——整段吃 `cfg.mode_combat` 门控（默认 false），
    /// 故未显式置位 mode_combat 时注入与否输出逐值不变。
    pub fn set_buff_definitions(&mut self, defs: Vec<BuffDef>) {
        self.env.buff_definitions = defs;
    }

    /// 注入 handler 注册表（数据条目 `handler_id` → Rust 真逻辑；聚合点 =
    /// pobr-build `handlers::build_registry()`）。未调用时为空注册表——
    /// handler 条目保守零输出（进 unhandled 报表，宁缺勿错值）。
    pub fn set_buff_handler_registry(&mut self, registry: Arc<HandlerRegistry>) {
        self.env.buff_handler_registry = registry;
    }

    /// 注入取整精度规则（M4-I 去重；`overlay/high_precision_mods.json` 经
    /// pobr-gamedata `RuleSet::high_precision_mods` 加载后由编排层喂入，照
    /// [`set_curse_priority`](Self::set_curse_priority) 先例）。消费点 =
    /// buff_pass / merge_flasks_charms 的 ScaleAddMod 数值缩放（与 T1 写原语
    /// [`crate::ModDb::scale_add_mod`] 同一份规则）。未注入 =
    /// [`crate::HighPrecisionRules::default`]（无例外表 fallback）。
    ///
    /// 注：本注入**不**写 `ModDb::set_high_precision_rules`（MORE 聚合精度
    /// 例外分支的开关）——那是独立的行为切换 commit（T1 域），此处仅供
    /// 缩放路径消费，保证注入本身对 MORE 聚合逐值不变。
    pub fn set_high_precision_rules(&mut self, rules: crate::HighPrecisionRules) {
        self.env.high_precision = rules;
    }

    /// 注入 MH/OH hand pass 输入（M4-T2 W-B2，蓝图 §3.3 契约 1；编排层武器段构造
    /// [`HandSource`](super::hand_pass::HandSource)）。未调用 = 空，`perform` 走与
    /// 历史完全一致的单管线路径（回退态）。`double_hits` = 技能数据
    /// `doubleHitsWhenDualWielding`（W-D1 数据通道落地前恒 false）。
    pub fn set_hand_sources(
        &mut self,
        sources: Vec<super::hand_pass::HandSource>,
        double_hits: bool,
    ) {
        self.env.hand_sources = sources;
        self.env.double_hits_when_dual_wielding = double_hits;
    }

    /// 按 `(config_level, tier)` 初始化敌人（怪物缩放 + 档位加成），写入 `Env.enemy`
    /// 的标量基础与 modDB（归因 [`SourceKind::EnemyConfig`]）。
    ///
    /// 仅在 `CalcConfig::mode_effective == true`（经 [`with_config`](Self::with_config)
    /// 设置）时影响有效 DPS / 命中 / 敌人减伤；面板口径不读取敌人交互，保持与历史输出一致。
    ///
    /// [`SourceKind::EnemyConfig`]: pobr_data::source::SourceKind::EnemyConfig
    pub fn setup_enemy(&mut self, config_level: u32, tier: EnemyTier) {
        super::setup_env::setup_enemy(&mut self.env, config_level, tier);
    }

    /// 直接向**敌人** modDB 注入已构造好的 modifier（保留 `SourceId` 归因）。
    ///
    /// M3-T1 A5 config 主路径入口：config 解释器把 `conditionEnemy<X>` 等条目
    /// 落成 enemy 桶产物（vendor `enemyModList:NewMod` 语义，ConfigOptions.lua
    /// 各 enemy 条目；`SourceKind::EnemyConfig` 归因），编排层经此注入。
    /// 须在 [`setup_enemy`](Self::setup_enemy) 之后调用（与 vendor enemy modDB
    /// 装配序一致；BASE 求和本身次序无关）。
    pub fn add_enemy_modifiers(&mut self, modifiers: impl IntoIterator<Item = Modifier>) {
        self.env.enemy.mod_db.add_list(modifiers);
    }

    /// 注入玩家施加的元素**曝光**（`[fire, cold, lightning]`，PoB2 config 默认每点 -20%
    /// 抗）：只写 enemy modDB 的 `<Element>Exposure BASE`；折算为 `<Element>Resist`
    /// 减项的归约统一发生在 `env_finalize` 阶段 8（[`reduce_enemy_exposure`]，
    /// vendor CalcPerform.lua:3214-3247——M4-L 起 buff_pass Debuff 路径（如
    /// Frost Bomb）也产曝光，归约收口单点防双扣）。仅在有效口径
    /// （`mode_effective`）下对伤害生效。须在 [`setup_enemy`](Self::setup_enemy)
    /// 之后调用。
    ///
    /// [`reduce_enemy_exposure`]: super::setup_env::reduce_enemy_exposure
    pub fn apply_enemy_exposure(&mut self, elements: [bool; 3], magnitude: f64) {
        let names = ["FireExposure", "ColdExposure", "LightningExposure"];
        for (on, name) in elements.iter().zip(names) {
            if *on {
                self.env.enemy.mod_db.add_list([Modifier::number(
                    ModName::from(name),
                    ModType::Base,
                    magnitude,
                )
                .with_source("config exposure")]);
            }
        }
    }

    pub fn perform_minimal(&mut self) -> MinimalOutput {
        perform(&mut self.env).expect("CalculationSession constructs a valid player actor");
        MinimalOutput::from_output_and_breakdown(
            &self.env.player.output,
            &self.env.player.breakdown,
        )
    }

    pub fn unsupported_modifier_texts(&self) -> &[String] {
        &self.unsupported_modifier_texts
    }

    /// 取玩家 modDB 中某 ModName 的 BASE 之和（按当前 cfg）。供编排层在全部来源注入后
    /// 读取总属性（Strength/Dexterity/Intelligence）以派生 life/mana/accuracy（属性派生需
    /// 用**最终**属性，而非仅职业基础）。
    pub fn base_sum(&self, name: &str) -> f64 {
        self.env
            .player
            .mod_db
            .sum(ModType::Base, &self.env.cfg, &[ModName::from(name)])
    }

    /// 主技能 Life 消耗快照（vendor `output.LifeCost`；含 hybrid mana→life 转换）。
    /// 编排层在全部来源注入后调用，回填 `cfg.stats/multipliers["LifeCost"]` 供
    /// per-life-cost 词条（PerStat stat=LifeCost，如 Atalui's Bloodletting 的
    /// gain-as-physical）在伤害聚合期取数——与 vendor CalcOffence 先算 cost 再算
    /// 伤害的顺序等价。
    pub fn life_cost_snapshot(&self) -> f64 {
        let base_mc = self.base_sum("SkillManaCostBase");
        let base_lc = self.base_sum("SkillLifeCostBase");
        super::skill_mechanics::calc_life_cost_hybrid(
            &self.env.player.mod_db,
            &self.env.cfg,
            base_lc,
            base_mc,
        )
        .final_cost
    }

    /// 属性最终总量（PoB2 `calculateAttributes`，CalcPerform.lua:381-388：
    /// `output[stat] = m_max(round(calcLib.val(modDB, stat)), 0)`，calcLib.val =
    /// `Σbase × (1 + Σinc/100) × Πmore`）。`class_base` = 职业起始属性（PoBR 把它
    /// 烘焙在 CharacterBase 派生值里、不作为 `Strength`/`Dexterity` modifier 入库，
    /// 故在此作为 BASE 加项参与 INC/MORE 缩放，与 vendor「职业属性也是 modDB BASE
    /// mod、同受 `N% increased <Attr>` 缩放」的口径对齐）。
    pub fn attribute_total(&self, name: &str, class_base: f64) -> f64 {
        let names = [ModName::from(name)];
        let db = &self.env.player.mod_db;
        let base = class_base + db.sum(ModType::Base, &self.env.cfg, &names);
        let inc = db.sum(ModType::Inc, &self.env.cfg, &names);
        let more = db.more(&self.env.cfg, &names);
        (base * (1.0 + inc / 100.0) * more).round().max(0.0)
    }

    /// 资源池最终总量（life/mana）：与 `perform` 内 offence 的池值计算同一管线
    /// （OVERRIDE 胜出 → `(actor_base + Σbase) × (1 + Σinc/100) × Πmore`，
    /// `offence::scaled_pool` 同源），即 vendor `output.Life/Mana`
    /// （CalcOffence pool 段）。供编排层在全部来源注入后回填 PerStat 资源分母
    /// （vendor PerStat tag 读 actor **output**，ModStore.lua:440-460 GetStat）——
    /// [`base_sum`](Self::base_sum) 只取 BASE 之和，会漏掉 inc/more 缩放后的池值。
    /// Spirit 最终池值（vendor `output.Spirit`，[`calc_spirit_pool`] 同源：
    /// OVERRIDE → (base + Extra) × 未转换比例 × (1+Σinc/100) × Πmore，round）。
    /// 供编排层回填 PerStat `Spirit` 分母——vendor PerStat 读 actor output
    /// （ModStore.lua:440-460 GetStat），BASE-only 会把「+2 Armour per 1 Spirit」
    /// （wolf-pack Perfidy，Spirit 336 vs base 300）欠算。
    ///
    /// [`calc_spirit_pool`]: super::calc_spirit_pool
    pub fn spirit_total(&self) -> f64 {
        super::calc_spirit_pool(&self.env.player.mod_db, &self.env.cfg)
    }

    pub fn pool_total(&self, name: &str) -> f64 {
        let actor_base = match name {
            "MaximumLife" => self.env.player.base.life,
            "MaximumMana" => self.env.player.base.mana,
            _ => 0.0,
        };
        super::offence::scaled_pool(&self.env.player.mod_db, &self.env.cfg, actor_base, name)
    }

    /// 取玩家完整 [`OutputTable`]（`perform`/`perform_minimal` 后填满）。包含
    /// armour/evasion/ES、异常、EHP、技能机制等全部 fill 阶段字段——`MinimalOutput`
    /// 仅是其攻击/抗性子集，需要完整输出时用此。
    pub fn output(&self) -> &OutputTable {
        &self.env.player.output
    }

    /// 诊断辅助：列出玩家 modDB 中名为 `name` 的全部 modifier（含 cfg 不匹配者），
    /// 供 parity 调试逐来源核对贡献。
    pub fn mods_named(&self, name: &str) -> Vec<&Modifier> {
        let target = ModName::from(name);
        self.env
            .player
            .mod_db
            .iter_mods()
            .filter(|m| m.name == target)
            .collect()
    }

    /// 玩家 ModDb 全部 modifier（诊断用，`POBR_DBG_ALLMODS`）：用于 engine vs legacy
    /// ingest 路径的逐 mod 全集 diff（M6 fork(a) 定位 ingest 分歧）。
    pub fn all_mods(&self) -> Vec<&Modifier> {
        self.env.player.mod_db.iter_mods().collect()
    }
}
