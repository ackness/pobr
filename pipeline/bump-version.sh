#!/usr/bin/env bash
# bump-version.sh —— 游戏数据版本升级一条命令编排（v0.0.3 P1-4，docs/version-bump-architecture.md）。
#
# 把此前散在 memory/README 里的升级 drill 串成单入口：
#   [1] 查询最新补丁号（query-patch-version.mjs），改 pipeline/config.json "patch"
#   [2] 下载 .dat 表（download-index.mjs 预热索引 + npx pathofexile-dat）
#   [3] 刷新被动树导出（GGG poe2-skilltree-export；官方停更时沿用现有，软失败）
#   [4] vendor 对齐（--vendor-sha 时 fetch-by-sha 换检出 + 更新 .pob2-version.txt）
#   [5] OLD_PATCH=<旧> regen-all.sh（含末步 test-pin bless）
#   [6] 推进版本标记：data/CURRENT + pobr-data DATA_VERSION 常量
#   [7] gen-zh-cn.mjs（读 data/CURRENT）+ web pnpm sync-data
#   [8] 定向验证（多版本 smoke + gamedata 套件；parity 仅报告不判失败）
#   [9] 摘要 + 剩余人工决策清单
#
# 刻意保留为人工决策（不自动化）：
#   - golden 翻转（GOLDEN_PARITY_DATA_VERSION + recapture_golden.py）——是否把
#     parity 基准挪到新版本取决于引擎适配进度，见 docs/adapting-to-0.5.4b.md 的教训；
#   - vendor 新 pin 的选取（PoB2 社区哪个 commit 对应新补丁）；
#   - 引擎公式适配本身（先跑 pipeline/diff-vendor-calcs.sh 拿 delta 清单）。
#
# 用法：
#   pipeline/bump-version.sh                          # 补丁号自动查询，vendor 不动
#   pipeline/bump-version.sh --patch 4.5.5.1          # 显式补丁号
#   pipeline/bump-version.sh --vendor-sha <全长sha>   # 同时换 vendor 检出
#   pipeline/bump-version.sh --skip-download          # 表已下好，从 regen 开始
#
# 各步幂等（下载有缓存、regen 整目录覆写），失败后修好直接重跑即可。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 1

NEW_PATCH=""
VENDOR_SHA=""
SKIP_DOWNLOAD=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --patch)         NEW_PATCH="$2";   shift 2 ;;
        --vendor-sha)    VENDOR_SHA="$2";  shift 2 ;;
        --skip-download) SKIP_DOWNLOAD=1;  shift ;;
        *) echo "bump-version: 未知参数 $1" >&2; exit 2 ;;
    esac
done

FAILURES=()
soft_step() {
    local label="$1"; shift
    echo "---- ${label}"
    if ! "$@"; then
        FAILURES+=("${label}")
        echo "   ⚠ 软步骤失败（续跑其余）：${label}" >&2
    fi
}
die_on_fail() { "$@" || { echo "bump-version: 关键步骤失败，中止：$*" >&2; exit 1; }; }

OLD_PATCH="$(sed -n 's/.*"patch"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' pipeline/config.json | head -1)"
[[ -n "${OLD_PATCH}" ]] || { echo "bump-version: 无法从 config.json 读取当前 patch" >&2; exit 1; }

