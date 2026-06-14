//! `overlay/mod_parser_rules.json` loader——ModParser 解析规则六表（special
//! 除外）+ §1.7 小查找表，schema 见 [`pobr_data::catalog::parser_rules`]
//! （M6 前置，蓝图 m6-parser-rules.md §1）。
//!
//! 数据来源：vendor PoB2 `Modules/ModParser.lua`，由
//! `sync-pob-catalog extract-lua --what parser-rules` headless 引导抽取
//! （schema 标识 `mod_parser_rules/v1`）。消费侧 = M6-B 数据驱动 scan 引擎
//! （`CompiledParserRules::compile` 在 pobr-core，本 loader 零语义、零编译，
//! 保 P9 边界：gamedata 只 load）。`RuleSet.parser_rules` 的填实属 M6-T8
//! 接线范围，本 loader 先随数据落地。

use pobr_data::catalog::parser_rules::ModParserRulesDoc;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载 ModParser 解析规则表（恒走 `overlay/` 定位）。文件缺失（旧数据包
    /// 无此 overlay 域）返回 `Ok(None)`——消费侧行为 = 数据引擎不可用（双跑
    /// 框架只能走 Legacy，向后兼容）；其余 IO / 解析错误照常上抛，不静默。
    pub fn mod_parser_rules(&self) -> Result<Option<ModParserRulesDoc>, LoadError> {
        match self.load_json_at::<ModParserRulesDoc>(self.overlay_path("mod_parser_rules.json")) {
            Ok(doc) => Ok(Some(doc)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use pobr_data::catalog::stat_map::StatMapValue;

    use crate::{GameData, repo_data_root};

    fn real_data() -> GameData {
        GameData::new(repo_data_root().join("4.5.0.3.4"))
    }

    /// 仓库真实数据：各段条目数与钉定 vendor commit 的实测值一致
    /// （蓝图 §1.9 计数自检的消费侧镜像；91/775/202/219/682 + 小查表）。
    #[test]
    fn real_data_section_counts() {
        let doc = real_data()
            .mod_parser_rules()
            .expect("加载 mod_parser_rules.json 不应失败")
            .expect("仓库数据包应含 mod_parser_rules 域");
        assert_eq!(doc.forms.len(), 91, "forms");
        assert_eq!(doc.name_map.len(), 775, "name_map");
        assert_eq!(doc.flag_phrases.len(), 202, "flag_phrases");
        assert_eq!(doc.pre_flags.len(), 219, "pre_flags");
        assert_eq!(doc.tag_phrases.len(), 682, "tag_phrases");
        assert_eq!(doc.suffix_types.len(), 40, "suffix_types");
        assert_eq!(doc.damage_types.len(), 5, "damage_types");
        assert_eq!(doc.pen_types.len(), 6, "pen_types");
        assert_eq!(doc.regen_types.len(), 32, "regen_types");
        assert_eq!(doc.degen_types.len(), 32, "degen_types");
        assert_eq!(doc.cost_types_map.len(), 32, "cost_types_map");
        assert_eq!(doc.base_cost_types.len(), 32, "base_cost_types");
        // 25 = vendor 24 + pobr `hindered`→`Condition:Hindered`（M6-conv2：legacy
        // `parse_enemy_inner` 对 `are hindered` 的特例搬迁，使 `Enemies in your
        // Presence are Hindered` 走 EnemyModifier 包装收敛，见 m6-dualrun-report §2.5）。
        assert_eq!(doc.flag_types.len(), 25, "flag_types");
        assert_eq!(doc.unsupported, vec!["mirrored"], "unsupported");
        assert_eq!(
            doc.unsupported_pobr_extra,
            vec!["split"],
            "unsupported_pobr_extra"
        );
        // form id 集 28 种（蓝图 §1.1）
        let forms: std::collections::BTreeSet<&str> =
            doc.forms.iter().map(|f| f.form.as_str()).collect();
        assert_eq!(forms.len(), 28, "distinct form ids");
        for id in [
            "INC", "RED", "MORE", "LESS", "BASE", "PEN", "DMG", "DOUBLED",
        ] {
            assert!(forms.contains(id), "form id 集应含 {id}");
        }
    }

    /// 抽样钉值：逐字段对照 vendor ModParser.lua 原文（搬迁不变式抽检）。
    #[test]
    fn real_data_sample_pins() {
        let doc = real_data().mod_parser_rules().unwrap().unwrap();

        // formList：`["^(%d+)%% increased"] = "INC"`（ModParser.lua:63）
        let inc = doc
            .forms
            .iter()
            .find(|f| f.pattern == "^(%d+)%% increased")
            .expect("INC form 应存在");
        assert_eq!(inc.form, "INC");
        assert_eq!(inc.literal.as_deref(), Some("% increased"));
        assert!(inc.anchored);

        // modNameList：`["attributes"]` vendor `{ "Str", "Dex", "Int", "All" }`（:161），
        // 经 M6.3 路线 B 抽取期归一展开为 PoBR 子名（聚合短语展开，去 vendor `All`）。
        let attributes = doc
            .name_map
            .iter()
            .find(|e| e.phrase == "attributes")
            .expect("attributes 应存在");
        assert_eq!(
            attributes.names,
            vec!["Strength", "Dexterity", "Intelligence"]
        );

        // modNameList 带 tag：`["mana cost of attacks"]`（SkillType.Attack 反查名）
        let mana_cost = doc
            .name_map
            .iter()
            .find(|e| e.phrase == "mana cost of attacks")
            .expect("mana cost of attacks 应存在");
        assert_eq!(mana_cost.names, vec!["ManaCost"]);
        assert_eq!(mana_cost.effects.tags.len(), 1);
        assert_eq!(mana_cost.effects.tags[0].tag_type, "SkillType");
        assert_eq!(
            mana_cost.effects.tags[0].fields.get("skill_type"),
            Some(&StatMapValue::Text("Attack".into()))
        );

        // modFlagList：`["with maces"] = { flags = bor(ModFlag.Mace, ModFlag.Hit) }`
        // （掩码分解为 bit 升序名数组）
        let with_maces = doc
            .flag_phrases
            .iter()
            .find(|e| e.phrase == "with maces")
            .expect("with maces 应存在");
        assert_eq!(with_maces.effects.flags, vec!["Hit", "Mace"]);

        // preFlagList：`["^minions [cthd][ae][ukva][sel]e? "] = { addToMinion = true }`
        let minions = doc
            .pre_flags
            .iter()
            .find(|e| e.pattern == "^minions [cthd][ae][ukva][sel]e? ")
            .expect("minions pre-flag 应存在");
        assert!(minions.effects.add_to_minion);
        assert_eq!(minions.literal.as_deref(), Some("minions "));
        assert!(minions.anchored);

        // modTagList 纯表条目：`["per power charge"]`
        let per_power = doc
            .tag_phrases
            .iter()
            .find(|e| e.pattern == "per power charge")
            .expect("per power charge 应存在");
        assert_eq!(per_power.effects.tags[0].tag_type, "Multiplier");
        assert_eq!(
            per_power.effects.tags[0].fields.get("var"),
            Some(&StatMapValue::Text("PowerCharge".into()))
        );
        assert!(!per_power.inferred);

        // modTagList 闭包条目（探针推断模板）：`["per (%d+) rage"] = function(num)
        // return { tag = { type = "Multiplier", var = "Rage", div = num } } end`
        let per_rage = doc
            .tag_phrases
            .iter()
            .find(|e| e.pattern == "per (%d+) rage")
            .expect("per (%d+) rage 应存在");
        assert!(per_rage.inferred);
        assert_eq!(
            per_rage.effects.tags[0].fields.get("div"),
            Some(&StatMapValue::Text("$1".into()))
        );
        assert_eq!(
            per_rage.effects.tags[0].fields.get("var"),
            Some(&StatMapValue::Text("Rage".into()))
        );

        // 字符串拼接模板：`per (%d+)%% (%a+) effect on enemy` 的 var =
        // firstToUpper(effectName) .. "Effect" → "$2:cap+Effect"
        let effect = doc
            .tag_phrases
            .iter()
            .find(|e| e.pattern == "per (%d+)%% (%a+) effect on enemy")
            .expect("effect on enemy 应存在");
        assert_eq!(
            effect.effects.tags[0].fields.get("var"),
            Some(&StatMapValue::Text("$2:cap+Effect".into()))
        );

        // 推断失败兜底：`per (%d+) rampage kills`（limit = 1000/num 超出 DSL）
        let rampage = doc
            .tag_phrases
            .iter()
            .find(|e| e.pattern == "per (%d+) rampage kills")
            .expect("rampage kills 应存在");
        assert!(
            rampage
                .handler_id
                .as_deref()
                .is_some_and(|id| id.starts_with("tag_phrase:"))
        );
        assert!(rampage.effects.tags.is_empty());

        // flagTypes hexproof 特例（内嵌 mod 形态）
        let hexproof = doc
            .flag_types
            .iter()
            .find(|e| e.phrase == "hexproof")
            .expect("hexproof 应存在");
        let mod_def = hexproof.mod_def.as_ref().expect("hexproof 应为 mod 形态");
        assert_eq!(mod_def.name, "CurseEffectOnSelf");
        assert_eq!(mod_def.mod_type, "MORE");
        assert_eq!(mod_def.value, -100.0);

        // regen 派生表（vendor 加载期 appendMod 展开）：多资源名集
        let life_mana = doc
            .regen_types
            .iter()
            .find(|e| e.phrase == "life and mana")
            .expect("life and mana 应存在");
        assert_eq!(life_mana.names, vec!["LifeRegen", "ManaRegen"]);
        // maximum 变体由 vendor 加载期补入（蓝图 §1.7）
        assert!(
            doc.regen_types.iter().any(|e| e.phrase == "maximum life"),
            "regen_types 应含 maximum 变体"
        );
    }

    /// 文件缺失（旧数据包）→ `Ok(None)` 向后兼容。
    #[test]
    fn missing_file_returns_none() {
        let dir = std::env::temp_dir().join(format!(
            "pobr-gamedata-parser-rules-missing-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let loaded = GameData::new(&dir).mod_parser_rules().unwrap();
        assert!(loaded.is_none());
    }

    /// 探针推断条目的 handler 兜底数量在蓝图预算内（≤15；当前实测 3）。
    #[test]
    fn handler_fallback_within_budget() {
        let doc = real_data().mod_parser_rules().unwrap().unwrap();
        let handlers = doc
            .pre_flags
            .iter()
            .filter(|e| e.handler_id.is_some())
            .count()
            + doc
                .tag_phrases
                .iter()
                .filter(|e| e.handler_id.is_some())
                .count();
        assert!(handlers <= 15, "handler 兜底 {handlers} 超出蓝图预算 ≤15");
        assert_eq!(handlers, 3, "钉定 vendor commit 下实测 3 条 handler 兜底");
    }
}
