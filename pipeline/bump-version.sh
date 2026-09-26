#!/usr/bin/env bash
# Download a selected patch, prepare its reviewed quality receipt, then generate,
# audit and validate an isolated complete snapshot before publication and CURRENT.
# Explicit vendor pin selection and numerical golden updates remain separate decisions.
# Usage: pipeline/bump-version.sh [--patch <version>] [--vendor-sha <full-sha>]
#                               [--skip-download]
# Same-version refreshes use the same candidate/rollback boundary as upgrades.
set -uo pipefail

# sccache 本机偶发拒绝启动会连坐所有 cargo 步骤（2026-08-01 两次中断实录）；
# 脚本进程内禁用 wrapper，冷构建慢一点但升级流程稳定。
export CARGO_BUILD_RUSTC_WRAPPER=""

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}" || exit 1

NEW_PATCH=""
EXPLICIT_PATCH=0
VENDOR_SHA=""
SKIP_DOWNLOAD=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --patch)         NEW_PATCH="$2"; EXPLICIT_PATCH=1; shift 2 ;;
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

# CURRENT is the sole active marker. config.json may already name an unpromoted
# candidate after a failed attempt; Rust derives its fallback from CURRENT.
OLD_PATCH="$(cat data/CURRENT)"
[[ -n "${OLD_PATCH}" ]] || { echo "bump-version: data/CURRENT is empty" >&2; exit 1; }

# ---- [1] 目标补丁号 ----
echo "== [1/9] 目标补丁号"
if [[ -z "${NEW_PATCH}" ]]; then
    NEW_PATCH="$(node pipeline/query-patch-version.mjs | sed -n 's/.*"version": "\([^"]*\)".*/\1/p')"
    [[ -n "${NEW_PATCH}" ]] || { echo "bump-version: patch 服务器查询失败，用 --patch 显式指定" >&2; exit 1; }
fi
echo "   ${OLD_PATCH} → ${NEW_PATCH}"
if [[ "${NEW_PATCH}" == "${OLD_PATCH}" ]]; then
    if [[ "$EXPLICIT_PATCH" -eq 0 && "$SKIP_DOWNLOAD" -eq 0 && -z "$VENDOR_SHA" ]]; then
        echo "   已是最新；无需下载或重新生成"
        exit 0
    fi
    echo "   已是最新（同版本重放请用 devs/scripts/version-bump-drill.sh）——继续执行属刷新语义"
fi
[[ "$NEW_PATCH" =~ ^[0-9]+(\.[0-9]+)+$ ]] || { echo "bump-version: invalid patch" >&2; exit 2; }
# Replace the actual config value, including when resuming a different failed
# candidate. Never interpolate an untrusted patch into shell code or a sed regex.
die_on_fail python3 - "$NEW_PATCH" <<'PY'
import json, pathlib, sys
path = pathlib.Path("pipeline/config.json")
config = json.loads(path.read_text(encoding="utf-8"))
config["patch"] = sys.argv[1]
path.write_text(json.dumps(config, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
PY

# ---- [2] .dat 表下载 ----
echo "== [2/9] GGG .dat 表下载（pipeline/tables/）"
if [[ "${SKIP_DOWNLOAD}" -eq 1 ]]; then
    echo "   --skip-download：跳过"
else
    # CDN 只保留当前补丁（pipeline/README.md）——下载失败通常意味着补丁号过期，重查步骤 1。
    die_on_fail bash -c "cd pipeline && node download-index.mjs"
    die_on_fail bash pipeline/export-tables.sh
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
        mkdir -p "${VENDOR_DIR}" || return 1
        local vendor_root
        vendor_root="$(git -C "${VENDOR_DIR}" rev-parse --show-toplevel 2>/dev/null)" || vendor_root=""
        if [[ -n "$vendor_root" && "$(cd "$vendor_root" && pwd -P)" != "$(cd "$VENDOR_DIR" && pwd -P)" ]]; then
            vendor_root=""
        fi
        if [[ -z "$vendor_root" ]]; then
            [[ -z "$(ls -A "$VENDOR_DIR")" ]] || { echo "bump-version: vendor 目录非空且不是独立 Git 仓库" >&2; return 1; }
            git -C "${VENDOR_DIR}" init -q || return 1
        fi
        local vendor_changes
        vendor_changes="$(git -C "${VENDOR_DIR}" status --porcelain --untracked-files=no)" || return 1
        [[ -z "$vendor_changes" ]] \
            || { echo "bump-version: vendor 有未提交修改，拒绝切换 commit" >&2; return 1; }
        git -C "${VENDOR_DIR}" remote get-url origin >/dev/null 2>&1 \
            || git -C "${VENDOR_DIR}" remote add origin https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2 \
            || return 1
        git -C "${VENDOR_DIR}" fetch -q --depth 1 origin "${VENDOR_SHA}" \
            && git -C "${VENDOR_DIR}" checkout -q FETCH_HEAD \
            && [[ "$(git -C "${VENDOR_DIR}" rev-parse HEAD)" == "${VENDOR_SHA}" ]] \
            && printf '%s\n' "${VENDOR_SHA}" > vendor/.pob2-version.txt
    }
    die_on_fail swap_vendor
    echo "   vendor → ${VENDOR_SHA}"
else
    echo "   未指定 --vendor-sha：vendor 保持 ${OLD_VENDOR_SHA:-<未检出>}（overlay 抽取将基于旧 vendor）"
fi

# ---- [5] Generate and validate the candidate snapshot ----
echo "== [5/9] candidate snapshot (OLD_PATCH=${OLD_PATCH})"
# Compatible value-only quality updates inherit reviewed semantic scope. New
# stats/effects/scopes remain explicit diagnostics rather than silently enabled.
if [[ ! -f "pipeline/gem-quality/$NEW_PATCH.json" ]]; then
    die_on_fail python3 pipeline/gem-quality/advance-receipt.py advance \
        --raw pipeline/tables/English --export pipeline/tables/quality-export-source.json \
        --previous-receipt "pipeline/gem-quality/$OLD_PATCH.json" \
        --previous-quality "data/$OLD_PATCH/overlay/gem_quality_stats.json" \
        --patch "$NEW_PATCH" --out "pipeline/gem-quality/$NEW_PATCH.json"
fi
# Generate, audit, seal and validate an isolated candidate before publishing it.
transaction_args=(regenerate --activate)
if [[ "$SKIP_DOWNLOAD" -eq 0 ]]; then transaction_args+=(--refresh-dictionary); fi
die_on_fail env OLD_PATCH="$OLD_PATCH" POBR_EXPECTED_PATCH="$NEW_PATCH" python3 pipeline/data_snapshot.py "${transaction_args[@]}"

if [[ -d web/node_modules ]]; then
    soft_step web_sync_data bash -c "cd web && pnpm run sync-data"
else
    echo "   web/node_modules 缺失——跳过 sync-data（web 下次 pnpm install 后手动跑）"
    FAILURES+=("web sync-data 未跑（无 node_modules）")
fi

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
