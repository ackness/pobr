#!/usr/bin/env bash
# version-bump-drill —— 「版本更新只动 JSON」可执行演练 第一版（M3 T5-F，P18；
# 蓝图 audits/rearchitecture-2026-06-10/blueprints/m3-orchestration.md §8.3）。
#
# 覆盖 M3 时点已存在的管线步：
#   [1] pipeline 下载校验（占位：校验输入目录表清单完整性）
#   [2] pobr-data-adapter 重跑 → 临时目录，与已提交 data/<ver>/base byte-diff
#   [3] extract 已注册抽取全跑：扫 data/<ver>/overlay/*.json 的 _meta.regen_command，
#       凡 `cargo run -p sync-pob-catalog -- …` 可执行命令均重跑到临时路径后 byte-diff
#       （人工策展域 regen_command 为对账说明而非命令 → SKIP）。
#       F1 已吸收：_meta.regen_command 的 --out 由工具归一化为 canonical 仓库相对
#       路径，临时重放只需保留 `data/<ver>/overlay/<file>` 尾段结构即可逐字节还原
#       同一 _meta——无须就地覆写已提交产物再还原。
#   [3b] 策展域对账（F4）：buff_definitions.json 跑 check-buff-refs（vendor_ref
#       行段 hash 对账，机跑）；high_precision_mods / local_mods / special_mods
#       的对账口径见各自 _meta.audit / regen_command 说明（人工域）。
#   [4] precompile：重跑 precompile-mods → 临时数据目录（符号链接输入），与已提交
#       data/<ver>/generated/{parsed_mods,parse-coverage}.json byte-diff（产物可再生性）。
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
#     [--data-export <pipeline/tables 目录>] [--vendor <vendor src 目录>] \
#     [--version <ver>] [--version-file <.pob2-version.txt 路径>]
#
# 无新版本时的演练形态＝对当前版本输入重放（全部 byte-diff 应为零）。
#
# vendor 副本场景（F6）：extract 默认按约定布局读
# `<vendor_root>/../../.pob2-version.txt`；当 `--vendor` 指向 vendor **副本**
# （如模拟数值变更的临时拷贝），该约定路径通常不存在 → 必须显式传
# `--version-file`（一般仍指向真 vendor 检出的 `.pob2-version.txt`），
# 否则全部抽取因读不到 vendor commit 而失败。

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DATA_EXPORT="$ROOT/pipeline/tables"
VENDOR="$ROOT/vendor/PathOfBuilding-PoE2/src"
VERSION=""
VERSION_FILE=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --data-export)  DATA_EXPORT="$2";  shift 2 ;;
        --vendor)       VENDOR="$2";       shift 2 ;;
        --version)      VERSION="$2";      shift 2 ;;
        --version-file) VERSION_FILE="$2"; shift 2 ;;
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
    echo "SKIP: vendor 检出不存在（${VENDOR}）"
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
        # F1 已吸收：工具把 _meta.regen_command 的 --out 归一化为 canonical 仓库
        # 相对路径（截取路径中最后一个 `data/...` 相对段）→ 重放到保留
        # `data/<ver>/overlay/<file>` 尾段的临时路径即可逐字节还原同一 _meta，
        # 直接 byte-diff 临时产物 vs 已提交文件（不再就地覆写+还原）。
        # `--vendor-root` / `--version-file` 同步重写为本次演练输入。
        tmp_out="$TMP/replay/data/$VERSION/overlay/$name"
        mkdir -p "$(dirname "$tmp_out")"
        rewritten="$(python3 - "$cmd" "$VENDOR" "$tmp_out" "$VERSION_FILE" <<'PY'
import shlex, sys
argv = shlex.split(sys.argv[1])
vendor, out, version_file = sys.argv[2:5]
def set_opt(flag, value):
    if not value:
        return
    if flag in argv:
        argv[argv.index(flag) + 1] = value
    else:
        argv.extend([flag, value])
set_opt("--vendor-root", vendor)
set_opt("--out", out)
set_opt("--version-file", version_file)
print(shlex.join(argv))
PY
)"
        if (cd "$ROOT" && eval "$rewritten" >/dev/null 2>"$TMP/err-$name"); then
            if cmp -s "$tmp_out" "$overlay"; then
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
    done
fi

