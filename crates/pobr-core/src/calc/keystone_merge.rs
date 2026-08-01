//! 词条授予 keystone 的合并。
//!
//! 对照 PoB2 `CalcPerform.lua:66-76 mergeKeystones`：Tabulate player db 的
//! `Keystone` LIST mod（装备/珠宝词条「You have \<Keystone\>」/ 裸 keystone 名行，
//! 由 mod_parser 产出 `Modifier{name:"Keystone", List, Text(名)}`）→ 未注入过的查
//! [`Env::keystone_mods`]（pobr-build 从天赋树 keystone 节点构造、经
//! `session.set_keystone_mods` 注入）把该 keystone 的 mods 写进玩家 modDB，归因
//! SourceId = (`GrantedKeystone`, `"keystone.<名>"`)。
//!
//! 去重语义（等价 PoB2 `env.keystonesAdded`）：
//! - **阶段间幂等**（env_finalize 阶段 1 与 5 各跑一次）：注入产物携带
//!   `GrantedKeystone` SourceId，本函数注入前扫 player db 同 SourceId 即跳过——
//!   不需要 Env 携带额外状态字段。
//! - **树已点同名不重复**：PoB2 树上已点 keystone 也只发 `Keystone` LIST mod、
//!   经同一 merge 注入一次；pobr 的树路径（passive ingest）直接注入节点 mods，
//!   故由 pobr-build 在构造 keystone_mods map 时**排除已点 keystone**（map 缺键
//!   ＝ 此处静默跳过，等价 PoB2 `keystoneMap[name]` nil 检查分支）。
//! - 同名 LIST mod 多条（多件装备授予同一 keystone）：本次调用内 HashSet 去重。
//!
//! 与`rules/keystone_registry.rs` 的关系：本模块只负责「词条→keystone 的
//! mod 注入」；CI/EB 等机制开关仍由 keystone_registry 读 flag 裁决（注入的 mods
//! 里含对应 flag 即自动接通）。
//!
//! 空转兼容（D1 搬迁不变式锚点）：无 `Keystone` LIST 词条 / `keystone_mods` 空
//! map 时 player db 零写入，输出逐值不变。

use std::collections::HashSet;

use pobr_data::prelude::*;

use super::Env;

/// `Keystone` LIST 通道的 ModName（PoB2 `Tabulate("LIST", nil, "Keystone")`）。
const KEYSTONE_LIST_NAME: &str = "Keystone";

/// keystone 注入归因 SourceId（`GrantedKeystone, "keystone.<name>"`）。
fn keystone_source_id(name: &str) -> SourceId {
    SourceId::new(SourceKind::GrantedKeystone, format!("keystone.{name}"))
}

