#!/usr/bin/env bash
# diff-vendor-calcs.sh — P0-2: vendor calc-delta report.
#
# Compares two PoB2 vendor pins (PathOfBuilding-PoE2 commits) and emits a
# markdown triage list of what a game patch changed in the calc engine and in
# the extracted overlays. Engine adaptation then starts from a checklist,
# instead of "flip the golden, watch parity drop, oracle-debug build by build".
#
# Usage:
#   pipeline/diff-vendor-calcs.sh <old-sha> <new-sha> [--out <file>]
#
# Default output: devs/docs/audits/vendor-delta-<new-sha[:8]>.md
#
# Env knobs:
#   MAX_HUNK_LINES=800   per-file calc hunk cap; larger diffs collapse to
#                        numstat only (keeps the report readable).
#   DATA_TOP=40          how many data files to rank by churn.
#
# Deps: git, python3, cargo (+ luajit for extract-lua). Each vendor pin is a
# shallow git checkout in .cache/vendor-delta/<sha>/ (gitignored, cache-hit
# skips re-fetch). Soft-steps (regen-all.sh convention): an extract that fails
# on the *old* pin (incompatible Lua structure) is noted and skipped, the report
# still generates.
set -euo pipefail

VENDOR_REPO="https://github.com/PathOfBuildingCommunity/PathOfBuilding-PoE2.git"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE="${ROOT}/.cache/vendor-delta"
HELPER="${ROOT}/pipeline/json_entry_diff.py"
SHIM="${ROOT}/pipeline/lua-shims/lua-utf8.lua"
MAX_HUNK_LINES="${MAX_HUNK_LINES:-800}"
DATA_TOP="${DATA_TOP:-40}"

# Calc engine files PoBR mirrors: everything Calc* plus the two big non-Calc
# calc files. Data*.lua go in the data section, so exclude them here.
CALC_GLOBS=("Modules/Calc"*.lua "Modules/ModParser.lua")

SYNC=(cargo run --quiet -p sync-pob-catalog --)
SOFT_FAILURES=()

usage() { grep -E '^# ' "$0" | sed -E 's/^# ?//'; }

die() { echo "diff-vendor-calcs: $*" >&2; exit 1; }

# soft: run cmd; on failure record label and keep going (always returns 0 so
# `set -e` never fires on an expected soft-step failure).
soft() {
  local label="$1"; shift
  if ! "$@"; then
    SOFT_FAILURES+=("${label}")
    echo "  ⚠ soft-step failed (continuing): ${label}" >&2
  fi
  return 0
}

# Fetch a pin as a shallow git checkout (driver.sh style). NOT a codeload
# tarball: the headless extractor's HeadlessWrapper.lua bootstrap needs the full
# working tree (e.g. TreeData/*/tree.lua) that codeload strips, so a tarball pin
# yields "modLib.parseMod missing" and the overlay diff can't run.
fetch() {
  local sha="$1"
  local dir="${CACHE}/${sha}"
  if [ -d "${dir}/.git" ] && [ -f "${dir}/src/Modules/ModParser.lua" ]; then
    echo "  cache hit: ${sha}" >&2
  else
    echo "  fetching ${sha} (shallow git) ..." >&2
    rm -rf "${dir}"; mkdir -p "${dir}"
    git -C "${dir}" init -q
    git -C "${dir}" remote add origin "${VENDOR_REPO}"
    # The vendor tree carries heavy binary art, so the shallow pack is large and
    # flaky over a slow link — retry the transient "early EOF" failures.
    local ok=0 attempt
    for attempt in 1 2 3; do
      if git -C "${dir}" fetch -q --depth 1 origin "${sha}"; then ok=1; break; fi
      echo "  fetch attempt ${attempt} failed; retrying ..." >&2
      sleep 5
    done
    [ "${ok}" -eq 1 ] || die "git fetch failed for ${sha} after 3 attempts (bad sha? network?)"
    git -C "${dir}" checkout -q FETCH_HEAD \
      || die "git checkout failed for ${sha}"
  fi
  # extract-lua's headless bootstrap requires lua-utf8 (C module in vendor);
  # drop the tracked pure-Lua shim where PoB expects it (regen-all.sh does this).
  local rt="${dir}/runtime/lua"
  if [ -d "${rt}" ] && [ ! -f "${rt}/lua-utf8.lua" ] && [ -f "${SHIM}" ]; then
    cp "${SHIM}" "${rt}/lua-utf8.lua"
  fi
}

pob_version() {
  local mf="${CACHE}/$1/manifest.xml" v=""
  [ -f "${mf}" ] && v="$(grep -oE 'Version number="[^"]+"' "${mf}" | head -1 | sed -E 's/.*"([^"]+)".*/\1/')"
  echo "${v:-unknown}"
}

