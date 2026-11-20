-- PoB2 headless oracle: cross-check modList after SkillStatMap injection (M1-T2b, blueprint T2.3 oracle sampling).
--
-- For each `(effectId, stat, value)` probe, invokes PoB2's real `calcs.mergeSkillInstanceMods`
-- (`Modules/CalcActiveSkill.lua:82`) with real `statMap` data (`Data/SkillStatMap.lua` global table /
-- `Data/Skills/*.lua` per-statSet overrides via metatable chain), dumping the injected modList
-- (PoB2 raw name, type, merged value, flags, keywordFlags, tags) as JSON. Values inject via
-- `extraStats`—the synthetic statSet has no own stats (`buildSkillInstanceStats` returns empty),
-- so the merged result isolates that single stat's mapped output for per-stat attribution.
--
-- `effectId = "GLOBAL"` probes the global `data.skillStatMap`; otherwise uses
-- `data.skills[effectId].statSets[1].statMap` (default set, matching PoBR's `set_key=None -> "1"`;
-- vendor `SkillsTab.lua:354` defaults `statSetIndex=1`).
--
-- Pure wrapper: modifies no vendor source. Run from `vendor/PathOfBuilding-PoE2/src` (like `oracle.lua`):
--   cd vendor/PathOfBuilding-PoE2/src
--   LUA_PATH="../runtime/lua/?.lua;../runtime/lua/?/init.lua;./?.lua;;" CI=true \
--     luajit ../../../tools/pob2-oracle/statmap_oracle.lua <effectId> <stat> <value> [...]
--
-- Arguments repeat `(effectId, stat, value)` triples; JSON array outputs to stdout.

if #arg < 3 or #arg % 3 ~= 0 then
	io.stderr:write("usage: statmap_oracle.lua <effectId|GLOBAL> <stat> <value> [...]\n")
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

-- `calcs` is module-local (`Modules/Calcs.lua:13-17` pattern)—load `CalcActiveSkill` into a
-- private table (`LoadModule` is idempotent and registers pure functions without mutating global state).
local calcs = {}
LoadModule("Modules/CalcActiveSkill", calcs)
assert(calcs.mergeSkillInstanceMods, "calcs.mergeSkillInstanceMods not loaded")
assert(type(data) == "table" and data.skillStatMap, "data.skillStatMap not loaded")

local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

-- Generic value serializer (`number` / `string` / `boolean` / nested table; sorts string keys and
-- preserves array order for deterministic output).
local function encodeValue(v, depth)
	depth = depth or 0
	if depth > 6 then return '"<depth>"' end
	local t = type(v)
	if t == "number" then
		return string.format("%.17g", v)
	elseif t == "boolean" then
		return tostring(v)
	elseif t == "string" then
		return '"' .. jsonEscape(v) .. '"'
	elseif t == "table" then
		local parts = {}
		-- Array part.
		local arr = {}
		for i, item in ipairs(v) do
			arr[#arr + 1] = encodeValue(item, depth + 1)
		end
		if #arr > 0 then
			parts[#parts + 1] = '"_list":[' .. table.concat(arr, ",") .. "]"
		end
		-- String key part (sorted).
		local keys = {}
		for k in pairs(v) do
			if type(k) == "string" then keys[#keys + 1] = k end
		end
		table.sort(keys)
		for _, k in ipairs(keys) do
			parts[#parts + 1] = '"' .. jsonEscape(k) .. '":' .. encodeValue(v[k], depth + 1)
		end
		return "{" .. table.concat(parts, ",") .. "}"
	end
	return '"<' .. t .. '>"'
end

-- Serialize an injected mod: name, aggregation type, merged value, flags, keywordFlags, and tags
-- (tags form the array portion of the mod table).
local function encodeMod(m)
	local tags = {}
	for _, tag in ipairs(m) do
		tags[#tags + 1] = encodeValue(tag, 1)
	end
	return string.format(
		'{"name":"%s","type":"%s","value":%s,"flags":%.17g,"keywordFlags":%.17g,"tags":[%s]}',
		jsonEscape(m.name or "?"), jsonEscape(m.type or "?"), encodeValue(m.value),
		m.flags or 0, m.keywordFlags or 0, table.concat(tags, ","))
end

-- Single probe: build a synthetic statSet (inheriting the real `statMap` with no own stats/baseMods)
-- and inject controlled `(stat, value)` via `extraStats` into the real `mergeSkillInstanceMods`.
local function probeOne(effectId, stat, value)
	local statMap
	if effectId == "GLOBAL" then
		statMap = data.skillStatMap
	else
		local ge = data.skills[effectId]
		assert(ge, "unknown effect id: " .. tostring(effectId))
		local set = ge.statSets and ge.statSets[1]
		assert(set, "effect has no statSets[1]: " .. effectId)
		statMap = set.statMap
	end
	local synthSet = {
		statMap = statMap,
		baseMods = {},
		stats = {},
		constantStats = {},
		levels = { [1] = {} },
	}
	local synthGE = { id = effectId, name = effectId, statSets = { synthSet }, levels = { [1] = {} } }
	local skillEffect = { grantedEffect = synthGE, level = 1, quality = 0, actorLevel = 90 }
	local modList = new("ModList")
	calcs.mergeSkillInstanceMods(nil, modList, skillEffect, synthSet,
		{ { key = stat, value = value } })
	return modList
end

local out = { "[" }
local first = true
for i = 1, #queries, 3 do
	local effectId, stat, value = queries[i], queries[i + 1], tonumber(queries[i + 2])
	local ok, result = pcall(probeOne, effectId, stat, value)
	local modsJson = {}
	local errText
	if ok then
		for _, m in ipairs(result) do
			modsJson[#modsJson + 1] = encodeMod(m)
		end
	else
		errText = tostring(result)
	end
	out[#out + 1] = (first and "" or ",")
		.. string.format(
			'{"effect":"%s","stat":"%s","value":%.17g,"mods":[%s]%s}',
			jsonEscape(effectId), jsonEscape(stat), value,
			table.concat(modsJson, ","),
			errText and (',"error":"' .. jsonEscape(errText) .. '"') or "")
	first = false
end
out[#out + 1] = "]"
realPrint(table.concat(out, "\n"))