# ---- [1] 目标补丁号 ----
echo "== [1/9] 目标补丁号"
if [[ -z "${NEW_PATCH}" ]]; then
    NEW_PATCH="$(node pipeline/query-patch-version.mjs | sed -n 's/.*"version": "\([^"]*\)".*/\1/p')"
    [[ -n "${NEW_PATCH}" ]] || { echo "bump-version: patch 服务器查询失败，用 --patch 显式指定" >&2; exit 1; }
fi
echo "   ${OLD_PATCH} → ${NEW_PATCH}"
if [[ "${NEW_PATCH}" == "${OLD_PATCH}" ]]; then
    echo "   已是最新（同版本重放请用 devs/scripts/version-bump-drill.sh）——继续执行属刷新语义"
fi
# 只改 "patch" 一行；config.json 其余（表清单）人工维护。sed -i 的 in-place 语法
# BSD/GNU 不兼容，统一走 tmp+mv。
sed_replace() {
    local pattern="$1" file="$2" tmp
    tmp="$(mktemp)"
    sed "${pattern}" "${file}" > "${tmp}" && mv "${tmp}" "${file}"
}
die_on_fail sed_replace "s/\"patch\": \"${OLD_PATCH}\"/\"patch\": \"${NEW_PATCH}\"/" pipeline/config.json

# ---- [2] .dat 表下载 ----
echo "== [2/9] GGG .dat 表下载（pipeline/tables/）"
if [[ "${SKIP_DOWNLOAD}" -eq 1 ]]; then
    echo "   --skip-download：跳过"
else
    # CDN 只保留当前补丁（pipeline/README.md）——下载失败通常意味着补丁号过期，重查步骤 1。
    die_on_fail bash -c "cd pipeline && node download-index.mjs"
    die_on_fail bash -c "cd pipeline && npx -y pathofexile-dat@15"
fi

# ---- [3] 被动树导出 ----
echo "== [3/9] 被动树导出（GGG poe2-skilltree-export）"
# 官方树导出常滞后于补丁（0.5.4b 时仍停在 0.5.2）——拿不到就沿用现有 data.json。
TREE_URL="https://raw.githubusercontent.com/grindinggear/poe2-skilltree-export/HEAD/data.json"
fetch_tree() {
    local tmp; tmp="$(mktemp)"
    if curl -fsSL --retry 3 -o "${tmp}" "${TREE_URL}" && [[ -s "${tmp}" ]]; then
        if cmp -s "${tmp}" pipeline/tree/data.json; then
            echo "   树导出无变化"
        else
            mv "${tmp}" pipeline/tree/data.json
            echo "   树导出已更新"
        fi
    else
        rm -f "${tmp}"
        echo "   下载失败——沿用现有 pipeline/tree/data.json（官方导出滞后属常态）"
    fi
}
soft_step tree_export fetch_tree

# ---- [4] vendor 对齐 ----
echo "== [4/9] PoB2 vendor 对齐"
VENDOR_DIR="vendor/PathOfBuilding-PoE2"
OLD_VENDOR_SHA="$(cat vendor/.pob2-version.txt 2>/dev/null || echo '')"
if [[ -n "${VENDOR_SHA}" ]]; then
    [[ "${#VENDOR_SHA}" -eq 40 ]] || { echo "bump-version: --vendor-sha 需全长 40 位（gh api 查，别猜短 sha）" >&2; exit 1; }
    swap_vendor() {
        mkdir -p "${VENDOR_DIR}"
        git -C "${VENDOR_DIR}" rev-parse --git-dir >/dev/null 2>&1 || git -C "${VENDOR_DIR}" init -q
        git -C "${VENDOR_DIR}" remote get-url origin >/dev/null 2>&1 \
            || git -C "${VENDOR_DIR}" remote add origin https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2
        git -C "${VENDOR_DIR}" fetch -q --depth 1 origin "${VENDOR_SHA}" \
            && git -C "${VENDOR_DIR}" checkout -q FETCH_HEAD \
            && printf '%s\n' "${VENDOR_SHA}" > vendor/.pob2-version.txt
    }
    die_on_fail swap_vendor
    echo "   vendor → ${VENDOR_SHA}"
else
    echo "   未指定 --vendor-sha：vendor 保持 ${OLD_VENDOR_SHA:-<未检出>}（overlay 抽取将基于旧 vendor）"
fi

# ---- [5] 全量重生成（含 test-pin bless 末步）----
echo "== [5/9] regen-all（OLD_PATCH=${OLD_PATCH}）"
die_on_fail env OLD_PATCH="${OLD_PATCH}" pipeline/regen-all.sh

# ---- [6] 推进活动版本标记 ----
echo "== [6/9] data/CURRENT + pobr_data::DATA_VERSION"
printf '%s\n' "${NEW_PATCH}" > data/CURRENT
# 只动 DATA_VERSION 一行；GOLDEN_PARITY_DATA_VERSION 是人工决策，绝不自动推进。
die_on_fail sed_replace \
    "s/^pub const DATA_VERSION: &str = \"${OLD_PATCH}\";/pub const DATA_VERSION: \&str = \"${NEW_PATCH}\";/" \
    crates/pobr-data/src/lib.rs
grep -q "pub const DATA_VERSION: &str = \"${NEW_PATCH}\";" crates/pobr-data/src/lib.rs \
    || { echo "bump-version: DATA_VERSION 常量替换失败（crates/pobr-data/src/lib.rs 格式变了？）" >&2; exit 1; }

# ---- [7] 衍生数据 ----
echo "== [7/9] 简中语言包 + web 数据同步"
soft_step gen_zh_cn node pipeline/gen-zh-cn.mjs
if [[ -d web/node_modules ]]; then
    soft_step web_sync_data bash -c "cd web && pnpm run sync-data"
else
    echo "   web/node_modules 缺失——跳过 sync-data（web 下次 pnpm install 后手动跑）"
    FAILURES+=("web sync-data 未跑（无 node_modules）")
fi

# ---- [8] 定向验证（禁全量，见 CLAUDE.md 验证分层）----
echo "== [8/9] 定向验证"
soft_step smoke_versions bash .claude/skills/run-pobr/driver.sh versions
soft_step gamedata_tests env CARGO_BUILD_RUSTC_WRAPPER="" cargo test --quiet -p pobr-gamedata
# parity 只报告不判失败：golden 仍钉旧版本时本就不应红；红/绿都由人读。
echo "---- parity（信息性，不计失败）"
env CARGO_BUILD_RUSTC_WRAPPER="" cargo test --quiet -p pobr-build --test parity 2>&1 | tail -5 || true

# ---- [9] 摘要 ----
echo "== [9/9] 摘要"
echo "补丁：${OLD_PATCH} → ${NEW_PATCH}    vendor：${OLD_VENDOR_SHA:-?} → $(cat vendor/.pob2-version.txt 2>/dev/null || echo '?')"
if [[ ${#FAILURES[@]} -gt 0 ]]; then
    echo "软失败步骤（须处理后重跑对应步）："
    printf '  - %s\n' "${FAILURES[@]}"
fi
cat <<EOF
剩余人工决策：
  1. review 本次 data/ diff 后提交（生成物已标 -diff，重点看 overlay 人工域与 base 结构变化）；
  2. 引擎适配 triage：pipeline/diff-vendor-calcs.sh ${OLD_VENDOR_SHA:-<旧sha>} <新sha> 拿公式 delta 清单；
  3. golden 是否翻到 ${NEW_PATCH}：引擎补齐后跑 examples/demo-bd-test/tools/recapture_golden.py
     并推进 pobr_data::GOLDEN_PARITY_DATA_VERSION（教训见 docs/adapting-to-0.5.4b.md）。
EOF
[[ ${#FAILURES[@]} -eq 0 ]]
