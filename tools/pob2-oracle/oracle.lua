-- PoB2 headless calculation oracle
--
-- Loads a build from a decoded PoB2 XML file, runs the full calc engine,
-- and dumps the player calc output (final + intermediate values) plus a set
-- of skillModList aggregate queries (conversion / gain-as-extra / increased /
-- more per damage type) as JSON to stdout.
--
-- This is a pure wrapper: it does NOT modify any vendor source. It is meant to
-- be run from the vendor src/ directory with LUA_PATH pointing at runtime/lua.
--
-- Usage (see run.sh / README.md):
--   cd vendor/PathOfBuilding-PoE2/src
--   LUA_PATH="../runtime/lua/?.lua;../runtime/lua/?/init.lua;./?.lua;;" CI=true \
--     luajit ../../../tools/pob2-oracle/oracle.lua <decoded.xml> [out.json]
--
-- Arg 1: path to decoded build XML (required)
-- Arg 2: output JSON path (optional; default stdout)
--
-- M6 Track C append-only sub-commands (parse-mod differential base). When arg[1]
-- is `--mode`, oracle.lua dispatches to a parse-mod handler and exits *before*
-- the calc path below — the original `<decoded.xml> [out.json]` calc oracle is
-- left byte-for-byte intact (only reached when no `--mode` flag is present):
--
--   oracle.lua --mode parsemod [--lines-file <path>]
--       Live differential: read modifier-text lines from stdin (or --lines-file),
--       run vendor `modLib.parseMod` (order=1 then order=2, matching the
--       ModParser.lua cache-closure double pass), emit one JSONL object per line:
--         {"line","mods":[{name,type,value,flags,keywordFlags,tags}],"unsupported",
--          "leftover"?}
--       (Equivalent to the M5b-D1 parsemod.lua wrapper; kept here so the roadmap's
--        "oracle.lua --mode parsemod" reference resolves against this one tool.)
--
--   oracle.lua --mode modcache-dump [--out <path>] [--limit <n>]
--       Offline golden: load vendor `Data/ModCache.lua` (PoB2's own pre-parsed
--       cache: key = mod text, value = { parsedModListOrNil, leftoverText }) and
--       emit the canonical golden JSON (schema = `modcache_golden.json`, §5.1 C3):
--         {"_meta":{...}, "entries":[{ "text", "status":"parsed|unsupported",
--          "mods":[...], "leftover" }]}
--       Values are normalized through the same name/type/flags/tags shape as the
--       parsemod handler so the Rust differential test compares like-for-like.
--       Self-contained once written — the Rust test never re-runs luajit.
--
-- Both sub-commands do NOT modify any vendor source.