# ---- [3b] 策展域对账：buff_definitions（F4，机跑通道）----
step 3b "策展域对账：check-buff-refs（buff_definitions.json）"
# buff_definitions 已迁至版本无关的 data/overlay-common/（P1-3）；版本特有覆盖若存在
# 则优先对账，否则对账 common 层基底。
DEFS="$OVERLAY_DIR/buff_definitions.json"
[[ -f "$DEFS" ]] || DEFS="$ROOT/data/overlay-common/buff_definitions.json"
if [[ ! -d "$VENDOR" ]]; then
    echo "SKIP: vendor 检出不存在（${VENDOR}）"
    SKIPPED+=("vendor 缺失 → buff_definitions 对账跳过")
elif [[ ! -f "$DEFS" ]]; then
    echo "SKIP: 无 $DEFS"
    SKIPPED+=("buff_definitions.json 缺失 → 对账跳过")
else
    if (cd "$ROOT" && cargo run --quiet -p sync-pob-catalog -- check-buff-refs \
        --vendor-root "$VENDOR" --defs "$DEFS" >/dev/null 2>"$TMP/err-buff-refs"); then
        echo "OK: vendor_ref 行段 hash 全部一致"
    else
        echo "FAIL: buff_definitions vendor_ref 行段漂移（vendor 升级后须人工复核 + --write 刷新）"
        tail -5 "$TMP/err-buff-refs" | sed 's/^/       /'
        FAIL=1
    fi
fi
# 其余策展域（high_precision_mods / local_mods / special_mods）无机跑对账命令，
# 版本 bump 时按各自 _meta.audit / regen_command 说明人工对账（F4 登记口径）。

# ---- [4] precompile 重放 → byte-diff ----
step 4 "precompile 重放（generated/parsed_mods + parse-coverage byte-diff）"
GEN_DIR="$ROOT/data/$VERSION/generated"
SD="$GEN_DIR/special_derived.json"
if [[ ! -f "$GEN_DIR/parsed_mods.json" ]]; then
    echo "SKIP: 无 $GEN_DIR/parsed_mods.json（precompile 产物未入库）"
    SKIPPED+=("generated/parsed_mods.json 缺失 → precompile 重放跳过")
elif [[ ! -f "$SD" ]]; then
    echo "SKIP: 无 ${SD}（precompile SD 语料输入缺失）"
    SKIPPED+=("generated/special_derived.json 缺失 → precompile 重放跳过")
else
    # 临时数据目录（符号链接输入，real generated 收输出）：examples_dir 取 data_dir
    # 上溯两级，故布局须为 <tmp>/data/<ver> 并在 <tmp> 放 examples 软链。不就地覆写
    # 已提交 generated 产物（与步骤 2/3 同纪律）。
    PC_DIR="$TMP/precompile"
    PC_DD="$PC_DIR/data/$VERSION"
    mkdir -p "$PC_DD/generated"
    ln -s "$ROOT/data/$VERSION/base" "$PC_DD/base"
    ln -s "$ROOT/data/$VERSION/overlay" "$PC_DD/overlay"
    [[ -f "$ROOT/data/$VERSION/manifest.json" ]] && ln -s "$ROOT/data/$VERSION/manifest.json" "$PC_DD/manifest.json"
    ln -s "$SD" "$PC_DD/generated/special_derived.json"
    [[ -d "$ROOT/examples" ]] && ln -s "$ROOT/examples" "$PC_DIR/examples"
    if cargo run --quiet -p precompile-mods --manifest-path "$ROOT/Cargo.toml" -- \
        --data "$PC_DD" --report >/dev/null 2>"$TMP/err-precompile"; then
        for art in parsed_mods.json parse-coverage.json; do
            committed="$GEN_DIR/$art"
            if [[ ! -f "$committed" ]]; then
                echo "SKIP ${art}（仓库无对应产物）"; SKIPPED+=("generated/$art 未入库")
            elif cmp -s "$PC_DD/generated/$art" "$committed"; then
                echo "OK   $art byte-diff=0"
            else
                echo "DIFF ${art}（重放产物与已提交不一致）"; DIFFED+=("generated/$art"); FAIL=1
            fi
        done
    else
        echo "FAIL precompile 重放退出非零，stderr 摘要："
        tail -3 "$TMP/err-precompile" | sed 's/^/       /'
        FAIL=1
    fi
fi

# ---- [5] 编译 + parity 可运行 ----
step 5 "cargo build --workspace（零改动编译）"
if (cd "$ROOT" && cargo build --workspace --quiet); then echo "OK"; else echo "FAIL"; FAIL=1; fi

step 5b "ninja_parity 可运行（不要求达标，要求不 crash）"
# ninja_parity 已并入 parity 聚合二进制（53→8 binary）；按子模块名过滤跑。
if (cd "$ROOT" && cargo test -p pobr-build --test parity --quiet ninja_parity >"$TMP/parity.out" 2>&1); then
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
