#!/usr/bin/env bash
# 全量重生成 data/<patch>/——把 pipeline 输入（tables/ + tree/data.json）+ vendor Lua
# 适配为完整的 PoBR 入库 JSON（base/ + i18n/ + overlay/ + generated/ + manifest.json）。
#
# 与 devs/scripts/regen-check.sh 的区别：regen-check 是“重生成到临时目录并 byte-diff 校验”
# 的 CI 防线；本脚本是“真正产出/刷新 data/<patch>/”的生产编排。
#
# 前置：
#   1) pipeline/config.json 的 "patch" 已指向目标版本；
#   2) pipeline/tables/{English,Traditional Chinese}/*.json 已由 `npx pathofexile-dat` 产出；
#   3) pipeline/tree/data.json 已下载（GGG poe2-skilltree-export）；
#   4) vendor/PathOfBuilding-PoE2/ 已检出目标版本，vendor/.pob2-version.txt 已更新。
#
# 用法：
#   pipeline/regen-all.sh                       # patch 读 config.json；手工域从同 patch 旧产物沿用
#   OLD_PATCH=4.5.0.3.4 pipeline/regen-all.sh    # 手工域从指定旧版本沿用（跨版本升级）
#
# 手工策展 overlay（无 vendor/官方自动通道）：buff_definitions / high_precision_mods /
# local_mods / special_mods / vendor_name_aliases —— 跨版本升级时从 OLD_PATCH 沿用，
# 并需人工复核（游戏平衡变更可能使其过时）。

# 韧性化：不用 -e 全局中止。前置步骤（adapter base/tree）失败仍立即退出（无 base
# 数据则后续无意义）；overlay 单步失败只记录、续跑其余，末尾汇总。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# 关键前置步骤：失败即致命（用于 adapter base/tree）。
die_on_fail() { "$@" || { echo "regen-all: 关键步骤失败，中止：$*" >&2; exit 1; }; }
# overlay 软步骤：失败记入 OVERLAY_FAILURES、续跑其余。
OVERLAY_FAILURES=()
soft_step() {
    local label="$1"; shift
    if ! "$@"; then
        OVERLAY_FAILURES+=("$label")
        echo "   ⚠ overlay 失败（续跑其余）：$label" >&2
    fi
}

PATCH="$(sed -n 's/.*"patch"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' pipeline/config.json | head -1)"
[[ -n "$PATCH" ]] || { echo "regen-all: 无法从 config.json 读取 patch" >&2; exit 1; }
OLD_PATCH="${OLD_PATCH:-$PATCH}"
# 绝对路径：部分 extract runner 会 cd 到 vendor_root 并由它派生 LUA_PATH；relative
# vendor_root 在 cd 后会让派生的 runtime/lua 路径双重嵌套（找不到 xml/dkjson 等）。
VENDOR="$ROOT/vendor/PathOfBuilding-PoE2/src"
OUT_DIR="data/$PATCH"
OVL="$OUT_DIR/overlay"

echo "regen-all: patch=$PATCH  old=$OLD_PATCH  vendor=$VENDOR"

ADAPTER=(cargo run --quiet -p pobr-data-adapter --)
SYNC=(cargo run --quiet -p sync-pob-catalog --)

# ---- 1) base/ + i18n/（GGG .dat → 物品/词缀/Stat/技能）----
echo "== [1/8] adapter --raw（base/ + i18n/）"
die_on_fail "${ADAPTER[@]}" --raw pipeline/tables --out data --patch "$PATCH"

# ---- 2) 被动天赋树（GGG 官方 data.json）----
echo "== [2/8] adapter --tree"
die_on_fail "${ADAPTER[@]}" --tree pipeline/tree/data.json --out data --patch "$PATCH"

