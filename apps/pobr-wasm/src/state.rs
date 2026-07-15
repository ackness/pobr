//! 游戏数据初始化与会话级缓存。
//!
//! wasm 环境无文件系统，数据文件由 JS 侧 fetch 后经 [`stage_data_file`] 逐个
//! 注入、[`init_staged_data`] 一次性构建 [`BuildData`]（内存后端
//! [`GameData::from_memory`]）。宿主（测试 / CLI）走 [`init_data_from_dir`]
//! 直接指向 `data/<version>/` 目录。两条路径产物一致：thread_local 缓存的
//! `Rc<BuildData>`，供 `build_api` 的计算入口零 I/O 复用。
//!
//! wasm 目标单线程，thread_local 即全局；宿主测试同线程内先 init 再调用即可。

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use pobr_build::BuildData;
use pobr_gamedata::GameData;

thread_local! {
    /// 分批注入的数据文件暂存区（`相对路径 -> bytes`）。
    static STAGED: RefCell<BTreeMap<String, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
    /// 已构建的 BuildData（`None` = 尚未初始化）。
    static BUILD_DATA: RefCell<Option<Rc<BuildData>>> = const { RefCell::new(None) };
    /// 构建 BuildData 的 GameData 本体（i18n 名称边车等按需查询用）。
    static GAME_DATA: RefCell<Option<Rc<GameData>>> = const { RefCell::new(None) };
    /// 中文词条行翻译器（懒构建；`None` 未尝试，`Some(None)` = 数据包无 zh-CN 模板）。
    static ZH_TRANSLATOR: RefCell<Option<Option<Rc<crate::zh::LineTranslator>>>> =
        const { RefCell::new(None) };
    /// 反向（英文 → 简中）显示翻译器：同一份模板对换向构建（树词条 tooltip /
    /// 配置 list 选项等展示面消费）。
    static EN_TO_ZH_TRANSLATOR: RefCell<Option<Option<Rc<crate::zh::LineTranslator>>>> =
        const { RefCell::new(None) };
    /// 词缀 tier 反查索引（懒构建；`Some(None)` = 数据包缺池数据/模板，降级不标）。
    static TIER_INDEX: RefCell<Option<Option<Rc<pobr_item::TierIndex>>>> =
        const { RefCell::new(None) };
    /// 入口级响应缓存（见 [`cached_response`]）。
    static RESPONSE_CACHE: RefCell<ResponseCache> = RefCell::new(ResponseCache::default());
}