/// env_finalize 阶段 1/5 的实现体（对照 CalcPerform.lua:66-76）。
pub fn merge_keystones(env: &mut Env) {
    if env.keystone_mods.is_empty() {
        return;
    }

    let granted = env
        .player
        .mod_db
        .list(&env.cfg, ModName::from(KEYSTONE_LIST_NAME));
    if granted.is_empty() {
        return;
    }

    // 已注入集合（阶段间幂等）：player db 中已有 GrantedKeystone 归因的 keystone 名。
    let mut added: HashSet<String> = env
        .player
        .mod_db
        .iter_mods()
        .filter_map(|m| m.origin.as_ref())
        .filter(|origin| origin.source_id.kind == SourceKind::GrantedKeystone)
        .filter_map(|origin| origin.source_id.id.strip_prefix("keystone."))
        .map(str::to_string)
        .collect();

    let mut to_inject: Vec<crate::Modifier> = Vec::new();
    for name in granted {
        // 等价 `if not env.keystonesAdded[modObj.value]`；本次调用内同名多条也去重。
        if !added.insert(name.clone()) {
            continue;
        }
        // 等价 `env.spec.tree.keystoneMap[modObj.value]` nil 检查：map 缺键（含
        // pobr-build 已排除的「树已点」keystone）静默跳过。
        let Some(mods) = env.keystone_mods.get(&name) else {
            continue;
        };
        let source_id = keystone_source_id(&name);
        for modifier in mods {
            // 归因统一覆写为 GrantedKeystone（raw_text 保留 map 构造侧已带的原词条行）。
            let raw_text = modifier
                .origin
                .as_ref()
                .and_then(|origin| origin.raw_text.clone())
                .or_else(|| modifier.source.clone());
            let mut origin = ModifierSource::new(source_id.clone());
            origin.raw_text = raw_text;
            to_inject.push(modifier.clone().with_origin(origin));
        }
    }

    for modifier in to_inject {
        env.player.mod_db.add_mod(modifier);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::env_finalize;
    use crate::calc::{Actor, ActorBaseStats};
    use crate::{ModValue, Modifier};
    use std::collections::BTreeMap;

    fn env_with_grant(keystone: &str) -> Env {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        env.player.mod_db.add_mod(Modifier::new(
            KEYSTONE_LIST_NAME,
            ModType::List,
            ModValue::Text(keystone.into()),
        ));
        env
    }

    fn iron_reflexes_map() -> BTreeMap<String, Vec<Modifier>> {
        BTreeMap::from([(
            "Iron Reflexes".to_string(),
            vec![
                Modifier::flag("Keystone:IronReflexes"),
                Modifier::number("Armour", ModType::Inc, 20.0),
            ],
        )])
    }

    /// 装备词条授予 keystone → 其 mods 注入一次，归因 GrantedKeystone。
    #[test]
    fn grants_keystone_mods_once_with_granted_keystone_source() {
        let mut env = env_with_grant("Iron Reflexes");
        env.keystone_mods = iron_reflexes_map();

        merge_keystones(&mut env);

        let injected: Vec<_> = env
            .player
            .mod_db
            .iter_mods()
            .filter(|m| {
                m.origin
                    .as_ref()
                    .is_some_and(|o| o.source_id.kind == SourceKind::GrantedKeystone)
            })
            .collect();
        assert_eq!(injected.len(), 2, "keystone 的 2 条 mod 各注入一次");
        for m in &injected {
            assert_eq!(
                m.origin.as_ref().unwrap().source_id.id,
                "keystone.Iron Reflexes"
            );
        }
        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("Armour")],),
            20.0
        );
    }

    /// 阶段 1 与阶段 5 各跑一次（PoB2 keystonesAdded 幂等）→ 不重复注入；
    /// 同名 LIST mod 多条同样去重。
    #[test]
    fn second_pass_and_duplicate_grants_are_idempotent() {
        let mut env = env_with_grant("Iron Reflexes");
        // 第二件装备授予同一 keystone。
        env.player.mod_db.add_mod(Modifier::new(
            KEYSTONE_LIST_NAME,
            ModType::List,
            ModValue::Text("Iron Reflexes".into()),
        ));
        env.keystone_mods = iron_reflexes_map();

        merge_keystones(&mut env); // 阶段 1
        merge_keystones(&mut env); // 阶段 5

        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("Armour")],),
            20.0,
            "幂等：重复授予/二次合并不得叠加"
        );
    }

    /// map 缺键（pobr-build 已排除「树已点同名」keystone）→ 静默跳过，零写入
    /// （等价 PoB2 keystoneMap[name] nil 分支）。
    #[test]
    fn missing_map_entry_is_silently_skipped() {
        let mut env = env_with_grant("Iron Reflexes");
        env.keystone_mods = BTreeMap::from([(
            "Acrobatics".to_string(),
            vec![Modifier::flag("Keystone:Acrobatics")],
        )]);
        let before = env.player.mod_db.iter_mods().count();

        merge_keystones(&mut env);

        assert_eq!(env.player.mod_db.iter_mods().count(), before);
    }

    /// 空 map → 逐值不变（D1 空转兼容锚点，经 env_finalize 全调度验证）。
    #[test]
    fn empty_map_keeps_db_unchanged_through_env_finalize() {
        let mut env = env_with_grant("Iron Reflexes");
        let before = env.player.mod_db.iter_mods().count();

        env_finalize::env_finalize(&mut env);

        assert_eq!(env.player.mod_db.iter_mods().count(), before);
    }
}
