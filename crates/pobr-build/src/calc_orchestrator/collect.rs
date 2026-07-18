//! collect — 角色基础 / 被动节点 / jewel 半径扩展 / keystone / item·gem 收集。

use super::*;

/// 从职业名 + 等级派生 [`CharacterBase`]（属性取职业起始值；树/装备属性加成走
/// modifier 管线，本入口只落地固有派生）。未知职业返回 `None`（跳过 CharacterBase 注入）。
pub(crate) fn character_base(build: &Build, data: &BuildData) -> Option<CharacterBase> {
    let attrs = data.class_attributes(&build.character.class_name)?;
    Some(CharacterBase {
        level: build.character.level,
        strength: f64::from(attrs.strength),
        dexterity: f64::from(attrs.dexterity),
        intelligence: f64::from(attrs.intelligence),
    })
}

/// 把已分配天赋节点解析为带节点归因的 [`AllocatedNode`]（经
/// [`collect_allocated_mods_for_class`] 完成 JewelSocket / Mastery gating +
/// isSwitchable 按职业/飞升变体选择，未知节点跳过）。
pub(crate) fn resolve_passive_nodes(build: &Build, data: &BuildData) -> Vec<AllocatedNode> {
    // isSwitchable 变体上下文（PoB curClassName / curAscendClassName，
    // PassiveSpec.lua:1251-1256）：来源 = Build XML 头部职业/飞升名。
    let class = ClassContext {
        class_name: &build.character.class_name,
        ascendancy_name: &build.character.ascendancy_name,
    };
    // 多版本树适配（PoB2 TreeData/<v> 对位）：旧赛季 build 按 `<Spec
    // treeVersion>` 选历史树词条（如 53853『Backup Plan』0_3 = 50/50 两条 vs
    // 0_5 = 20/40/40 三条）；无对应历史树数据落回默认树。
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    collect_allocated_mods_for_class(&build.tree, nodes, class)
        .into_iter()
        .map(|node| {
            // 飞升节点由其 PassiveNodeDef::ascendancy_id 判定。
            let ascendancy = nodes
                .get(&node.node_id.0)
                .map(|def| def.ascendancy_id.is_some())
                .unwrap_or(false);
            AllocatedNode {
                node_id: node.node_id,
                ascendancy,
                modifier_texts: combine_wrapped_then_filter(node.modifier_texts, engine_ctx(data)),
            }
        })
        .collect()
}

/// （M4-H）油涂授予的 notable：扫描全部装备/珠宝词条行，解析出 `GrantedPassive`
/// LIST（vendor `Allocates <name>` enchant，ModParser.lua:5809），按名称
/// （ASCII 不区分大小写——解析侧已小写归一）匹配 **Notable** 节点（vendor
/// `spec.tree.notableMap`，CalcSetup.lua:1322-1331 只查 notable），追加为
/// [`AllocatedNode`]。已分配节点跳过（vendor `allocNodes[id]` 幂等同语义）。
/// 同名 notable（切换类变体等）按最小 skill id 取（确定性）。
pub(crate) fn append_granted_passives(
    build: &Build,
    data: &BuildData,
    nodes: &mut Vec<AllocatedNode>,
) {
    let mut allocated: std::collections::HashSet<u32> = nodes.iter().map(|n| n.node_id.0).collect();
    for def in granted_passive_defs(build, data) {
        if !allocated.insert(def.skill) {
            continue; // 已分配/已授予，幂等。
        }
        nodes.push(AllocatedNode {
            node_id: pobr_data::passive_tree::NodeId(def.skill),
            ascendancy: def.ascendancy_id.is_some(),
            // 树 stat 同样可能折行（与 combine_wrapped_then_filter 同源数据）。
            modifier_texts: combine_wrapped_then_filter(def.stats.clone(), engine_ctx(data)),
        });
    }
}

