#!/usr/bin/env bash
# version-bump-drill —— 「版本更新只动 JSON」可执行演练 第一版（M3 T5-F，P18；
# 蓝图 audits/rearchitecture-2026-06-10/blueprints/m3-orchestration.md §8.3）。
#
# 覆盖 M3 时点已存在的管线步：
#   [1] pipeline 下载校验（占位：校验输入目录表清单完整性）
#   [2] pobr-data-adapter 重跑 → 临时目录，与已提交 data/<ver>/base byte-diff
#   [3] extract 已注册抽取全跑：扫 data/<ver>/overlay/*.json 的 _meta.regen_command，
#       凡 `cargo run -p sync-pob-catalog -- …` 可执行命令均重跑到临时文件后 byte-diff
#       （人工策展域 regen_command 为对账说明而非命令 → SKIP）
#   [4] precompile：占位 SKIP（M6 落地后接入）
#   [5] 校验：A) 上述 byte-diff=0  B) cargo build --workspace 零改动编译
#       C) ninja_parity 可运行（要求不 crash，不要求达标）
#   [6] 摘要输出；发现项登记到 audits/rearchitecture-2026-06-10/drill-findings-m3.md
#       （由演练执行者人工撰写——「必须改 Rust 才能吸收」的每项一条）
#
# 缺依赖优雅降级：无 luajit / 无 vendor 检出 / 无 pipeline 输入 → 对应步骤报 SKIP
# 不报错（退出码 0）；byte-diff 漂移 / 编译失败 → 退出码 1。
#
# 用法：
#   devs/scripts/version-bump-drill.sh \
#     [--data-export <pipeline/tables 目录>] [--vendor <vendor src 目录>] [--version <ver>]
#
# 无新版本时的演练形态＝对当前版本输入重放（全部 byte-diff 应为零）。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DATA_EXPORT="$ROOT/pipeline/tables"
VENDOR="$ROOT/vendor/PathOfBuilding-PoE2/src"
VERSION=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --data-export) DATA_EXPORT="$2"; shift 2 ;;
        --vendor)      VENDOR="$2";      shift 2 ;;
        --version)     VERSION="$2";     shift 2 ;;
        *) echo "version-bump-drill: 未知参数 $1" >&2; exit 2 ;;
    esac
done

if [[ -z "$VERSION" ]]; then
    VERSION="$(sed -n 's/.*"patch"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
        "$ROOT/pipeline/config.json" 2>/dev/null | head -1)"
fi
if [[ -z "$VERSION" || ! -d "$ROOT/data/$VERSION" ]]; then
    echo "version-bump-drill: 无法确定版本（--version / pipeline/config.json \"patch\"），或 data/$VERSION 不存在" >&2
    exit 2
fi

OVERLAY_DIR="$ROOT/data/$VERSION/overlay"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/pobr-drill.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

FAIL=0
SKIPPED=()
DIFFED=()

step() { echo ""; echo "==== [$1] $2 ===="; }

# ---- [1] pipeline 下载校验（占位）----
step 1 "pipeline 输入清单校验（占位）"
if [[ -f "$DATA_EXPORT/English/BaseItemTypes.json" \
   && -f "$DATA_EXPORT/Traditional Chinese/BaseItemTypes.json" ]]; then
    echo "OK: $DATA_EXPORT 含 English / Traditional Chinese 表导出"
    HAVE_TABLES=1
else
    echo "SKIP: $DATA_EXPORT 缺 .dat 表导出（pipeline/README.md 第 2 步）"
    SKIPPED+=("pipeline tables 输入缺失 → adapter 步跳过")
    HAVE_TABLES=0
fi
TREE_JSON="$ROOT/pipeline/tree/data.json"
[[ -f "$TREE_JSON" ]] || { echo "SKIP: 无 pipeline/tree/data.json（树域重放跳过）"; SKIPPED+=("pipeline tree 输入缺失 → 树域重放跳过"); }

# ---- [2] adapter 重放 → byte-diff ----
step 2 "pobr-data-adapter 重放（base 域）"
if [[ "$HAVE_TABLES" -eq 1 ]]; then
    if cargo run --quiet -p pobr-data-adapter --manifest-path "$ROOT/Cargo.toml" -- \
        --raw "$DATA_EXPORT" --out "$TMP/regen" --patch "$VERSION"; then
        N=0
        while IFS= read -r f; do
            rel="${f#"$TMP/regen/$VERSION/"}"
            [[ "$rel" == "manifest.json" ]] && continue   # 多步骤合并产物，暂排除（同 regen-check.sh）
            committed="$ROOT/data/$VERSION/base/$rel"
            [[ -f "$committed" ]] || committed="$ROOT/data/$VERSION/$rel"
            if [[ ! -f "$committed" ]]; then
                DIFFED+=("base/${rel}（重放产物在仓库无对应文件）"); FAIL=1; continue
            fi
            cmp -s "$f" "$committed" || { DIFFED+=("base/$rel"); FAIL=1; }
            N=$((N + 1))
        done < <(find "$TMP/regen/$VERSION" -type f -name '*.json' | sort)
        echo "byte-diff 比对 $N 个 base 文件"
    else
        echo "FAIL: adapter 重放退出非零"; FAIL=1
    fi