/// 入口级 `(端点, 请求 JSON 字符串) → 响应字符串` 缓存。
///
/// 键是原始请求字符串：同字符串进必同结果出（计算确定性 + 数据初始化后不变），
/// 请求形状加字段自动改变键——不存在「缓存键漏字段返回陈旧结果」的漂移风险
/// （`BuildSnapshot` 文档明确警告其 content_hash 不足以作 data 路径缓存键，
/// 故不采用）。命中场景：面板切换 / 对比视图来回 / 点击触发的重复
/// full-dps·归因请求。FIFO 逐出，Err 不缓存。
#[derive(Default)]
struct ResponseCache {
    entries: std::collections::HashMap<(&'static str, String), String>,
    order: std::collections::VecDeque<(&'static str, String)>,
    hits: u64,
}

/// 缓存条目数上限。响应可达数百 KB（breakdown 全量），16 条足够覆盖
/// 「对比两个 build 来回切 + 各面板重复请求」且内存有界。
const RESPONSE_CACHE_CAP: usize = 16;

/// 以 `(endpoint, request_json)` 为键缓存 `compute` 的成功结果。
pub(crate) fn cached_response(
    endpoint: &'static str,
    request_json: &str,
    compute: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    let hit = RESPONSE_CACHE.with_borrow_mut(|cache| {
        let got = cache.entries.get(&(endpoint, request_json.to_string()));
        if got.is_some() {
            cache.hits += 1;
        }
        got.cloned()
    });
    if let Some(response) = hit {
        return Ok(response);
    }
    let response = compute()?;
    RESPONSE_CACHE.with_borrow_mut(|cache| {
        if cache.order.len() >= RESPONSE_CACHE_CAP
            && let Some(oldest) = cache.order.pop_front()
        {
            cache.entries.remove(&oldest);
        }
        let key = (endpoint, request_json.to_string());
        if cache
            .entries
            .insert(key.clone(), response.clone())
            .is_none()
        {
            cache.order.push_back(key);
        }
    });
    Ok(response)
}

/// 缓存命中计数（测试断言用）。
pub fn response_cache_hits() -> u64 {
    RESPONSE_CACHE.with_borrow(|c| c.hits)
}

/// 数据（重新）初始化时清空响应缓存——结果对数据版本敏感。
fn clear_response_cache() {
    RESPONSE_CACHE.with_borrow_mut(|c| {
        c.entries.clear();
        c.order.clear();
        c.hits = 0;
    });
}

/// 注入一个数据文件（`path` = 版本目录内相对路径，正斜杠，如 `base/stats.json`）。
///
/// 只暂存不解析；重复 path 后写覆盖。
pub fn stage_data_file(path: &str, content: &str) {
    STAGED.with_borrow_mut(|map| {
        map.insert(path.to_string(), content.as_bytes().to_vec());
    });
}

/// 用暂存区文件构建内存后端 [`GameData`] + [`BuildData`] 并缓存；清空暂存区。
pub fn init_staged_data() -> Result<(), String> {
    let files = STAGED.with_borrow_mut(std::mem::take);
    if files.is_empty() {
        return Err("no data files staged; call stage_data_file first".to_string());
    }
    let data = GameData::from_memory(files);
    let build_data = BuildData::load(&data).map_err(|e| format!("load BuildData: {e}"))?;
    BUILD_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(build_data)));
    GAME_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(data)));
    clear_response_cache();
    Ok(())
}

/// 宿主便捷入口：从磁盘版本目录初始化（wasm 下调用会因文件 I/O 失败而报错）。
pub fn init_data_from_dir(version_dir: &str) -> Result<(), String> {
    let data = GameData::new(version_dir);
    let build_data = BuildData::load(&data).map_err(|e| format!("load BuildData: {e}"))?;
    BUILD_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(build_data)));
    GAME_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(data)));
    clear_response_cache();
    Ok(())
}

/// 数据是否已初始化。
pub fn is_data_ready() -> bool {
    BUILD_DATA.with_borrow(|slot| slot.is_some())
}

/// 取已初始化的 BuildData；未初始化时报出可透传给前端的错误消息。
pub fn build_data() -> Result<Rc<BuildData>, String> {
    BUILD_DATA.with_borrow(|slot| {
        slot.clone()
            .ok_or_else(|| "game data not initialized; call init first".to_string())
    })
}

/// 取构建时的 GameData（i18n 名称边车查询）；未初始化时报错同上。
pub fn game_data() -> Result<Rc<GameData>, String> {
    GAME_DATA.with_borrow(|slot| {
        slot.clone()
            .ok_or_else(|| "game data not initialized; call init first".to_string())
    })
}

/// 取中文词条行翻译器（首次调用构建并缓存；数据包无 zh-CN 模板时为 `None`，
/// 消费侧按「无输入翻译」降级——中文行原样进 parser 落 unsupported）。
pub fn zh_translator() -> Option<Rc<crate::zh::LineTranslator>> {
    ZH_TRANSLATOR.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = Some(build_zh_translator());
        }
        slot.as_ref().and_then(Clone::clone)
    })
}

