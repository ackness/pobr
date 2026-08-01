//! Build 的只读计算快照与内容哈希。
//!
//! [`BuildSnapshot`] 把一个 [`Build`] 中**影响计算结果**的输入收敛成一份确定性、
//! 可哈希的视图。[`BuildSnapshot::content_hash`] 给出稳定的 64 位内容哈希，用作
//! [`crate::calc_cache::CalcCache`] 的键：只要计算相关输入不变，哈希就不变，可命中缓存。
//!
//! 哈希通过把规范化字段写入一个稳定字符串再做 FNV-1a 实现（无第三方哈希依赖，
//! 跨平台 / 跨进程一致；不依赖 `HashMap` 的随机化遍历顺序）。

use pobr_data::item::EquipmentSlot;

use crate::build::Build;

/// 计算输入的只读快照。字段顺序固定，遍历确定性，可安全用于内容哈希。
///
/// **范围限定（勿误用）**：本快照只覆盖 **text-only `calculate`** 的输入
/// （等级 / 职业 / 已分配节点 / 物品词条文本 / socket 组 gem id / config 键）。
/// 它**不足以**作为 [`crate::calc_orchestrator::calculate_with_data`] 的缓存键——
/// 后者还消费 gem 等级 / 品质、珠宝半径效果、运行时数据等本快照未捕获的输入。
/// `calculate_with_data` 的缓存需要在 `(build_hash, options, data...)` 维度上自建键，
/// 切勿直接拿 [`content_hash`](BuildSnapshot::content_hash) 当其缓存键。
#[derive(Debug, Clone, PartialEq)]
pub struct BuildSnapshot {
    pub level: u32,
    pub class_name: String,
    pub ascendancy_name: String,
    pub allocated_nodes: Vec<u32>,
    /// 已装备物品的 (槽位 id, 规范化词条文本)，按槽位 id 字典序排序。
    pub items: Vec<(String, Vec<String>)>,
    /// 已启用技能宝石组的 gem id 序列，按组保持原序。
    pub socket_groups: Vec<Vec<String>>,
    /// Build 配置的规范化键值（攻击 / 法术 / 条件 / 乘子），稳定排序。
    pub config_keys: Vec<String>,
}

impl BuildSnapshot {
    /// 从 [`Build`] 抽取计算相关输入，规范化为确定性快照。
    pub fn from_build(build: &Build) -> Self {
        let mut allocated_nodes: Vec<u32> =
            build.tree.allocated_nodes.iter().map(|n| n.0).collect();
        allocated_nodes.sort_unstable();

        let items = collect_items(build);
        let socket_groups = build
            .enabled_socket_groups()
            .map(|g| g.gem_ids.clone())
            .collect();
        let config_keys = collect_config_keys(build);

        Self {
            level: build.character.level,
            class_name: build.character.class_name.clone(),
            ascendancy_name: build.character.ascendancy_name.clone(),
            allocated_nodes,
            items,
            socket_groups,
            config_keys,
        }
    }

    /// 64 位确定性内容哈希（FNV-1a，跨平台稳定）。
    ///
    /// 仅覆盖 text-only `calculate` 的输入；作 [`crate::calc_cache::CalcCache`] 键时
    /// 必须再叠加 [`OrchestratorOptions`](crate::calc_orchestrator::OrchestratorOptions)
    /// 的哈希（见 `calc_cache`）。**不要**单独拿它当 `calculate_with_data` 的缓存键
    /// （gem 等级/品质、珠宝等未捕获）——详见 [`BuildSnapshot`] 上的范围限定说明。
    pub fn content_hash(&self) -> u64 {
        let mut hasher = Fnv1a::new();
        hasher.write_u32(self.level);
        hasher.write_str(&self.class_name);
        hasher.write_str(&self.ascendancy_name);

        hasher.write_str("|nodes|");
        for node in &self.allocated_nodes {
            hasher.write_u32(*node);
        }

        hasher.write_str("|items|");
        for (slot, texts) in &self.items {
            hasher.write_str(slot);
            for text in texts {
                hasher.write_str(text);
            }
        }

        hasher.write_str("|skills|");
        for group in &self.socket_groups {
            hasher.write_str("(");
            for gem in group {
                hasher.write_str(gem);
            }
            hasher.write_str(")");
        }

        hasher.write_str("|config|");
        for key in &self.config_keys {
            hasher.write_str(key);
        }

        hasher.finish()
    }
}

fn collect_items(build: &Build) -> Vec<(String, Vec<String>)> {
    build
        .equipped_items()
        .into_iter()
        .map(|(slot, item)| {
            let mut texts: Vec<String> = Vec::new();
            texts.extend(item.enchant_texts.iter().cloned());
            texts.extend(item.implicit_texts.iter().cloned());
            texts.extend(item.modifier_texts.iter().cloned());
            (slot_tag(slot).to_string(), texts)
        })
        .collect()
}