/// 从 `data` 取数据驱动解析规则打包成解析上下文（注入时走新引擎，缺规则的旧数据包
/// 回退旧解析器，逐值不变）。供未经 `CalculationSession`（已自带规则注入）的零散
/// passive ingest 调用统一走与主路径一致的解析路径。
pub(crate) fn engine_ctx(data: &BuildData) -> ParseCtx<'_> {
    match data.parser_rules.as_deref() {
        Some(rules) => ParseCtx::with_engine(rules),
        None => ParseCtx::none(),
    }
}

/// 解析全部装备/珠宝词条行的 `GrantedPassive`（`Allocates <name>` enchant），按
/// 名称匹配 Notable 节点定义并去重返回（[`append_granted_passives`] 与
/// [`gem_property_bonuses`] 的共享解析；语义注释见前者）。
pub(crate) fn granted_passive_defs<'d>(
    build: &Build,
    data: &'d BuildData,
) -> Vec<&'d pobr_data::catalog::PassiveNodeDef> {
    use pobr_core::ModValue;

    // 收集授予名（装备三段 + 珠宝词条；解析失败行静默跳过——与 skip-and-collect 同口径）。
    let item_texts = build.items.values().flat_map(|item| {
        item.implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
    });
    let jewel_texts = build.jewels.iter().flat_map(|jewel| {
        jewel
            .implicit_texts
            .iter()
            .chain(&jewel.modifier_texts)
            .chain(&jewel.enchant_texts)
    });
    let ctx = engine_ctx(data);
    let mut granted: Vec<String> = Vec::new();
    for text in item_texts.chain(jewel_texts) {
        let Ok(outcome) = ctx.parse(text) else {
            continue;
        };
        for m in outcome.mods {
            if m.name.as_str() == "GrantedPassive"
                && let ModValue::Text(name) = &m.value
            {
                granted.push(name.clone());
            }
        }
    }
    if granted.is_empty() {
        return Vec::new();
    }

    // notable 名（小写）→ 节点（同名取最小 skill id，确定性）。
    let mut by_name: std::collections::BTreeMap<String, &pobr_data::catalog::PassiveNodeDef> =
        std::collections::BTreeMap::new();
    for def in data.passive_nodes.values() {
        if def.kind != pobr_data::catalog::PassiveNodeKind::Notable {
            continue;
        }
        let Some(name) = &def.name else { continue };
        by_name
            .entry(name.to_ascii_lowercase())
            .and_modify(|existing| {
                if def.skill < existing.skill {
                    *existing = def;
                }
            })
            .or_insert(def);
    }

    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for name in granted {
        let Some(def) = by_name.get(name.to_ascii_lowercase().as_str()) else {
            continue; // 未知名（树外/变体），欠算安全跳过。
        };
        if seen.insert(def.skill) {
            out.push(*def);
        }
    }
    out
}

/// 树节点词条的「折行合并」解析（M4-H；vendor `PassiveTree.lua:445-462`：单行
/// parse 失败时与后续行逐次拼接重试，成功则消耗被并入的行，全部失败则按原样
/// 丢弃该行、后续行继续独立解析）。
///
/// 树数据的多词长 stat 会被折成多行（vendor tree.lua sd 数组 / 入库 JSON 的
/// `\n`，经 `pobr_tree::split_lines` 摊平）——如 Demolitionist
/// 「Gain 4% of Damage as Extra Fire Damage for / every different Grenade
/// fired in the past 8 seconds」是**一条** mod 的两行。仅树路径需要此合并
/// （装备词条本就按行入库）。
pub(crate) fn combine_wrapped_then_filter(texts: Vec<String>, ctx: ParseCtx<'_>) -> Vec<String> {
    let parses = |t: &str| gate_parses(ctx, t);
    let mut out = Vec::new();
    let mut i = 0;
    while i < texts.len() {
        if parses(&texts[i]) {
            out.push(texts[i].clone());
            i += 1;
            continue;
        }
        // 与后续行逐次拼接重试（vendor :448-462 comb 循环）。
        let mut combined: Option<(String, usize)> = None;
        for end in (i + 1)..texts.len() {
            let comb = texts[i..=end].join(" ");
            if parses(&comb) {
                combined = Some((comb, end));
                break;
            }
        }
        match combined {
            Some((comb, end)) => {
                out.push(comb);
                i = end + 1;
            }
            None => {
                // 诊断口径与 filter_parseable 一致（结构性丢弃可见性）。
                if std::env::var("POBR_DBG_DROPPED").is_ok() {
                    eprintln!("[POBR_DROP] {}", texts[i]);
                }
                i += 1;
            }
        }
    }
    out
}

