# PoB2 Headless Oracle

Runs PoB2's real Lua calc engine headless to produce **golden** numbers for parity
verification. Validated: deadeye `AverageDamage = 88060.086` matches the build code's
embedded `<PlayerStat>` exactly.

## One-time setup (vendor is a partial checkout)

From `vendor/PathOfBuilding-PoE2/`:

```bash
# 1. Lua deps from the bundled win32 runtime zip
mkdir -p runtime && unzip -o -q runtime-win32.zip "lua/*" -d runtime/

# 2. lua-utf8 shim (ASCII-safe; calc doesn't depend on real utf8 — only number formatting)
cat > runtime/lua/lua-utf8.lua <<'SHIM'
local u={} for k,v in pairs(string) do u[k]=v end
u.len=function(s) return #s end
u.offset=function(_,n) return n end
u.charpattern="[%z\1-\127\194-\244][\128-\191]*"
return u
SHIM

# 3. Missing data files (fetch from the community repo; needs gh)
mkdir -p src/TreeData/0_5 src/Data/TimelessJewelData
gh api repos/PathOfBuildingCommunity/PathOfBuilding-PoE2/contents/src/TreeData/0_5/tree.lua \
  -H "Accept: application/vnd.github.raw" > src/TreeData/0_5/tree.lua
gh api repos/PathOfBuildingCommunity/PathOfBuilding-PoE2/contents/src/Data/TimelessJewelData/LegionPassives.lua \
  -H "Accept: application/vnd.github.raw" > src/Data/TimelessJewelData/LegionPassives.lua
gh api repos/PathOfBuildingCommunity/PathOfBuilding-PoE2/contents/runtime/lua/sha2.lua \
  -H "Accept: application/vnd.github.raw" > runtime/lua/sha2.lua
```

## Run

```bash
cd vendor/PathOfBuilding-PoE2/src
BUILD=/abs/path/to/build.xml luajit \
  -e "package.path=package.path..';../runtime/lua/?.lua;../runtime/lua/?/init.lua'; dofile('../../../devs/tools/pob2-oracle/oracle.lua')" \
  </dev/null 2>&1 | grep '^@@'
```

Get a build XML from a PoB code: `cargo run -p pobr-cli -- decode-code "$(cat build.txt)" > build.xml`.

## Per-type base/conversion breakdown (deeper)

`oracle.lua` reports per-type **final** hits (`StoredCombinedAvg`) non-invasively. For the
**base → converted → gained → ×increase** stages, temporarily instrument
`src/Modules/CalcOffence.lua` after `output[damageType.."SummedMaxBase"] = ...` and after
`damageTypeHitMin, damageTypeHitMax = calcDamage(...)` with `io.stderr:write(...)` guarded by
`os.getenv("POBDBG")`, then restore. (That's how the deadeye lightning decomposition
`summedBase 11753 × mult 5.67 = 66658` was obtained.)
