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
	intermediates = intermediates,
	damageModList = damageModList,
	damageAgg = damageAgg,
	components = components,
	summedBase = summedBase,
	damageTypeBreakdown = damageTypeBreakdown,
	conversionTable = conversionTable,
	skillInfo = skillInfo,
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