echo "== [3/8] adapter --tree-variants / --tree-coords / --tree-anoints（vendor tree.lua 回填）"
die_on_fail "${ADAPTER[@]}" --tree-variants "$VENDOR/TreeData/0_5/tree.lua" --out data --patch "$PATCH"
# 节点平面坐标（web 树渲染依赖 x/y；漏跑则前端树永远空白）。
die_on_fail "${ADAPTER[@]}" --tree-coords "$VENDOR/TreeData/0_5/tree.lua" --out data --patch "$PATCH"
# tree-anoints 在“无缺失 notable 需回填”时硬报错退出（tree_anoints.rs:111）。新版 GGG
# 树导出 data.json 已自带油涂专属 notable（如 Paragon），回填成 no-op 属正常——
# 仅当报错信息是该 no-op 时告警放行；其他错误（真解析失败等）仍致命。
anoint_log="$(mktemp)"
if "${ADAPTER[@]}" --tree-anoints "$VENDOR/TreeData/0_5/tree.lua" --out data --patch "$PATCH" >"$anoint_log" 2>&1; then
    cat "$anoint_log"
elif grep -q "未解析出任何缺失 notable" "$anoint_log"; then
    echo "   tree-anoints: 无缺失 notable 需回填（新树已自带油涂 notable）——跳过"
else
    cat "$anoint_log" >&2
    rm -f "$anoint_log"
    echo "regen-all: tree-anoints 异常退出（非 no-op）" >&2
    exit 1
fi
rm -f "$anoint_log"

# ---- 4) keystone 派生 special 表（输入 = 刚产出的 passive_tree.json）----
echo "== [4/8] adapter --emit-special-derived"
die_on_fail "${ADAPTER[@]}" --emit-special-derived "$OUT_DIR/base/passive_tree.json" --out data --patch "$PATCH"

# ---- 5) overlay（自动通道：sync-pob-catalog extract-lua / gen-* / extract-bases）----
echo "== [5/8] overlay 自动重生成（对新 vendor）"
mkdir -p "$OVL"
# PoB2 0.21.0+ 的 Common.lua 硬 require('lua-utf8')（C 模块），vendor 仅附 Windows .dll。
# 完整 headless 抽取（config-options/parser-rules/skill-overrides）需要它；把跟踪的
# 纯 Lua 垫片装到 PoB 期望的外部库位置（vendor 重新检出后此文件会丢，故每次确保存在）。
VENDOR_RUNTIME_LUA="$(dirname "$VENDOR")/runtime/lua"
if [[ -d "$VENDOR_RUNTIME_LUA" && ! -f "$VENDOR_RUNTIME_LUA/lua-utf8.lua" ]]; then
    cp pipeline/lua-shims/lua-utf8.lua "$VENDOR_RUNTIME_LUA/lua-utf8.lua"
    echo "   装入 lua-utf8 垫片 → $VENDOR_RUNTIME_LUA/lua-utf8.lua"
fi
GEMFILES="act_dex,act_int,act_str,minion,other,spectre,sup_dex,sup_int,sup_str"
SMFILES="act_dex,act_int,act_str,other,sup_dex,sup_int,sup_str"