/// （M3 T5-E2）keystone 名 → 该 keystone 的 modifier 列表（树 keystone 节点 stats
/// 经 passive ingest 解析）。供「You have \<Keystone\>」类授予词条在 env_finalize
/// `merge_keystones`（CalcPerform.lua:66-76 等价）阶段注入。
///
/// **排除已点 keystone**：其 mods 已由 `add_passive_nodes` 以 Tree 归因注入，map
/// 缺键＝merge 静默跳过——等价 PoB2 `env.keystonesAdded` 对树/词条双来源的去重。
pub(crate) fn keystone_mod_map(
    data: &BuildData,
    allocated: &[AllocatedNode],
) -> std::collections::BTreeMap<String, Vec<Modifier>> {
    let allocated_ids: std::collections::HashSet<u32> =
        allocated.iter().map(|n| n.node_id.0).collect();
    let mut map = std::collections::BTreeMap::new();
    for (id, def) in &data.passive_nodes {
        if def.kind != pobr_data::catalog::PassiveNodeKind::Keystone || allocated_ids.contains(id) {
            continue;
        }
        let Some(name) = def.name.clone() else {
            continue;
        };
        let node = AllocatedNode {
            node_id: pobr_data::passive_tree::NodeId(*id),
            ascendancy: def.ascendancy_id.is_some(),
            modifier_texts: filter_parseable(def.stats.clone(), engine_ctx(data)),
        };
        // 解析失败（硬错）/ 零产出的 keystone 不入 map（merge 端静默跳过，欠算安全）。
        let Ok(ingest) = pobr_core::passive::ingest_passive_nodes_with_ctx(
            std::slice::from_ref(&node),
            engine_ctx(data),
        ) else {
            continue;
        };
        if !ingest.modifiers.is_empty() {
            map.insert(name, ingest.modifiers);
        }
    }
    map
}

/// 把 `Radius:` 档位文本映射为 [`JewelRadius`]。未识别/缺失时回退 `Large`
/// （PoB2 树珠宝默认半径），保持几何近似可用。
pub(crate) fn parse_jewel_radius(label: Option<&str>) -> JewelRadius {
    match label.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("small") => JewelRadius::Small,
        Some("medium") => JewelRadius::Medium,
        Some("large") => JewelRadius::Large,
        Some("very large") => JewelRadius::VeryLarge,
        _ => JewelRadius::Large,
    }
}

/// 范围珠宝 `also grant` 行的目标节点种类（由前缀决定授予对象）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrantTargetKind {
    Notable,
    /// `Small Passive Skills` = 普通（非 notable/keystone/socket/mastery）节点。
    Small,
}

/// 解析 `<Kind> Passive Skills in Radius also grant <mod>` 行 → (目标种类, 授予 mod 文本)。
///
/// 仅识别 `Notable` / `Small` 前缀；其它前缀（如 keystone 授予，目前未见样本）返回 None。
pub(crate) fn parse_grant_line(line: &str) -> Option<(GrantTargetKind, String)> {
    const MARKER: &str = "Passive Skills in Radius also grant";
    let idx = line.find(MARKER)?;
    let prefix = line[..idx].trim();
    let kind = match prefix.to_ascii_lowercase().as_str() {
        "notable" => GrantTargetKind::Notable,
        "small" => GrantTargetKind::Small,
        _ => return None,
    };
    let granted = line[idx + MARKER.len()..].trim();
    if granted.is_empty() {
        None
    } else {
        Some((kind, granted.to_string()))
    }
}

