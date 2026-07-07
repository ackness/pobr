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
    Ok(())
}

/// 宿主便捷入口：从磁盘版本目录初始化（wasm 下调用会因文件 I/O 失败而报错）。
pub fn init_data_from_dir(version_dir: &str) -> Result<(), String> {
    let data = GameData::new(version_dir);
    let build_data = BuildData::load(&data).map_err(|e| format!("load BuildData: {e}"))?;
    BUILD_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(build_data)));
    GAME_DATA.with_borrow_mut(|slot| *slot = Some(Rc::new(data)));
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