# --files 缺省 = Data/Bases 全量（tags 全集抽取需要每个可装备基底文件）。
soft_step base_item_overrides "${SYNC[@]}" extract-bases --vendor-root "$VENDOR" --out "$OVL/base_item_overrides.json"
soft_step catalysts       "${SYNC[@]}" extract-lua --what catalysts       --vendor-root "$VENDOR" --out "$OVL/catalysts.json"
soft_step config_options  "${SYNC[@]}" extract-lua --what config-options  --vendor-root "$VENDOR" --out "$OVL/config_options.json"
soft_step curse_priority  "${SYNC[@]}" extract-lua --what curse-priority  --vendor-root "$VENDOR" --out "$OVL/curse_priority.json"
soft_step gem_effects     "${SYNC[@]}" extract-lua --what gem-effects     --vendor-root "$VENDOR" --out "$OVL/gem_effects.json"
soft_step gem_quality_stats "${SYNC[@]}" extract-lua --what gem-quality   --vendor-root "$VENDOR" --files "$GEMFILES" --out "$OVL/gem_quality_stats.json"
soft_step granted_effect_minions "${SYNC[@]}" extract-lua --what minion-list --vendor-root "$VENDOR" --out "$OVL/granted_effect_minions.json"
soft_step minions         "${SYNC[@]}" extract-lua --what minions         --vendor-root "$VENDOR" --out "$OVL/minions.json"
soft_step mod_parser_rules "${SYNC[@]}" extract-lua --what parser-rules   --vendor-root "$VENDOR" --out "$OVL/mod_parser_rules.json"
soft_step mod_scalability "${SYNC[@]}" extract-lua --what mod-scalability --vendor-root "$VENDOR" --out "$OVL/mod_scalability.json"
soft_step runes           "${SYNC[@]}" extract-lua --what runes           --vendor-root "$VENDOR" --out "$OVL/runes.json"
soft_step skill_overrides "${SYNC[@]}" extract-lua --vendor-root "$VENDOR" --files "$GEMFILES" --out "$OVL/skill_overrides.json"
soft_step skill_stat_map  "${SYNC[@]}" extract-lua --what stat-map        --vendor-root "$VENDOR" --files "$SMFILES" --out "$OVL/skill_stat_map.json"
soft_step spectres        "${SYNC[@]}" extract-lua --what spectres        --vendor-root "$VENDOR" --out "$OVL/spectres.json"
soft_step stat_descriptions "${SYNC[@]}" extract-lua --what stat-descriptions --vendor-root "$VENDOR" --out "$OVL/stat_descriptions.json"
soft_step stat_set_labels "${SYNC[@]}" extract-lua --what stat-set-labels --vendor-root "$VENDOR" --files "$GEMFILES" --out "$OVL/stat_set_labels.json"
soft_step uniques         "${SYNC[@]}" extract-lua --what uniques         --vendor-root "$VENDOR" --out "$OVL/uniques.json"
soft_step trade_stat_map  luajit pipeline/extract-trade-map.lua "$VENDOR" "$OVL/trade_stat_map.json"
soft_step mirage_configs  "${SYNC[@]}" gen-mirage-configs  --vendor-root "$VENDOR" --out "$OVL/mirage_configs.json"
soft_step trigger_configs "${SYNC[@]}" gen-trigger-configs --vendor-root "$VENDOR" --out "$OVL/trigger_configs.json"
# stat_id_map（M6 E/F 段 B）须在 stat_descriptions + mod_parser_rules 之后——消费两者跑引擎派生。
soft_step stat_id_map     "${SYNC[@]}" gen-stat-id-map --overlay-dir "$OVL" --out "$OVL/stat_id_map.json"

# ---- 6) 手工策展 overlay：从 OLD_PATCH 沿用（需人工复核）----
echo "== [6/8] 手工策展 overlay 从 $OLD_PATCH 沿用"
for f in buff_definitions high_precision_mods local_mods special_mods vendor_name_aliases; do
    src="data/$OLD_PATCH/overlay/$f.json"
    if [[ -f "$src" ]]; then
        cp "$src" "$OVL/$f.json"
        echo "   carried over: $f.json（人工域，复核游戏平衡变更）"
    else
        echo "   WARN: 缺 ${src}，跳过 $f.json" >&2
    fi
done

# ---- 6b) 手工策展 base 文件：管线不产出，从 OLD_PATCH 沿用（需人工复核版本变更）----
# 这些是 git 跟踪、无生成器的游戏常量/定义（武器类型、game/character constants、
# monster_scaling、non_damaging_ailments、jewel_radii、base_player_mods、enemy_presets、
# unarmed_data）；adapter --raw / extract-lua 都不产出它们。
echo "== [6b] 手工策展 base 文件从 $OLD_PATCH 沿用"
mkdir -p "$OUT_DIR/base"
# 历史树快照（0_1..0_4，冻结不变；旧 build 的 treeVersion 导入适配需要）整目录沿用。
if [[ -d "data/$OLD_PATCH/base/passive_trees" && ! -d "$OUT_DIR/base/passive_trees" ]]; then
    cp -R "data/$OLD_PATCH/base/passive_trees" "$OUT_DIR/base/"
    echo "   carried over: base/passive_trees/（历史树快照）"