/// 属性小点判定（PoB2 tree.lua `isAttribute=true` 节点的 PoBR 等价）：词条为
/// `+N to any [Attributes|Attribute]` 三选一形式。catalog 不带 isAttribute 旗标，
/// 按节点词条文本判定（与 pobr-tree 属性三选一改写使用同一文本形式）。
pub(crate) fn is_attribute_node(def: &pobr_data::catalog::PassiveNodeDef) -> bool {
    def.stats.iter().any(|s| {
        let lower = s.to_ascii_lowercase();
        lower.contains(" to any ") && lower.contains("attribute")
    })
}

/// 单个范围珠宝的几何展开结果：半径内**已分配**的 notable 节点 id 列表与
/// small（普通且非属性）节点计数。
pub(crate) struct RadiusJewelExpansion<'a> {
    jewel: &'a RadiusJewel,
    /// 半径内已分配 Notable 节点 id（含属性 notable；效果缩放消费方自行再过滤）。
    notable_nodes: Vec<u32>,
    small_count: usize,
}

/// 对全部范围珠宝做半径几何展开（圆心 = 插槽节点坐标，档位 = `Radius:` 行）。
///
/// 候选只在**已分配**节点集合中筛；socket 坐标缺失或几何计算失败的珠宝跳过
/// （不臆造）。供 [`radius_jewel_grant_texts`]（授予词条展开）与
/// [`radius_jewel_notable_effect_copies`]（notable 效果缩放）共享。
pub(crate) fn radius_jewel_expansions<'a>(
    build: &'a Build,
    data: &BuildData,
) -> Vec<RadiusJewelExpansion<'a>> {
    if build.radius_jewels.is_empty() {
        return Vec::new();
    }
    // 已分配节点集合（含种类）。坐标取自 data.passive_nodes（树数据回填的 x/y）。
    //
    // 只取**激活武器集**的已分配节点：PoB2 虽把非激活集专属点留在 allocNodes，且范围
    // 珠宝授予也会写进它们的 modList，但节点上的**每条** mod（含珠宝授予）都会被追加
    // `Condition: WeaponSet<N>`（CalcSetup.lua:222-223，节点自身 allocMode 优先于
    // 珠宝来源的 :224-227 分支）——非激活集节点上的授予净效果为零。oracle Tabulate
    // 实证（gemling crit jewel）：6 个 in-radius notable 只有激活集的 1 个贡献 +7
    // （critModList 仅一条 `7 @ Tree:32763`，CritChance 8.55）。PoBR 解析层已按激活集
    // 剔除非激活专属点，这里直接用 `tree.allocated_nodes` 即等价。
    let allocated: std::collections::HashSet<u32> =
        build.tree.allocated_nodes.iter().map(|n| n.0).collect();

    // 位置表：socket 自身 + 全部已分配节点（候选只在已分配集合中筛）。
    let mut positions: std::collections::HashMap<u32, (f64, f64)> =
        std::collections::HashMap::new();
    for (&skill, def) in &data.passive_nodes {
        if let (Some(x), Some(y)) = (def.x, def.y)
            && allocated.contains(&skill)
        {
            positions.insert(skill, (x, y));
        }
    }

    let mut out: Vec<RadiusJewelExpansion<'a>> = Vec::new();
    for jewel in &build.radius_jewels {
        // socket 坐标须可得，否则无法定圆心（跳过，不臆造）。
        let Some(socket_pos) =
            data.passive_nodes
                .get(&jewel.socket_node)
                .and_then(|d| match (d.x, d.y) {
                    (Some(x), Some(y)) => Some((x, y)),
                    _ => None,
                })
        else {
            continue;
        };

        let radius = parse_jewel_radius(jewel.radius_label.as_deref());

        // 把 socket 坐标并入位置表（compute 会排除 socket 自身）。
        let mut pos = positions.clone();
        pos.insert(jewel.socket_node, socket_pos);

        // 档位有效半径由注入的 jewel_radii 数据解析（M0-W3；无数据时 BuildData
        // 已回退 Default，与 JSON 逐值相等，输出不变）。
        let effect = match compute_radius_jewel_effect_with_radii(
            jewel.socket_node,
            radius,
            &data.jewel_radii,
            &pos,
            Vec::new(),
        ) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // 半径内已分配节点按种类归集。Small 排除属性小点（`+5 to any Attribute`
        // 三选一节点）：vendor `<Kind> Passive Skills in Radius also grant` 处理函数
        // 要求 `node.type == "Normal" and not node.isAttribute`（PoB2
        // ModParser.lua:6855-6857，tree.lua 对应节点带 `isAttribute=true`）。
        let mut notable_nodes: Vec<u32> = Vec::new();
        let mut small_count = 0usize;
        for &skill in &effect.affected_nodes {
            let Some(def) = data.passive_nodes.get(&skill) else {
                continue;
            };
            match def.kind {
                pobr_data::catalog::PassiveNodeKind::Notable => notable_nodes.push(skill),
                pobr_data::catalog::PassiveNodeKind::Normal if !is_attribute_node(def) => {
                    small_count += 1;
                }
                _ => {}
            }
        }
        notable_nodes.sort_unstable();
        out.push(RadiusJewelExpansion {
            jewel,
            notable_nodes,
            small_count,
        });
    }
    out
}