# ---- args ----------------------------------------------------------------
OLD=""; NEW=""; OUT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    -*) die "unknown flag: $1" ;;
    *)  if [ -z "${OLD}" ]; then OLD="$1"; elif [ -z "${NEW}" ]; then NEW="$1"; else die "unexpected arg: $1"; fi; shift ;;
  esac
done
[ -n "${OLD}" ] && [ -n "${NEW}" ] || { usage; exit 2; }
[ -f "${HELPER}" ] || die "missing helper: ${HELPER}"
OUT="${OUT:-${ROOT}/devs/docs/audits/vendor-delta-${NEW:0:8}.md}"

# ---- fetch both pins -----------------------------------------------------
echo "== [1/4] fetch vendor pins"
fetch "${OLD}"
fetch "${NEW}"
OLD_SRC="${CACHE}/${OLD}/src"
NEW_SRC="${CACHE}/${NEW}/src"
# extract-lua reads a `.pob2-version.txt` next to the vendor tree for _meta;
# the cache layout has none, so write a per-pin file and pass --version-file.
OLD_VF="${CACHE}/${OLD}/.pob2-version.txt"; printf '%s\n' "${OLD}" > "${OLD_VF}"
NEW_VF="${CACHE}/${NEW}/.pob2-version.txt"; printf '%s\n' "${NEW}" > "${NEW_VF}"
OLD_VER="$(pob_version "${OLD}")"
NEW_VER="$(pob_version "${NEW}")"

# ---- ensure sync-pob-catalog builds (sccache-safe) -----------------------
echo "== [2/4] build sync-pob-catalog"
if ! cargo build --quiet -p sync-pob-catalog 2>/dev/null; then
  echo "  build failed; retrying with rustc wrapper disabled (sccache bypass)" >&2
  export CARGO_BUILD_RUSTC_WRAPPER=""
  cargo build --quiet -p sync-pob-catalog || die "sync-pob-catalog build failed"
fi

# ---- report scaffolding --------------------------------------------------
mkdir -p "$(dirname "${OUT}")"
: > "${OUT}"
w() { printf '%s\n' "$*" >> "${OUT}"; }

w "# Vendor calc-delta — ${OLD:0:8} → ${NEW:0:8}"
w ""
w "| | old | new |"
w "|---|---|---|"
w "| commit | \`${OLD}\` | \`${NEW}\` |"
w "| PoB2 version | ${OLD_VER} | ${NEW_VER} |"
w ""
w "Generated by \`pipeline/diff-vendor-calcs.sh ${OLD} ${NEW}\`."
w "This is a Phase-1 engine work list: the calc-file hunks and overlay entry"
w "deltas below are what a hand-port must chase before flipping the parity golden."
w ""

# ---- 3a. calc module diff ------------------------------------------------
echo "== [3/4] diff calc modules + data"
w "## Calc module diff"
w ""
w "Files PoBR mirrors: \`Modules/Calc*.lua\` + \`Modules/ModParser.lua\`."
w "Full hunks are in the collapsed blocks; a file whose diff exceeds"
w "\`MAX_HUNK_LINES=${MAX_HUNK_LINES}\` shows only its stat line."
w ""

# collect the union of calc files across both pins
declare -a CALC_FILES=()
for base in "${OLD_SRC}" "${NEW_SRC}"; do
  for g in "${CALC_GLOBS[@]}"; do
    for f in "${base}/"${g}; do
      [ -e "${f}" ] || continue
      CALC_FILES+=("${f#"${base}/"}")
    done
  done
done
# unique, sorted
mapfile -t CALC_FILES < <(printf '%s\n' "${CALC_FILES[@]}" | sort -u)

w "| file | +added | -removed |"
w "|---|---:|---:|"
declare -a CALC_CHANGED=()
for rel in "${CALC_FILES[@]}"; do
  a="${OLD_SRC}/${rel}"; b="${NEW_SRC}/${rel}"
  [ -f "${a}" ] || a=/dev/null
  [ -f "${b}" ] || b=/dev/null
  ns="$(git diff --no-index --numstat "${a}" "${b}" 2>/dev/null || true)"
  [ -n "${ns}" ] || continue
  add="$(printf '%s' "${ns}" | awk '{print $1}')"
  del="$(printf '%s' "${ns}" | awk '{print $2}')"
  w "| \`${rel}\` | ${add} | ${del} |"
  CALC_CHANGED+=("${rel}")
