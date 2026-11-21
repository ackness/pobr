-- PoB2 headless oracle: cross-check `buildSkillInstanceStats` quality scaling contributions (M1-T1).
--
-- For a given `(skillId, level, quality)`, runs `calcLib.buildSkillInstanceStats`
-- (`Modules/CalcTools.lua:138`) against real PoB2 data at `quality=Q` and `quality=0`,
-- emitting the per-stat difference (the quality contribution expected from PoBR's
-- `BuildData::effect_stats(..).quality`).
--
-- Pure wrapper: modifies no vendor source. Run from `vendor/PathOfBuilding-PoE2/src` (like `oracle.lua`):
--   cd vendor/PathOfBuilding-PoE2/src
--   LUA_PATH="../runtime/lua/?.lua;../runtime/lua/?/init.lua;./?.lua;;" CI=true \
--     luajit ../../../tools/pob2-oracle/quality_stats.lua <skillId> <level> <quality> [...]
--
-- Arguments repeat `(skillId, level, quality)` triples; JSON outputs to stdout.

if #arg < 3 or #arg % 3 ~= 0 then
	io.stderr:write("usage: quality_stats.lua <skillId> <level> <quality> [<skillId> <level> <quality> ...]\n")
	os.exit(2)
end

-- HeadlessWrapper startup may mutate global `arg`; snapshot it first.
local queries = {}
for i = 1, #arg do
	queries[i] = arg[i]
end

-- Silence headless startup noise (same pattern as `oracle.lua`).
local realPrint = print
_G.print = function() end
dofile("HeadlessWrapper.lua")
_G.print = realPrint

assert(type(calcLib) == "table" and calcLib.buildSkillInstanceStats, "calcLib not loaded")
assert(type(data) == "table" and data.skills, "data.skills not loaded")

local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

local out = { "[" }
local first = true
for i = 1, #queries, 3 do
	local skillId, level, quality = queries[i], tonumber(queries[i + 1]), tonumber(queries[i + 2])
	local grantedEffect = data.skills[skillId]
	assert(grantedEffect, "unknown skill id: " .. tostring(skillId))
	local statSet = grantedEffect.statSets[1]
	assert(statSet, "skill has no statSets[1]: " .. skillId)
	local withQ = calcLib.buildSkillInstanceStats(
		{ level = level, quality = quality, actorLevel = 90 }, grantedEffect, statSet)
	local without = calcLib.buildSkillInstanceStats(
		{ level = level, quality = 0, actorLevel = 90 }, grantedEffect, statSet)
	-- Per-stat delta (quality contribution); sort by stat name for deterministic output.
	local keys = {}
	for stat in pairs(withQ) do keys[#keys + 1] = stat end
	table.sort(keys)
	local parts = {}
	for _, stat in ipairs(keys) do
		local diff = (withQ[stat] or 0) - (without[stat] or 0)
		if diff ~= 0 then
			parts[#parts + 1] = string.format('"%s":%.17g', jsonEscape(stat), diff)
		end
	end
	out[#out + 1] = (first and "" or ",")
		.. string.format(
			'{"skill":"%s","level":%d,"quality":%d,"quality_contribution":{%s}}',
			jsonEscape(skillId), level, quality, table.concat(parts, ","))
	first = false
end
out[#out + 1] = "]"
realPrint(table.concat(out, "\n"))