/// vendor `ModStore:ScaleAddMod` 的数值缩放语义（ModStore.lua:45-80）：
/// `m_modf(round(value * scale, 2))` —— 先两位小数四舍五入、再**截尾取整**
/// （朝零方向，如 `30.5 → 30`、`14.76 → 14`）。
pub(crate) fn vendor_scale_mod_value(value: f64, scale: f64) -> f64 {
    let rounded = (value * scale * 100.0).round() / 100.0;
    rounded.trunc()
}

/// 把词条文本里**第一个**数值 token 按 [`vendor_scale_mod_value`] 缩放后回写
/// （如 `10% increased X` ×1.22 → `12% increased X`）。无数值 token 时返回 None
/// （flag 类词条不缩放，vendor 同语义——非数值 mod 原样 AddMod）。
pub(crate) fn scale_leading_number(text: &str, scale: f64) -> Option<String> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let end = text[start..]
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .map(|i| start + i)
        .unwrap_or(text.len());
    let value: f64 = text[start..end].parse().ok()?;
    let scaled = vendor_scale_mod_value(value, scale);
    Some(format!("{}{}{}", &text[..start], scaled, &text[end..]))
}

/// 把所有范围珠宝的 `also grant` 词条按半径几何展开为全局 modifier 文本。
///
/// 对每个珠宝：以插槽节点坐标为圆心、按 `Radius:` 档位筛出**已分配**节点，按种类计数
/// （notable / small=普通），每个 `also grant` 行按对应计数注入 `count` 份授予 mod 文本。
/// 这复刻 PoB2「半径内每个该种类（已分配）节点各获得一份授予」的累加效果。
///
/// Notable 效果缩放（Time-Lost『N% increased Effect of Notable Passive Skills in
/// Radius』）：vendor 把授予 mod 写进节点 modList 后对 Notable 节点整体
/// ScaleAddList（CalcSetup.lua:246-275），等价于授予值 ×(1+inc/100)（截尾，
/// [`vendor_scale_mod_value`]）。半径重叠时 vendor 同节点后写覆盖单一效果，
/// PoBR 按授予珠宝自身效果近似（当前样本无重叠）。
pub(crate) fn radius_jewel_grant_texts(build: &Build, data: &BuildData) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for exp in radius_jewel_expansions(build, data) {
        let notable_scale = 1.0 + f64::from(exp.jewel.notable_effect_inc) / 100.0;
        for line in &exp.jewel.grant_lines {
            let Some((kind, granted)) = parse_grant_line(line) else {
                continue;
            };
            let (count, text) = match kind {
                GrantTargetKind::Notable => {
                    let scaled = if exp.jewel.notable_effect_inc > 0 {
                        scale_leading_number(&granted, notable_scale).unwrap_or(granted)
                    } else {
                        granted
                    };
                    (exp.notable_nodes.len(), scaled)
                }
                GrantTargetKind::Small => (exp.small_count, granted),
            };
            for _ in 0..count {
                out.push(text.clone());
            }
        }
    }
    out
}