/// 取英文 → 简中显示翻译器（懒构建同上）。
pub fn en_to_zh_translator() -> Option<Rc<crate::zh::LineTranslator>> {
    EN_TO_ZH_TRANSLATOR.with_borrow_mut(|slot| {
        if slot.is_none() {
            let built = (|| {
                let game = game_data().ok()?;
                let templates = game.stat_line_templates("zh-CN").ok().flatten()?;
                // 换向：src=英文模板、en 字段放简中模板 → translate_line 即 en→zh。
                let swapped: Vec<pobr_data::catalog::StatLineTemplate> = templates
                    .into_iter()
                    .map(|t| pobr_data::catalog::StatLineTemplate {
                        src: t.en,
                        en: t.src,
                    })
                    .collect();
                // 名词直译表（英文 → 简中）：基底名（i18n id→zh × base 英名→id join）
                // + Words 名词（唯一物品名等）。物品名/基底行靠这张表整行命中。
                let mut names = std::collections::HashMap::new();
                if let (Ok(zh_by_id), Ok(data)) = (game.base_item_names("zh-CN"), build_data()) {
                    for (en_name, def) in &data.base_items {
                        if let Some(zh) = zh_by_id.get(def.id.as_str()) {
                            names.insert(en_name.clone(), zh.clone());
                        }
                    }
                }
                if let Ok(words) = game.word_names("zh-CN") {
                    names.extend(words);
                }
                if let Ok(passives) = game.passive_node_names("zh-CN") {
                    names.extend(passives);
                }
                let mut translator = crate::zh::LineTranslator::new(&swapped, names);
                // 词缀名表（Mods 表）：启用魔法物品名「后缀+前缀+基底」组合翻译。
                if let Ok(affixes) = game.affix_names("zh-CN") {
                    translator.set_affix_names(affixes.into_iter().collect());
                }
                // RARE 随机名组成词表：启用双词连写组合翻译。
                if let Ok(words) = game.rare_name_words("zh-CN") {
                    translator.set_rare_words(words.into_iter().collect());
                }
                Some(Rc::new(translator))
            })();
            *slot = Some(built);
        }
        slot.as_ref().and_then(Clone::clone)
    })
}

/// 取词缀 tier 反查索引（首次调用构建并缓存；mods 缺 group/spawn_weights 或
/// 无 StatDescriptions overlay 的旧数据包返回 `None`——消费侧不标 tier）。
pub fn tier_index() -> Option<Rc<pobr_item::TierIndex>> {
    TIER_INDEX.with_borrow_mut(|slot| {
        if slot.is_none() {
            let built = (|| {
                let game = game_data().ok()?;
                let mods = game.mods().ok()?;
                let descriptions = game.stat_descriptions().ok().flatten()?;
                let index = pobr_item::TierIndex::build(&mods, &descriptions);
                (!index.is_empty()).then(|| Rc::new(index))
            })();
            *slot = Some(built);
        }
        slot.as_ref().and_then(Clone::clone)
    })
}

/// 按英文基底名查 (tags, mod_domain)（tier 反查的适用性输入）。
pub fn base_item_tags(base_name: &str) -> Option<(Vec<String>, u32)> {
    let data = build_data().ok()?;
    let def = data.base_items.get(base_name)?;
    Some((def.tags.clone(), def.mod_domain))
}

fn build_zh_translator() -> Option<Rc<crate::zh::LineTranslator>> {
    let game = game_data().ok()?;
    let templates = game.stat_line_templates("zh-CN").ok().flatten()?;
    // 基底名反查表：i18n（id → 简中名）× base（英文名 → def.id）join。
    let mut base_names = std::collections::HashMap::new();
    if let (Ok(zh_by_id), Ok(data)) = (game.base_item_names("zh-CN"), build_data()) {
        for (en_name, def) in &data.base_items {
            if let Some(zh) = zh_by_id.get(def.id.as_str()) {
                base_names.insert(zh.clone(), en_name.clone());
            }
        }
    }
    // Words 名词反查（简中唯一物品名 → 英文），国服物品文本的名称行走这里。
    if let Ok(words) = game.word_names("zh-CN") {
        for (en, zh) in words {
            base_names.entry(zh).or_insert(en);
        }
    }
    Some(Rc::new(crate::zh::LineTranslator::new(
        &templates, base_names,
    )))
}