fn collect_config_keys(build: &Build) -> Vec<String> {
    let cfg = &build.config;
    let mut keys = vec![
        format!("attack={}", cfg.is_attack),
        format!("spell={}", cfg.is_spell),
        format!("dt={:?}", cfg.damage_type),
        format!("bandit={:?}", cfg.bandit),
    ];
    let mut conds: Vec<String> = cfg
        .conditions
        .iter()
        .map(|(k, v)| format!("c:{k}={v}"))
        .collect();
    conds.sort();
    keys.extend(conds);
    let mut mults: Vec<String> = cfg
        .multipliers
        .iter()
        .map(|(k, v)| format!("m:{k}={}", v.to_bits()))
        .collect();
    mults.sort();
    keys.extend(mults);
    // 原始 config 输入（主路径数据源）：解释器消费 raw_inputs 产出
    // conditions/multipliers 之外的行为（enemy 覆盖 / customMods 等逐类打开），
    // 须进内容哈希避免缓存别名。BTreeMap 遍历本身确定序。
    keys.extend(
        cfg.raw_inputs
            .values
            .iter()
            .map(|(k, v)| format!("r:{k}={v:?}")),
    );
    keys
}

fn slot_tag(slot: EquipmentSlot) -> &'static str {
    slot.id()
}

/// 64 位 FNV-1a 哈希器（确定性，无随机种子）。
struct Fnv1a {
    state: u64,
}

impl Fnv1a {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self {
            state: Self::OFFSET,
        }
    }

    fn write_byte(&mut self, byte: u8) {
        self.state ^= byte as u64;
        self.state = self.state.wrapping_mul(Self::PRIME);
    }

    fn write_str(&mut self, s: &str) {
        for b in s.as_bytes() {
            self.write_byte(*b);
        }
        // 长度分隔，避免拼接歧义（"ab"+"c" vs "a"+"bc"）。
        self.write_byte(0xff);
    }

    fn write_u32(&mut self, value: u32) {
        for b in value.to_le_bytes() {
            self.write_byte(b);
        }
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{Build, CharacterIdentity, SocketGroup};
    use pobr_data::item::{Item, ItemBaseId, ItemRarity, RolledDefence};
    use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};

    fn build_with_level(level: u32) -> Build {
        Build::new().with_character(CharacterIdentity {
            level,
            class_name: "Ranger".into(),
            ascendancy_name: "Deadeye".into(),
        })
    }

    #[test]
    fn hash_is_deterministic() {
        let a = BuildSnapshot::from_build(&build_with_level(90));
        let b = BuildSnapshot::from_build(&build_with_level(90));
        assert_eq!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn hash_changes_with_level() {
        let a = BuildSnapshot::from_build(&build_with_level(90)).content_hash();
        let b = BuildSnapshot::from_build(&build_with_level(91)).content_hash();
        assert_ne!(a, b);
    }

    #[test]
    fn hash_insensitive_to_node_order() {
        let mut a = build_with_level(90);
        a = a.with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(3), NodeId(1), NodeId(2)],
            ..Default::default()
        });
        let mut b = build_with_level(90);
        b = b.with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(1), NodeId(2), NodeId(3)],
            ..Default::default()
        });
        assert_eq!(
            BuildSnapshot::from_build(&a).content_hash(),
            BuildSnapshot::from_build(&b).content_hash()
        );
    }

    #[test]
    fn hash_changes_with_item() {
        let base = build_with_level(90);
        let with_item = base.clone().set_item(
            EquipmentSlot::Ring1,
            Item {
                base: ItemBaseId::from("Iron Ring"),
                rarity: ItemRarity::Rare,
                quality: 0,
                corrupted: false,
                implicit_texts: vec![],
                modifier_texts: vec!["+10 to maximum Life".into()],
                enchant_texts: vec![],
                rolled_defence: RolledDefence::default(),
                parsed_stats: vec![],
            },
        );
        assert_ne!(
            BuildSnapshot::from_build(&base).content_hash(),
            BuildSnapshot::from_build(&with_item).content_hash()
        );
    }

    #[test]
    fn enabled_groups_affect_hash() {
        let base = build_with_level(90);
        let with_group = base
            .clone()
            .add_socket_group(SocketGroup::new().with_gem("Spark"));
        assert_ne!(
            BuildSnapshot::from_build(&base).content_hash(),
            BuildSnapshot::from_build(&with_group).content_hash()
        );
    }
}