----------------------------------------------------------------------
-- Sub-command dispatch (append-only; see header). Falls through to the
-- original calc oracle when arg[1] is not `--mode`.
----------------------------------------------------------------------
if arg[1] == "--mode" then
	local mode = arg[2]
	-- Parse the remaining flags (--lines-file / --out / --limit) positionally.
	local linesFile, dumpOut, dumpLimit
	do
		local i = 3
		while arg[i] do
			if arg[i] == "--lines-file" then
				linesFile = arg[i + 1]
				i = i + 2
			elseif arg[i] == "--out" then
				dumpOut = arg[i + 1]
				i = i + 2
			elseif arg[i] == "--limit" then
				dumpLimit = tonumber(arg[i + 1])
				i = i + 2
			else
				i = i + 1
			end
		end
	end

	-- Bootstrap headless PoB2 silently. We need modLib (parseMod) for the live
	-- mode and the ModFlag / KeywordFlag tables for bit-name reverse lookup in
	-- both modes; ModCache is loaded lazily inside the dump branch.
	local realPrint = print
	_G.print = function() end
	dofile("HeadlessWrapper.lua")
	_G.print = realPrint

	----------------------------------------------------------------------
	-- Shared JSON encoder (deterministic key order; same shape as the calc
	-- oracle's encoder below, duplicated here so the dispatch is self-contained
	-- and exits before that code is defined).
	----------------------------------------------------------------------
	local function isArray(t)
		local n = 0
		for k in pairs(t) do
			if type(k) ~= "number" then return false end
			n = n + 1
		end
		return n == #t
	end
	local function escape(s)
		return (s:gsub('[%z\1-\31\\"]', function(c)
			local map = { ['"'] = '\\"', ['\\'] = '\\\\', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }
			return map[c] or string.format('\\u%04x', c:byte())
		end))
	end
	local encode
	encode = function(v, depth)
		depth = depth or 0
		local t = type(v)
		if t == "number" then
			if v ~= v then return '"NaN"' end
			if v == math.huge then return '"Infinity"' end
			if v == -math.huge then return '"-Infinity"' end
			if v == math.floor(v) and math.abs(v) < 1e15 then
				return string.format("%d", v)
			end
			return string.format("%.10g", v)
		elseif t == "boolean" then
			return v and "true" or "false"
		elseif t == "string" then
			return '"' .. escape(v) .. '"'
		elseif t == "nil" then
			return "null"
		elseif t == "table" then
			if depth > 8 then return '"<max-depth>"' end
			if isArray(v) and #v > 0 then
				local parts = {}
				for i = 1, #v do parts[i] = encode(v[i], depth + 1) end
				return "[" .. table.concat(parts, ",") .. "]"
			end
			if next(v) == nil then return "[]" end
			local keys = {}
			for k in pairs(v) do keys[#keys + 1] = k end
			table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
			local parts = {}
			for _, k in ipairs(keys) do
				local val = v[k]
				local tv = type(val)
				if tv ~= "function" and tv ~= "userdata" and tv ~= "thread" then
					parts[#parts + 1] = '"' .. escape(tostring(k)) .. '":' .. encode(val, depth + 1)
				end
			end
			return "{" .. table.concat(parts, ",") .. "}"
		end
		return '"<' .. t .. '>"'
	end

	----------------------------------------------------------------------
	-- Bit-name reverse lookup for ModFlag / KeywordFlag (Global.lua tables).
	-- A bit name is included when the value fully covers its single-bit mask.
	----------------------------------------------------------------------
	local function flagNames(value, flagTable)
		local names = {}
		if not value or value == 0 or type(flagTable) ~= "table" then return names end
		for name, bit in pairs(flagTable) do
			if type(bit) == "number" and bit ~= 0 and bit % 1 == 0 and bit > 0 then
				if (value % (bit * 2)) >= bit then
					names[#names + 1] = name
				end
			end
		end
		table.sort(names)
		return names
	end

	----------------------------------------------------------------------
	-- Normalize one PoB2 mod table to the canonical differential shape:
	--   { name, type, value, flags?[], keywordFlags?[], tags?[] }
	-- tags are the positional m[1..n] sub-tables (each has a .type).
	----------------------------------------------------------------------
	local function normMod(m)
		local out = { name = m.name, type = m.type }
		local tv = type(m.value)
		if tv == "number" or tv == "boolean" or tv == "string" then
			out.value = m.value
		elseif tv == "table" then
			out.value = m.value -- nested LIST payload; encoder walks tables
		end
		if m.flags and m.flags ~= 0 then
			out.flags = flagNames(m.flags, ModFlag)
		end
		if m.keywordFlags and m.keywordFlags ~= 0 then
			out.keywordFlags = flagNames(m.keywordFlags, KeywordFlag)
		end
		local tags = {}
		for i = 1, #m do
			if type(m[i]) == "table" and m[i].type then
				tags[#tags + 1] = m[i]
			end
		end
		if #tags > 0 then out.tags = tags end
		return out
	end

	if mode == "parsemod" then
		assert(type(modLib) == "table" and type(modLib.parseMod) == "function",
			"headless did not expose modLib.parseMod")
		local input
		if linesFile then
			input = assert(io.open(linesFile, "r"), "cannot open lines file: " .. linesFile)
		else
			input = io.stdin
		end
		for raw in input:lines() do
			local line = raw:gsub("^%s+", ""):gsub("%s+$", "")
			if #line > 0 then
				-- modLib.parseMod returns (modList, leftover) matching the
				-- ModParser order=1/2 double-pass cache closure.
				local modList, leftover = modLib.parseMod(line)
				local mods = {}
				if modList then
					for _, m in ipairs(modList) do
						mods[#mods + 1] = normMod(m)
					end
				end
				local rec = {
					line = line,
					mods = mods,
					unsupported = (modList == nil) or (#mods == 0),
				}
				if type(leftover) == "string" and #leftover > 0 then
					rec.leftover = leftover
				end
				io.write(encode(rec), "\n")
			end
		end
		if linesFile then input:close() end
		os.exit(0)
	elseif mode == "modcache-dump" then
		-- Load vendor Data/ModCache.lua. It returns a function `local c=...`
		-- pattern: the chunk takes the cache table as its single argument and
		-- populates it. We pass a fresh table and read it back.
		local chunk = assert(loadfile("Data/ModCache.lua"),
			"cannot load Data/ModCache.lua (run from vendor src/)")
		local cache = {}
		chunk(cache)
		-- Collect + sort keys for byte-stable output.
		local texts = {}
		for text in pairs(cache) do texts[#texts + 1] = text end
		table.sort(texts)
		local entries = {}
		local parsedCount = 0
		for _, text in ipairs(texts) do
			if dumpLimit and #entries >= dumpLimit then break end
			local entry = cache[text]
			-- entry = { parsedModListOrNil, leftoverText }
			local modList = entry[1]
			local leftover = entry[2]
			local mods = {}
			if type(modList) == "table" then
				for _, m in ipairs(modList) do
					if type(m) == "table" and m.name then
						mods[#mods + 1] = normMod(m)
					end
				end
			end
			local status = (#mods > 0) and "parsed" or "unsupported"
			if status == "parsed" then parsedCount = parsedCount + 1 end
			local rec = { text = text, status = status, mods = mods }
			if type(leftover) == "string" then rec.leftover = leftover end
			entries[#entries + 1] = rec
		end
		local doc = {
			_meta = {
				source = "vendor Data/ModCache.lua",
				generator = "tools/pob2-oracle/oracle.lua --mode modcache-dump",
				total = #entries,
				parsed = parsedCount,
				unsupported = #entries - parsedCount,
			},
			entries = entries,
		}
		local json = encode(doc) .. "\n"
		if dumpOut and dumpOut ~= "" then
			local out = assert(io.open(dumpOut, "w"), "cannot open out file: " .. dumpOut)
			out:write(json)
			out:close()
			io.stderr:write(string.format(
				"modcache-dump: %d entries (%d parsed / %d unsupported) -> %s\n",
				#entries, parsedCount, #entries - parsedCount, dumpOut))
		else
			io.write(json)
		end
		os.exit(0)
	else
		io.stderr:write("oracle.lua: unknown --mode '" .. tostring(mode) ..
			"' (expected parsemod | modcache-dump)\n")
		os.exit(2)
	end
end

local xmlPath = arg[1]
local outPath = arg[2]
if outPath == "" then outPath = nil end -- run.sh passes "" when no out file requested
if not xmlPath then
	io.stderr:write("usage: oracle.lua <decoded.xml> [out.json]\n")
	os.exit(2)
end

-- Read the build XML before bootstrapping (cwd is vendor src/).
local f = assert(io.open(xmlPath, "r"), "cannot open build xml: " .. xmlPath)
local xmlText = f:read("*a")
f:close()

-- Bootstrap headless PoB2. Silence the very noisy image/tree load prints by
-- swallowing PoB's ConPrintf/print noise (image-not-found, tree load, etc.) for
-- the whole bootstrap + calc so stdout stays clean JSON. We keep a real handle
-- to emit the final report. ConPrintf calls print(string.format(...)), so
-- silencing print covers both.
local realPrint = print
_G.print = function() end
dofile("HeadlessWrapper.lua")

assert(type(loadBuildFromXML) == "function", "headless did not expose loadBuildFromXML")

-- Load and calculate. SetMode("BUILD", ..., xmlText) + OnFrame parses, builds
-- the spec and runs the first calc pass. We then force a fresh BuildOutput so
-- the CALCS env (which carries breakdowns) is fully populated.
loadBuildFromXML(xmlText, "oracle")
build.calcsTab:BuildOutput()

local mainOutput = build.calcsTab.mainOutput        -- == mainEnv.player.output
local calcsOutput = build.calcsTab.calcsOutput      -- == calcsEnv.player.output (has breakdowns)
local player = build.calcsTab.mainEnv.player
local mainSkill = player.mainSkill
-- CALCS env carries actor.breakdown (per-type damage chain: base / inc / more / conv)
local calcsPlayer = build.calcsTab.calcsEnv.player
local calcsBreakdown = calcsPlayer.breakdown

----------------------------------------------------------------------
-- Minimal JSON encoder (deterministic key order, handles number/string/
-- bool/nil/table). Avoids pulling dkjson formatting quirks for our shape.
----------------------------------------------------------------------
local function isArray(t)
	local n = 0
	for k in pairs(t) do
		if type(k) ~= "number" then return false end
		n = n + 1
	end
	return n == #t
end

local function escape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ['\\'] = '\\\\', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }
		return map[c] or string.format('\\u%04x', c:byte())
	end))
end

local encode
encode = function(v, depth)
	depth = depth or 0
	local t = type(v)
	if t == "number" then
		if v ~= v then return '"NaN"' end
		if v == math.huge then return '"Infinity"' end
		if v == -math.huge then return '"-Infinity"' end
		-- keep full precision
		if v == math.floor(v) and math.abs(v) < 1e15 then
			return string.format("%d", v)
		end
		return string.format("%.10g", v)
	elseif t == "boolean" then
		return v and "true" or "false"
	elseif t == "string" then
		return '"' .. escape(v) .. '"'
	elseif t == "nil" then
		return "null"
	elseif t == "table" then
		if depth > 6 then return '"<max-depth>"' end
		if isArray(v) and #v > 0 then
			local parts = {}
			for i = 1, #v do parts[i] = encode(v[i], depth + 1) end
			return "[" .. table.concat(parts, ",") .. "]"
		end
		-- sort keys for determinism
		local keys = {}
		for k in pairs(v) do keys[#keys + 1] = k end
		table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
		local parts = {}
		for _, k in ipairs(keys) do
			local val = v[k]
			local tv = type(val)
			if tv ~= "function" and tv ~= "userdata" and tv ~= "thread" then
				parts[#parts + 1] = '"' .. escape(tostring(k)) .. '":' .. encode(val, depth + 1)
			end
		end
		return "{" .. table.concat(parts, ",") .. "}"
	end
	return '"<' .. t .. '>"'
end

----------------------------------------------------------------------
-- Flatten the player output table to scalars only (numbers / strings / bools).
-- Nested tables in output are breakdown-ish; we capture scalars here and pull
-- targeted breakdowns separately.
----------------------------------------------------------------------
local function scalarsOf(tbl)
	local out = {}
	for k, val in pairs(tbl) do
		local tv = type(val)
		if tv == "number" or tv == "string" or tv == "boolean" then
			out[tostring(k)] = val
		end
	end
	return out
end

----------------------------------------------------------------------
-- skillModList aggregate queries. cfg lets us scope by skill flags. We query
-- the main skill's full mod list (post-support, post-gem) which is what drives
-- per-hit damage.
----------------------------------------------------------------------
local sml = mainSkill and mainSkill.skillModList
local damageTypes = { "Physical", "Lightning", "Cold", "Fire", "Chaos" }

local function smlSum(modType, cfg, ...)
	if not sml then return nil end
	local ok, res = pcall(function(...) return sml:Sum(modType, cfg, ...) end, ...)
	if ok then return res end
	return nil
end
local function smlMore(cfg, ...)
	if not sml then return nil end
	local ok, res = pcall(function(...) return sml:More(cfg, ...) end, ...)
	if ok then return res end
	return nil
end

local skillCfg = mainSkill and mainSkill.skillCfg or nil

local intermediates = {}

-- Global increased/more damage
intermediates.IncDamage = smlSum("INC", skillCfg, "Damage")
intermediates.MoreDamage = smlMore(skillCfg, "Damage")

-- Damaging-ailment magnitude breakdown (the `<Ailment>MagnitudeEffect` factor =
-- calcLib.mod(skillModList, dotCfg, "AilmentMagnitude"), CalcOffence.lua:5145).
-- Uses the exact per-ailment dotCfg PoB2 stashes on the active skill
-- (activeSkill["poisonCfg"] etc., :5010) so inc/more match the engine's value.
for _, ail in ipairs({ "poison", "bleed", "ignite" }) do
	local dotCfg = mainSkill and mainSkill[ail .. "Cfg"]
	if dotCfg then
		local cap = ail:sub(1, 1):upper() .. ail:sub(2)
		intermediates["IncAilmentMagnitude_" .. cap] = smlSum("INC", dotCfg, "AilmentMagnitude")
		intermediates["MoreAilmentMagnitude_" .. cap] = smlMore(dotCfg, "AilmentMagnitude")
	end
end

-- Per damage-type: increased, more, conversion-out, gain-as-extra
for _, dt in ipairs(damageTypes) do
	local lower = dt:lower()
	intermediates["Inc_" .. dt .. "Damage"] = smlSum("INC", skillCfg, dt .. "Damage")
	intermediates["More_" .. dt .. "Damage"] = smlMore(skillCfg, dt .. "Damage")
	-- conversion: "X% of <type> converted to <other>" stored as <type>DamageConvertTo<Other>
	for _, dt2 in ipairs(damageTypes) do
		if dt ~= dt2 then
			local conv = smlSum("BASE", skillCfg, dt .. "DamageConvertTo" .. dt2)
			if conv and conv ~= 0 then
				intermediates["Convert_" .. dt .. "To" .. dt2] = conv
			end
		end
	end
	-- gain as extra: <type>DamageGainAs<Other> and Skill variant
	for _, dt2 in ipairs(damageTypes) do
		local gain = smlSum("BASE", skillCfg, dt .. "DamageGainAs" .. dt2)
		if gain and gain ~= 0 then
			intermediates["GainAs_" .. dt .. "As" .. dt2] = gain
		end
		local gainSkill = smlSum("BASE", skillCfg, "Skill" .. dt .. "DamageGainAs" .. dt2)
		if gainSkill and gainSkill ~= 0 then
			intermediates["SkillGainAs_" .. dt .. "As" .. dt2] = gainSkill
		end
	end
end

-- Un-prefixed "Gain X% of Damage as Extra <To>" => DamageGainAs<To> BASE.
-- Also Skill-scoped and per-source forms already covered above.
for _, dt2 in ipairs(damageTypes) do
	local g = smlSum("BASE", skillCfg, "DamageGainAs" .. dt2)
	if g and g ~= 0 then intermediates["DamageGainAs_" .. dt2] = g end
	local gs = smlSum("BASE", skillCfg, "SkillDamageGainAs" .. dt2)
	if gs and gs ~= 0 then intermediates["SkillDamageGainAs_" .. dt2] = gs end
	-- elemental / nonchaos source forms
	local ge = smlSum("BASE", skillCfg, "ElementalDamageGainAs" .. dt2)
	if ge and ge ~= 0 then intermediates["ElementalDamageGainAs_" .. dt2] = ge end
	local gn = smlSum("BASE", skillCfg, "NonChaosDamageGainAs" .. dt2)
	if gn and gn ~= 0 then intermediates["NonChaosDamageGainAs_" .. dt2] = gn end
end

----------------------------------------------------------------------
-- Enemy mitigation intermediates (M4-H effective enemy-multiplier line).
-- Mirrors the CalcOffence.lua:4060-4170 enemy resist/armour/penetration
-- segment so PoBR's offence::enemy_damage_multiplier can be diffed per
-- sub-segment (resist base / cap / pen / taken chain / armour+PDR).
----------------------------------------------------------------------
local mainEnv = build.calcsTab.mainEnv
local enemyDB = mainEnv.player.enemy and mainEnv.player.enemy.modDB
local enemyMitigation = nil
if enemyDB then
	local function edbSum(modType, cfg, ...)
		local ok, res = pcall(function(...) return enemyDB:Sum(modType, cfg, ...) end, ...)
		if ok then return res end
		return nil
	end
	local function edbMore(cfg, ...)
		local ok, res = pcall(function(...) return enemyDB:More(cfg, ...) end, ...)
		if ok then return res end
		return nil
	end
	local function edbOverride(cfg, name)
		local ok, res = pcall(function() return enemyDB:Override(cfg, name) end)
		if ok then return res end
		return nil
	end
	enemyMitigation = {}
	for _, dt in ipairs({ "Fire", "Cold", "Lightning", "Chaos" }) do
		local isEle = dt ~= "Chaos"
		enemyMitigation[dt] = {
			resistBase = edbSum("BASE", skillCfg, dt .. "Resist", isEle and "ElementalResist" or nil),
			resistInc = edbSum("INC", skillCfg, dt .. "Resist", isEle and "ElementalResist" or nil),
			resistMore = edbMore(skillCfg, dt .. "Resist", isEle and "ElementalResist" or nil),
			resistOverride = edbOverride(skillCfg, dt .. "Resist"),
			configMaxResist = mainEnv.configInput and mainEnv.configInput["enemy" .. dt .. "Resist"] or nil,
			pen = smlSum("BASE", skillCfg, isEle and (dt .. "Penetration") or "ChaosPenetration",
				isEle and "ElementalPenetration" or nil),
			minPen = smlSum("BASE", skillCfg, isEle and (dt .. "PenetrationMinimum") or nil,
				isEle and "ElementalPenetrationMinimum" or nil),
			takenInc = edbSum("INC", skillCfg, "DamageTaken", dt .. "DamageTaken"),
			takenMore = edbMore(skillCfg, "DamageTaken", dt .. "DamageTaken"),
			eleTakenInc = isEle and edbSum("INC", skillCfg, "ElementalDamageTaken") or nil,
		}
	end
	enemyMitigation.Physical = {
		armour = edbSum("BASE", nil, "Armour"),
		armourOverride = edbOverride(nil, "Armour"),
		flatPDR = edbSum("BASE", nil, "PhysicalDamageReduction"),
		overwhelm = smlSum("BASE", skillCfg, "EnemyPhysicalDamageReduction"),
		takenInc = edbSum("INC", skillCfg, "DamageTaken", "PhysicalDamageTaken"),
		takenMore = edbMore(skillCfg, "DamageTaken", "PhysicalDamageTaken"),
	}
	enemyMitigation.projTakenInc = edbSum("INC", nil, "ProjectileDamageTaken")
	enemyMitigation.projAttackTakenInc = edbSum("INC", nil, "ProjectileAttackDamageTaken")
	-- Per-mod source dump of the enemy DamageTaken chain (which build sources feed
	-- takenInc, e.g. curses / "enemies take increased damage" passives).
	local takenMods = {}
	local okTab, takenTable = pcall(function()
		return enemyDB:Tabulate("INC", skillCfg, "DamageTaken", "PhysicalDamageTaken",
			"FireDamageTaken", "ColdDamageTaken", "LightningDamageTaken", "ChaosDamageTaken",
			"ElementalDamageTaken")
	end)
	if okTab and takenTable then
		for _, entry in ipairs(takenTable) do
			takenMods[#takenMods + 1] = {
				name = entry.mod.name,
				value = entry.value,
				source = entry.mod.source,
			}
		end
	end
	enemyMitigation.takenMods = takenMods
	-- Per-mod source dump of enemy resist contributions (curse / exposure / boss
	-- preset), so the resist-final composition can be diffed segment by segment.
	local resistMods = {}
	local okRes, resTable = pcall(function()
		return enemyDB:Tabulate("BASE", skillCfg, "FireResist", "ColdResist", "LightningResist",
			"ChaosResist", "ElementalResist")
	end)
	if okRes and resTable then
		for _, entry in ipairs(resTable) do
			resistMods[#resistMods + 1] = {
				name = entry.mod.name,
				value = entry.value,
				source = entry.mod.source,
			}
		end
	end
	enemyMitigation.resistMods = resistMods
	-- Exposure effect composition (vendor CalcPerform.lua:3222-3227): global modDB
	-- INC + per-active-skill skill-scoped INC, plus extra exposure BASE.
	local pdb = mainEnv.player.modDB
	local expo = {}
	for _, el in ipairs({ "Fire", "Cold", "Lightning" }) do
		local okG, g = pcall(function() return pdb:Sum("INC", nil, el .. "ExposureEffect") end)
		local okE, extra = pcall(function() return pdb:Sum("BASE", nil, "ExtraExposure", "Extra" .. el .. "Exposure") end)
		local skillEff = 0
		for _, as in ipairs(mainEnv.player.activeSkillList or {}) do
			local okS, sv = pcall(function() return as.skillModList:Sum("INC", { source = "Skill" }, el .. "ExposureEffect") end)
			if okS and sv and sv > skillEff then skillEff = sv end
		end
		expo[el] = { globalInc = okG and g or nil, skillInc = skillEff, extra = okE and extra or nil }
	end
	enemyMitigation.exposureEffect = expo
end

-- Crit, speed, hit chance aggregates
intermediates.IncCritChance = smlSum("INC", skillCfg, "CritChance")
intermediates.BaseCritChance = smlSum("BASE", skillCfg, "CritChance")
intermediates.IncCritMultiplier = smlSum("INC", skillCfg, "CritMultiplier")
intermediates.BaseCritMultiplier = smlSum("BASE", skillCfg, "CritMultiplier")
intermediates.IncSpeed = smlSum("INC", skillCfg, "Speed")
intermediates.IncCastSpeed = smlSum("INC", skillCfg, "Speed", "CastSpeed")
intermediates.IncAttackSpeed = smlSum("INC", skillCfg, "Speed", "AttackSpeed")

----------------------------------------------------------------------
-- Per-mod damage list dump. Tabulate every individual mod that contributes
-- to Sum("INC", cfg, <name>) / More(cfg, <name>) for the generic + per-type
-- damage names that drive per-hit damage. This is the diagnostic surface for
-- the "PoBR is missing ~150-220 increased damage" gap: each entry is a
-- {name, type, value, source, flags, keywordFlags, tags, applies} record so we
-- can diff PoB2 vs PoBR mod-by-mod and classify each missing mod as
-- (a) not ingested, (b) ingested but condition not defaulted true, (c) parse/scope.
--
-- tags carries the condition / multiplier tagList (mod[1..n]); a mod with a
-- Condition/Multiplier/ActorCondition tag is the (b) class candidate — PoB2
-- defaults the condition true via ConfigOptions, PoBR may not.
----------------------------------------------------------------------

-- modLib is a global in PoB; formatTags/formatFlags render the tagList & flags.
local function flagsStr(flags, src)
	if not modLib or not flags then return nil end
	local ok, s = pcall(modLib.formatFlags, flags, src)
	if ok and s ~= "-" then return s end
	return nil
end

-- Render the tagList (integer-indexed entries on the mod) to a flat array of
-- strings, and separately surface Condition/Multiplier var names for class (b).
local function tagInfo(mod)
	local tags = {}
	local condVars = {}
	for i, tag in ipairs(mod) do
		if type(tag) == "table" then
			local ok, s = pcall(modLib.formatTag, tag)
			if ok and s and s ~= "" then tags[#tags + 1] = s end
			-- surface the condition/multiplier var so stage 2 knows what to default
			if tag.type == "Condition" and tag.var then
				condVars[#condVars + 1] = "Condition:" .. tostring(tag.var)
			elseif tag.type == "ActorCondition" and tag.var then
				condVars[#condVars + 1] = "ActorCondition:" .. tostring(tag.var)
			elseif tag.type == "Multiplier" and tag.var then
				condVars[#condVars + 1] = "Multiplier:" .. tostring(tag.var)
			elseif tag.type == "MultiplierThreshold" and tag.var then
				condVars[#condVars + 1] = "MultiplierThreshold:" .. tostring(tag.var)
			elseif tag.type == "PerStat" and tag.stat then
				condVars[#condVars + 1] = "PerStat:" .. tostring(tag.stat)
			elseif tag.type == "StatThreshold" and tag.stat then
				condVars[#condVars + 1] = "StatThreshold:" .. tostring(tag.stat)
			end
		end
	end
	return tags, condVars
end

-- Tabulate one (modType, name) over the main skill modList. Returns the array
-- of per-mod records. evalValue is the post-EvalMod value PoB actually uses
-- (so conditional mods that fail evaluate to 0 / are dropped by Tabulate).
local function tabulateModList(modType, name)
	local out = {}
	if not sml then return out end
	local ok, modTable = pcall(function() return sml:Tabulate(modType, skillCfg, name) end)
	if not ok or not modTable then return out end
	for _, entry in ipairs(modTable) do
		local mod = entry.mod
		local tags, condVars = tagInfo(mod)
		out[#out + 1] = {
			queryName = name,
			name = mod.name,
			type = mod.type,
			-- mod.value is the raw declared value (table-valued mods become a string)
			rawValue = (type(mod.value) == "number") and mod.value or modLib.formatValue(mod.value),
			evalValue = entry.value, -- post-EvalMod contribution actually counted
			source = mod.source,
			flags = flagsStr(mod.flags, ModFlag),
			keywordFlags = flagsStr(mod.keywordFlags, KeywordFlag),
			tags = (#tags > 0) and tags or nil,
			condVars = (#condVars > 0) and condVars or nil,
			conditional = (#condVars > 0) or nil,
		}
	end
	return out
end

-- Names that feed the generic + per-type increased/more damage chain in PoBR's
-- aggregate_inc_more: generic (Damage + Attack/Spell/Projectile/Area/Melee +
-- weapon/keyword-derived), per-type <Type>Damage, and ElementalDamage.
local damageNames = {
	"Damage",
	"AttackDamage", "SpellDamage", "ProjectileDamage", "AreaDamage", "MeleeDamage",
	"PhysicalDamage", "LightningDamage", "ColdDamage", "FireDamage", "ChaosDamage",
	"ElementalDamage",
	-- common weapon/keyword-derived damage scaling names (PoB2 Data)
	"BowDamage", "CrossbowDamage", "GrenadeDamage", "StaffDamage", "MaceDamage",
	"SwordDamage", "AxeDamage", "ClawDamage", "DaggerDamage", "WandDamage",
	"UnarmedDamage", "SpellSkillDamage",
}

local damageModList = { INC = {}, MORE = {} }
for _, name in ipairs(damageNames) do
	local incList = tabulateModList("INC", name)
	if #incList > 0 then damageModList.INC[name] = incList end
	local moreList = tabulateModList("MORE", name)
	if #moreList > 0 then damageModList.MORE[name] = moreList end
end

-- Aggregate cross-check: Sum/More over the same names PoBR uses, so the JSON
-- carries the authoritative PoB2 totals alongside the per-mod breakdown.
local damageAgg = {
	IncDamageGeneric = smlSum("INC", skillCfg, "Damage"),
	IncAttackDamage = smlSum("INC", skillCfg, "AttackDamage"),
	IncSpellDamage = smlSum("INC", skillCfg, "SpellDamage"),
	IncProjectileDamage = smlSum("INC", skillCfg, "ProjectileDamage"),
	IncAreaDamage = smlSum("INC", skillCfg, "AreaDamage"),
	IncMeleeDamage = smlSum("INC", skillCfg, "MeleeDamage"),
}

----------------------------------------------------------------------
-- Per-component damage min/max/avg pulled directly from output (written by
-- CalcOffence as output[Type.."Min"/"Max"/"HitAverage"]).
----------------------------------------------------------------------
local components = {}
for _, dt in ipairs(damageTypes) do
	components[dt] = {
		Min = mainOutput[dt .. "Min"],
		Max = mainOutput[dt .. "Max"],
		HitAverage = mainOutput[dt .. "HitAverage"],
	}
end

----------------------------------------------------------------------
-- Skill metadata (level, active gem level, supports) for sanity-checking the
-- skill is at the expected level — critical for the deadeye +6 question.
----------------------------------------------------------------------
local skillInfo = {}
if mainSkill then
	local ae = mainSkill.activeEffect
	if ae then
		skillInfo.activeGemName = ae.grantedEffect and ae.grantedEffect.name
		skillInfo.activeGemLevel = ae.level
		skillInfo.activeGemQuality = ae.quality
		if ae.grantedEffect and ae.grantedEffect.id then
			skillInfo.grantedEffectId = ae.grantedEffect.id
		end
	end
	skillInfo.skillFlags = {}
	if mainSkill.skillFlags then
		for k, vv in pairs(mainSkill.skillFlags) do
			if vv then skillInfo.skillFlags[k] = true end
		end
	end
	-- skillData.dpsMultiplier (post calcLib.mod "DPS" fold, CalcOffence.lua:3863) for
	-- the W-C4 dps end-factor parity checks.
	if mainSkill.skillData then
		skillInfo.dpsMultiplier = mainSkill.skillData.dpsMultiplier
	end
	-- support gems and their levels
	skillInfo.supports = {}
	if mainSkill.supportList then
		for _, sup in ipairs(mainSkill.supportList) do
			skillInfo.supports[#skillInfo.supports + 1] = {
				name = sup.grantedEffect and sup.grantedEffect.name,
				level = sup.level,
			}
		end
	end
end

----------------------------------------------------------------------
-- Per-type summed base damage (input to the inc/more chain) from output.
-- CalcOffence writes output[Type.."SummedMinBase"/"SummedMaxBase"].
----------------------------------------------------------------------
local summedBase = {}
for _, dt in ipairs(damageTypes) do
	summedBase[dt] = {
		SummedMinBase = mainOutput[dt .. "SummedMinBase"],
		SummedMaxBase = mainOutput[dt .. "SummedMaxBase"],
	}
end

----------------------------------------------------------------------
-- Pull the damage-type breakdown rows from CALCS env actor.breakdown.
-- breakdown.damageTypes = { {source, base, inc, more, convSrc, total, convDst}, ... }
-- These are the per-type base / inc / more / conversion intermediates.
----------------------------------------------------------------------
local damageTypeBreakdown = nil
if calcsBreakdown and calcsBreakdown.damageTypes then
	damageTypeBreakdown = {}
	for i, row in ipairs(calcsBreakdown.damageTypes) do
		damageTypeBreakdown[i] = {
			source = row.source,
			base = row.base,
			inc = row.inc,
			more = row.more,
			convSrc = row.convSrc,
			total = row.total,
			convDst = row.convDst,
		}
	end
end

-- conversionTable (resolved fractions per type) from the main skill.
local conversionTable = nil
if mainSkill and mainSkill.conversionTable then
	conversionTable = {}
	for src, tbl in pairs(mainSkill.conversionTable) do
		if type(tbl) == "table" then
			local row = {}
			for dst, frac in pairs(tbl) do
				if type(frac) == "number" and frac ~= 0 then row[tostring(dst)] = frac end
			end
			if next(row) then conversionTable[tostring(src)] = row end
		end
	end
end

----------------------------------------------------------------------
-- Assemble report
----------------------------------------------------------------------
local report = {
	mainOutput = scalarsOf(mainOutput),
	calcsOutput = scalarsOf(calcsOutput),
	-- Per-hand attack pass outputs (CalcOffence.lua:2371 output.MainHand = {}):
	-- DoubleDamageChance / TripleDamageChance / ScaledDamageEffect / per-pass
	-- CritChance live here for attacks (M4-T3 W-C1 oracle parity).
	mainHandOutput = type(mainOutput.MainHand) == "table" and scalarsOf(mainOutput.MainHand) or nil,
	offHandOutput = type(mainOutput.OffHand) == "table" and scalarsOf(mainOutput.OffHand) or nil,
	-- CALCS-mode per-hand outputs: attack-path <Type>EffMult is written into the
	-- per-pass output table (output.MainHand) under env.mode == "CALCS" only.
	calcsMainHandOutput = type(calcsOutput.MainHand) == "table" and scalarsOf(calcsOutput.MainHand) or nil,
	calcsOffHandOutput = type(calcsOutput.OffHand) == "table" and scalarsOf(calcsOutput.OffHand) or nil,
	intermediates = intermediates,
	damageModList = damageModList,
	damageAgg = damageAgg,
	components = components,
	summedBase = summedBase,
	damageTypeBreakdown = damageTypeBreakdown,
	conversionTable = conversionTable,
	skillInfo = skillInfo,
	enemyMitigation = enemyMitigation,
}

local json = encode(report)
if outPath then
	local of = assert(io.open(outPath, "w"))
	of:write(json)
	of:close()
	io.stderr:write("wrote " .. outPath .. "\n")
else
	realPrint(json)
end