fi
for f in base_player_mods character_constants enemy_presets game_constants \
         jewel_radii monster_scaling non_damaging_ailments unarmed_data weapon_types; do
    src="data/$OLD_PATCH/base/$f.json"
    if [[ -f "$src" && ! -f "$OUT_DIR/base/$f.json" ]]; then
        cp "$src" "$OUT_DIR/base/$f.json"
        echo "   carried over: base/$f.json（人工域，复核游戏版本变更）"
    fi
done

# ---- 5b) 失败 overlay 回退：从 OLD_PATCH 沿用（降级而非缺失，使新数据目录完整）----
# 韧性化核心：某 overlay 抽取失败时，不让它在新版缺席而拖垮加载/计算——若新版未
# 产出且旧版有，则沿用旧版并标记（数据/代码隔离：坏的那一块用旧值兜底，其余照常）。
if [[ ${#OVERLAY_FAILURES[@]} -gt 0 ]]; then
    echo "== [5b] 失败 overlay 回退沿用 $OLD_PATCH"
    for label in "${OVERLAY_FAILURES[@]}"; do
        new="$OVL/$label.json"; old="data/$OLD_PATCH/overlay/$label.json"
        if [[ ! -f "$new" && -f "$old" ]]; then
            cp "$old" "$new"
            echo "   fallback: $label.json ← ${OLD_PATCH}（抽取失败，沿用旧值待复核）"
        fi
    done
fi

# ---- 6c) vendor specialModList 批量抽取 (generated/special_vendor.json) ----
# 必须在 special_derived (步骤 4) 与 special_mods 沿用 (步骤 6) 之后：抽取器对这
# 两个文件做 key 去重。注意去重读的是 pobr_data::data_version() 指向的数据目录，
# 不是 $PATCH——升级 drill 中先把 DATA_VERSION 常量推进到 $PATCH 再跑本脚本。
# 4.5.4.3 升级曾漏掉这一步 (special_vendor 为 0 条)；precompile-mods --check 现在
# 会对缺失报错。
echo "== [6c] extract-lua --what special-mods (generated/special_vendor.json)"
mkdir -p "$OUT_DIR/generated"
soft_step special_vendor "${SYNC[@]}" extract-lua --what special-mods --vendor-root "$VENDOR" --out "$OUT_DIR/generated/special_vendor.json"

# ---- 7) generated/（precompile-mods）----
echo "== [7/8] precompile-mods（generated/）"
soft_step precompile_mods cargo run --quiet -p precompile-mods -- --data "$OUT_DIR" --report

# ---- 8) manifest.json：以 OLD_PATCH 结构为模板，仅换版本号 ----
echo "== [8/8] manifest.json"
if [[ -f "data/$OLD_PATCH/manifest.json" ]]; then
    sed "s/\"$OLD_PATCH\"/\"$PATCH\"/g" "data/$OLD_PATCH/manifest.json" > "$OUT_DIR/manifest.json"
fi

# ---- 汇总 ----
echo "regen-all: 完成 → $OUT_DIR"
if [[ ${#OVERLAY_FAILURES[@]} -gt 0 ]]; then
    echo "regen-all: ⚠ ${#OVERLAY_FAILURES[@]} 个 overlay 抽取失败（已尽量回退沿用旧版）：${OVERLAY_FAILURES[*]}" >&2
    echo "regen-all: 数据目录仍完整可用；请复核上述域是否需手工修复抽取通道。" >&2
    exit 2
fi