/// （M4-n）范围珠宝 Notable 效果对**节点自身词条**的缩放副本。
///
/// vendor CalcSetup.lua:246-275：对半径内每个『Notable 且非属性且非飞升』节点的
/// modList 整体 `ScaleAddList ×(1+inc/100)`（数值 = [`vendor_scale_mod_value`]
/// 截尾语义）。PoBR 等价：基础份已按 1.0 注入（add_passive_nodes），此处追加
/// **数值差额副本** `trunc(round(v×scale,2)) − v`（BASE/INC；MORE 的乘性缩放无
/// 加性等价、树 notable 当前无 MORE 数值词条，跳过）。多珠宝半径重叠时同节点
/// 后写覆盖单一效果（vendor `localNotableIncEffect = mod.value` 语义）。
pub(crate) fn radius_jewel_notable_effect_copies(
    build: &Build,
    data: &BuildData,
    passive_nodes: &[AllocatedNode],
) -> Result<Vec<Modifier>, BuildError> {
    let mut node_effect: std::collections::BTreeMap<u32, u32> = Default::default();
    for exp in radius_jewel_expansions(build, data) {
        if exp.jewel.notable_effect_inc == 0 {
            continue;
        }
        for &n in &exp.notable_nodes {
            node_effect.insert(n, exp.jewel.notable_effect_inc);
        }
    }
    if node_effect.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<Modifier> = Vec::new();
    for node in passive_nodes {
        let Some(&inc) = node_effect.get(&node.node_id.0) else {
            continue;
        };
        let Some(def) = data.passive_nodes.get(&node.node_id.0) else {
            continue;
        };
        // vendor 缩放条件（CalcSetup.lua:269）：Notable 且非属性且非飞升。
        if def.kind != pobr_data::catalog::PassiveNodeKind::Notable
            || def.ascendancy_id.is_some()
            || is_attribute_node(def)
        {
            continue;
        }
        let scale = 1.0 + f64::from(inc) / 100.0;
        let ingest = pobr_core::passive::ingest_passive_nodes_with_ctx(
            std::slice::from_ref(node),
            engine_ctx(data),
        )
        .map_err(|e| BuildError::Parse(e.to_string()))?;
        out.extend(
            ingest
                .modifiers
                .into_iter()
                .filter(|m| matches!(m.mod_type, ModType::Base | ModType::Inc))
                .filter_map(|m| match m.value {
                    pobr_core::ModValue::Number(v) => {
                        let delta = vendor_scale_mod_value(v, scale) - v;
                        (delta != 0.0).then_some(Modifier {
                            value: pobr_core::ModValue::Number(delta),
                            ..m
                        })
                    }
                    _ => None,
                }),
        );
    }
    Ok(out)
}

/// 可解析性闸门单点（B3 杀双 parser）：判定 = vendor `list and not extra`
/// （PassiveTree.lua:447 `if not list or extra` 触发折行重试）——即
/// `status == Parsed && unparsed == None`。ingest 侧同走 engine，闸门与 ingest
/// 同一 parser；新词条缺口一律修数据表（mod_parser_rules.json / overlay）。
/// engine 未注入（旧数据包）时无解析器：一切按 Unsupported → 全部丢弃。
pub(crate) fn gate_parses(ctx: ParseCtx<'_>, t: &str) -> bool {
    // 诊断：POBR_GATE_DENY=子串1,子串2 强制丢弃匹配行（parity 二分定位用）。
    if let Ok(deny) = std::env::var("POBR_GATE_DENY")
        && deny.split(',').any(|p| !p.is_empty() && t.contains(p))
    {
        return false;
    }
    let pass = ctx.parse(t).is_ok_and(|o| {
        matches!(o.status, pobr_core::mod_parser::ParseStatus::Parsed) && o.unparsed.is_none()
    });
    // 诊断：POBR_DBG_GATE=1 时 dump 被闸门丢弃的行。
    if !pass && std::env::var("POBR_DBG_GATE").is_ok() {
        eprintln!("[GATE_DROP] {t}");
    }
    pass
}