else
    echo "SKIP（见步骤 1）"
fi
if [[ -f "$TREE_JSON" ]]; then
    cargo run --quiet -p pobr-data-adapter --manifest-path "$ROOT/Cargo.toml" -- \
        --tree "$TREE_JSON" --out "$TMP/regen" --patch "$VERSION" || { echo "FAIL: 树域重放退出非零"; FAIL=1; }
fi

# ---- [3] 已注册抽取全跑 → byte-diff ----
step 3 "sync-pob-catalog 已注册抽取重放（overlay 域）"
if [[ ! -d "$VENDOR" ]]; then
    echo "SKIP: vendor 检出不存在（$VENDOR）"
    SKIPPED+=("vendor 缺失 → 全部 overlay 抽取跳过")
elif ! command -v luajit >/dev/null 2>&1; then
    echo "SKIP: 未安装 luajit（extract-lua 依赖）"
    SKIPPED+=("luajit 缺失 → 全部 overlay 抽取跳过")
else
    for overlay in "$OVERLAY_DIR"/*.json; do
        name="$(basename "$overlay")"
        cmd="$(python3 -c '
import json,sys
d=json.load(open(sys.argv[1]))
print((d.get("_meta",{}) or {}).get("regen_command","") if isinstance(d,dict) else "")' "$overlay")"
        if [[ "$cmd" != cargo\ run\ -p\ sync-pob-catalog\ --\ * ]]; then
            echo "SKIP ${name}（无可执行 regen_command：人工策展/对账域）"
            SKIPPED+=("overlay/$name 非抽取域（人工策展）")
            continue
        fi
        # `_meta.regen_command` 把 `--out` 原路径写进产物 → 重放必须**原路径就地写**
        # 再 byte-diff（临时 out 会让 _meta 自指差异假阳性）。快照后就地跑、对比、还原。
        # `--vendor-root` 可重写（_meta 内按约定固定写 canonical 相对路径，不影响字节）。
        rewritten="$(python3 - "$cmd" "$VENDOR" <<'PY'
import shlex, sys
argv = shlex.split(sys.argv[1]); vendor = sys.argv[2]
for i, a in enumerate(argv):
    if a == "--vendor-root": argv[i + 1] = vendor
print(shlex.join(argv))
PY
)"
        snap="$TMP/snap-$name"
        cp "$overlay" "$snap"
        if (cd "$ROOT" && eval "$rewritten" >/dev/null 2>"$TMP/err-$name"); then
            if cmp -s "$overlay" "$snap"; then
                echo "OK   $name byte-diff=0"
            else
                echo "DIFF ${name}（重放产物与已提交不一致）"
                DIFFED+=("overlay/$name"); FAIL=1
            fi
        else
            echo "FAIL ${name}（抽取命令退出非零，stderr 摘要：）"
            tail -3 "$TMP/err-$name" | sed 's/^/       /'
            FAIL=1
        fi
        cp "$snap" "$overlay"   # 无论结果如何还原已提交产物（drill 不留工作区改动）
    done
fi

# ---- [4] precompile（占位）----
step 4 "precompile（M6）"
echo "SKIP: parser 规则 precompile 在 M6 落地后接入"

# ---- [5] 编译 + parity 可运行 ----
step 5 "cargo build --workspace（零改动编译）"
if (cd "$ROOT" && cargo build --workspace --quiet); then echo "OK"; else echo "FAIL"; FAIL=1; fi

step 5b "ninja_parity 可运行（不要求达标，要求不 crash）"
if (cd "$ROOT" && cargo test -p pobr-build --test ninja_parity --quiet >"$TMP/parity.out" 2>&1); then
    echo "OK: ninja_parity 套件通过"
else
    if grep -qE "test result:" "$TMP/parity.out"; then
        echo "WARN: ninja_parity 有用例失败（可运行，达标情况见输出）——非 drill 失败项"
        grep -E "test result:" "$TMP/parity.out" | sed 's/^/       /'
    else
        echo "FAIL: ninja_parity 无法运行（crash/编译失败）"; FAIL=1
    fi
fi

# ---- [6] 摘要 ----
step 6 "摘要"
echo "版本：$VERSION"
[[ ${#DIFFED[@]} -gt 0 ]] && { echo "byte-diff 漂移："; printf '  - %s\n' "${DIFFED[@]}"; }
[[ ${#SKIPPED[@]} -gt 0 ]] && { echo "跳过项："; printf '  - %s\n' "${SKIPPED[@]}"; }
echo "发现项登记：audits/rearchitecture-2026-06-10/drill-findings-m3.md（人工撰写，"
echo "「必须改 Rust 才能吸收」的每项一条 → 转入 M5/M6 数据化清单）"
if [[ "$FAIL" -eq 0 ]]; then echo "version-bump-drill: PASS"; else echo "version-bump-drill: FAIL"; fi
exit "$FAIL"