done
[ ${#CALC_CHANGED[@]} -gt 0 ] || w "| _(no calc files changed)_ | | |"
w ""

for rel in "${CALC_CHANGED[@]}"; do
  a="${OLD_SRC}/${rel}"; b="${NEW_SRC}/${rel}"
  [ -f "${a}" ] || a=/dev/null
  [ -f "${b}" ] || b=/dev/null
  patch="$(git diff --no-index -- "${a}" "${b}" 2>/dev/null || true)"
  n="$(printf '%s\n' "${patch}" | wc -l | tr -d ' ')"
  w "<details><summary><code>${rel}</code> (${n} diff lines)</summary>"
  w ""
  if [ "${n}" -gt "${MAX_HUNK_LINES}" ]; then
    w "_Hunks omitted (${n} lines > MAX_HUNK_LINES=${MAX_HUNK_LINES}). Re-run with a higher MAX_HUNK_LINES to inline._"
  else
    w '```diff'
    printf '%s\n' "${patch}" >> "${OUT}"
    w '```'
  fi
  w ""
  w "</details>"
  w ""
done

# ---- 3b. data module diff (stat + churn ranking only) --------------------
w "## Data module diff"
w ""
w "\`src/Data/\` + \`src/Modules/Data*.lua\` — data tables; ranked by churn, no hunks."
w ""
# numstat over the two Data trees; Modules/Data* handled by a second pass.
data_numstat() {
  git diff --no-index --numstat "${OLD_SRC}/Data" "${NEW_SRC}/Data" 2>/dev/null || true
  for base in "${OLD_SRC}" "${NEW_SRC}"; do
    for f in "${base}/Modules/Data"*.lua; do
      [ -e "${f}" ] || continue
      rel="${f#"${base}/"}"
      a="${OLD_SRC}/${rel}"; b="${NEW_SRC}/${rel}"
      [ -f "${a}" ] || a=/dev/null
      [ -f "${b}" ] || b=/dev/null
      git diff --no-index --numstat "${a}" "${b}" 2>/dev/null || true
    done
  done
}
w "| file | +added | -removed | churn |"
w "|---|---:|---:|---:|"
rows="$(data_numstat | awk 'NF>=3 && ($1+0>0 || $2+0>0){ch=$1+$2; path=$3; for(i=4;i<=NF;i++)path=path" "$i; print ch"\t"$1"\t"$2"\t"path}' | sort -rn | head -n "${DATA_TOP}")"
if [ -n "${rows}" ]; then
  # strip the codeload cache prefix from paths for readability
  printf '%s\n' "${rows}" | while IFS=$'\t' read -r ch add del path; do
    # collapse git's `{old => new}` rename braces first, then strip the cache
    # prefix so only the vendor-relative path (Data/...) remains.
    rel="$(printf '%s' "${path}" | sed -E "s#\{[^}]*=> ([^}]*)\}#\1#g; s#.*/${NEW}/src/##; s#.*/${OLD}/src/##")"
    w "| \`${rel}\` | ${add} | ${del} | ${ch} |"
  done
else
  w "| _(no data files changed)_ | | | |"
fi
w ""

# ---- 4. extracted overlay diff -------------------------------------------
echo "== [4/4] diff extracted overlays"
w "## Extracted overlay diff"
w ""
w "\`extract-lua\` run against both pins; entry-level add/remove/change per collection."
w "A section missing below means extraction failed on the *old* pin (incompatible"
w "Lua structure) — see the soft-step summary at the end."
w ""
TMP="$(mktemp -d)"
trap 'rm -rf "${TMP}"' EXIT

extract_diff() {
  local what="$1"
  local o="${TMP}/${what}-old.json" n="${TMP}/${what}-new.json"
  w "### \`--what ${what}\`"
  w ""
  local ok_old=1 ok_new=1
  "${SYNC[@]}" extract-lua --what "${what}" --vendor-root "${OLD_SRC}" --version-file "${OLD_VF}" --out "${o}" >/dev/null 2>&1 || ok_old=0
  "${SYNC[@]}" extract-lua --what "${what}" --vendor-root "${NEW_SRC}" --version-file "${NEW_VF}" --out "${n}" >/dev/null 2>&1 || ok_new=0
  if [ "${ok_old}" -eq 0 ] || [ "${ok_new}" -eq 0 ]; then
    local which="old=${ok_old} new=${ok_new}"
    SOFT_FAILURES+=("extract ${what} (${which})")
    w "_Skipped — extraction failed (${which}, 1=ok 0=fail). Old-pin failures are"
    w "expected when the vendor Lua structure changed across the bump._"
    w ""
    return 0
  fi
  python3 "${HELPER}" "${o}" "${n}" >> "${OUT}" || SOFT_FAILURES+=("diff ${what}")
  w ""
}

for what in special-mods parser-rules uniques; do
  extract_diff "${what}"
done

# ---- soft-step summary ---------------------------------------------------
w "## Soft-step summary"
w ""
if [ ${#SOFT_FAILURES[@]} -eq 0 ]; then
  w "All steps succeeded."
else
  for f in "${SOFT_FAILURES[@]}"; do w "- ⚠ ${f}"; done
fi
w ""

echo "report written: ${OUT}"