/// 保留通过 [`gate_parses`] 闸门的词条文本，丢弃解析失败/残留的词条。
///
/// 部分真实词条形式无法贡献 modifier（legacy 硬 `ParseError` / engine 残留
/// unparsed）；此处遵循 PoB 的 skip-and-collect 语义在入口侧过滤，使端到端计算
/// 对真实数据健壮（被丢弃的文本不报错，亦不臆造数值）。
pub(crate) fn filter_parseable(texts: Vec<String>, ctx: ParseCtx<'_>) -> Vec<String> {
    texts
        .into_iter()
        .filter(|text| {
            let ok = gate_parses(ctx, text);
            // 诊断：POBR_DBG_DROPPED=1 时 dump 被结构性丢弃的词条（parity 排查用）。
            if !ok && std::env::var("POBR_DBG_DROPPED").is_ok() {
                eprintln!("[POBR_DROP] {text}");
            }
            ok
        })
        .collect()
}

/// 对一件装备的三段词条（implicit / explicit / enchant）各自过滤为可解析子集，
/// 保留段落归属（[`CalculationSession::add_item`] 按段分配来源类别归因）。
pub(crate) fn filter_item_parseable(item: &Item, ctx: ParseCtx<'_>) -> Item {
    let mut filtered = item.clone();
    filtered.implicit_texts = filter_parseable(filtered.implicit_texts, ctx);
    filtered.modifier_texts = filter_parseable(filtered.modifier_texts, ctx);
    filtered.enchant_texts = filter_parseable(filtered.enchant_texts, ctx);
    filtered
}

/// 把已启用技能宝石组解析为带分类（active/support）的 [`GemModSource`]。
///
/// 当前数据管线尚未导出宝石→词条 stat set（见模块文档），故 `modifier_texts` 为空：
/// 宝石只完成 source-level 归因注册（active 归 `SkillGem` / support 归 `SupportGem`，
/// 并把 support 关联到同组首个 active 宝石的 parent source），自身暂不贡献 modifier。
/// 未知 gem id（不在 [`BuildData`] 宝石表）按 active 处理（保守，不臆造辅助语义）。
pub(crate) fn resolve_gems(build: &Build, data: &BuildData) -> Vec<GemModSource> {
    let mut gems = Vec::new();
    for group in build.enabled_socket_groups() {
        // 组内首个 active 宝石作为 support 的被支援目标（PoB Gem 列表顺序：active 在前）。
        let active_gem_id = group
            .gem_ids
            .iter()
            .find(|id| data.is_support_gem(id) != Some(true))
            .cloned();

        for gem_id in &group.gem_ids {
            let is_support = data.is_support_gem(gem_id).unwrap_or(false);
            if is_support {
                let mut src = GemModSource::support(gem_id.clone(), Vec::<String>::new());
                if let Some(active) = &active_gem_id
                    && active != gem_id
                {
                    src = src.supporting(active.clone());
                }
                gems.push(src);
            } else {
                gems.push(GemModSource::active(gem_id.clone(), Vec::<String>::new()));
            }
        }
    }
    gems
}

/// 收集所有已装备物品的词条文本（按确定性槽位顺序）。供 text-only 路径使用。
pub(crate) fn collect_item_texts(build: &Build) -> Vec<String> {
    let mut texts = Vec::new();
    for (_slot, item) in build.equipped_items() {
        texts.extend(item.enchant_texts.iter().cloned());
        texts.extend(item.implicit_texts.iter().cloned());
        texts.extend(item.modifier_texts.iter().cloned());
    }
    texts
}
