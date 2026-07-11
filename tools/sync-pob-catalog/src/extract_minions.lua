-- extract_minions.lua — `extract-lua --what minions|spectres` 的引导脚本
--
-- 在最小 stub 环境下加载 vendor/PathOfBuilding-PoE2 的 Data/Minions.lua 或
-- Data/Spectres.lua（接收 `local minions, mod, flag = ...` 注入，对应 vendor
-- Modules/Data.lua:987/989 `LoadModule("Data/Minions", data.minions,
-- makeSkillMod, makeFlagMod)`），把每个条目以 JSONL（每行一个 JSON 对象，
-- serde 形状 = pobr_data::catalog::actors::MinionEntryDef）写到 stdout。
--
-- R3 纪律（M5a 蓝图 §5）：mod(...)/flag(...) 构造的**全部入参**忠实序列化
-- （name/type/value/flags/keywordFlags/尾随 tag 表），任何不认识的形态立刻
-- 报错退出而非静默丢弃；vendor 注释行非数据，由 Lua 解释器自然忽略。
--
-- 确定性约定：本脚本只负责忠实抽取 + 合法 JSON；排序与数字最短表示由
-- Rust 侧统一完成。
--
-- 用法（由 Rust 侧经 stdin 注入执行）：
--   luajit - <vendor_src_dir> <Minions|Spectres>

local vendorSrc = arg and arg[1]
local fileListArg = arg and arg[2]
if not vendorSrc or not fileListArg then
	io.stderr:write("usage: luajit - <vendor_src_dir> <Minions|Spectres>\n")
	os.exit(2)
end

----------------------------------------------------------------------
-- stub：与 vendor Modules/Data.lua:56 makeSkillMod / :66 makeFlagMod 同形，
-- 把构造调用捕获为纯表（值的语义无关紧要，只透传）。
----------------------------------------------------------------------
local function makeSkillMod(modName, modType, modVal, flags, keywordFlags, ...)
	return {
		name = modName,
		type = modType,
		value = modVal,
		flags = flags,
		keywordFlags = keywordFlags,
		tags = { ... },
		__isModStub = true,
	}
end
local function makeFlagMod(modName, ...)
	return makeSkillMod(modName, "FLAG", true, 0, 0, ...)
end

----------------------------------------------------------------------
-- 加载目标数据文件（vendor 只读）
----------------------------------------------------------------------
local minions = {}
for fileName in string.gmatch(fileListArg, "[^,]+") do
	local path = vendorSrc .. "/Data/" .. fileName .. ".lua"
	local chunk, loadErr = loadfile(path)
	if not chunk then
		io.stderr:write("cannot load " .. path .. ": " .. tostring(loadErr) .. "\n")
		os.exit(3)
	end
	local ok, runErr = pcall(chunk, minions, makeSkillMod, makeFlagMod)
	if not ok then
		io.stderr:write("error executing " .. path .. ": " .. tostring(runErr) .. "\n")
		os.exit(3)
	end
end

----------------------------------------------------------------------
-- JSON 工具
----------------------------------------------------------------------
local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

-- %.17g 保证 f64 精度无损往返；Rust 侧 serde_json 重排为最短表示
local function jsonNum(v)
	if v ~= v or v == math.huge or v == -math.huge then
		error("non-finite number in minion data")
	end
	return string.format("%.17g", v)
end

-- 标量（boolean/number/string）→ JSON；其它类型 = 数据形态超出契约，报错。
local function jsonScalar(v, where)
	local t = type(v)
	if t == "boolean" then
		return v and "true" or "false"
	elseif t == "number" then
		return jsonNum(v)
	elseif t == "string" then
		return '"' .. jsonEscape(v) .. '"'
	end
	error("unsupported scalar type " .. t .. " at " .. where)
end

local function jsonStrArray(list)
	local parts = {}
	for i, v in ipairs(list) do
		parts[i] = '"' .. jsonEscape(v) .. '"'
	end
	return "[" .. table.concat(parts, ",") .. "]"
end

----------------------------------------------------------------------
-- mod 构造 → MinionModDef JSON（tag 表 = 扁平 键→标量 映射，键升序）
----------------------------------------------------------------------
local function encodeTag(tag, where)
	if type(tag) ~= "table" then
		error("mod tag is not a table at " .. where)
	end
	local keys = {}
	for k, v in pairs(tag) do
		if type(k) ~= "string" then
			error("non-string tag key at " .. where)
		end
		if type(v) == "table" then
			error("nested table tag value at " .. where .. " key " .. k .. " (扩 schema 后再抽)")
		end
		keys[#keys + 1] = k
	end
	table.sort(keys)
	local parts = {}
	for i, k in ipairs(keys) do
		parts[i] = '"' .. jsonEscape(k) .. '":' .. jsonScalar(tag[k], where .. "." .. k)
	end
	return "{" .. table.concat(parts, ",") .. "}"
end

local function encodeMod(m, where)
	if type(m) ~= "table" or not m.__isModStub then
		error("modList entry is not a mod()/flag() construction at " .. where)
	end
	local parts = {
		'"name":' .. jsonScalar(m.name, where .. ".name"),
		'"type":' .. jsonScalar(m.type, where .. ".type"),
		'"value":' .. jsonScalar(m.value, where .. ".value"),
	}
	if m.flags ~= nil then
		parts[#parts + 1] = '"flags":' .. jsonScalar(m.flags, where .. ".flags")
	end
	if m.keywordFlags ~= nil then
		parts[#parts + 1] = '"keyword_flags":' .. jsonScalar(m.keywordFlags, where .. ".keywordFlags")
	end
	if #m.tags > 0 then
		local tagParts = {}
		for i, tag in ipairs(m.tags) do
			tagParts[i] = encodeTag(tag, where .. ".tags[" .. i .. "]")
		end
		parts[#parts + 1] = '"tags":[' .. table.concat(tagParts, ",") .. "]"
	end
	return "{" .. table.concat(parts, ",") .. "}"
end

----------------------------------------------------------------------
-- 条目输出（字段名 = MinionEntryDef serde 名；缺失键不输出 = 忠实转录）
----------------------------------------------------------------------
local REQUIRED_NUM = {
	{ "life", "life" },
	{ "damage", "damage" },
	{ "damageSpread", "damage_spread" },
	{ "attackTime", "attack_time" },
	{ "attackRange", "attack_range" },
	{ "accuracy", "accuracy" },
	{ "critChance", "crit_chance" },
	{ "baseMovementSpeed", "base_movement_speed" },
	{ "fireResist", "fire_resist" },
	{ "coldResist", "cold_resist" },
	{ "lightningResist", "lightning_resist" },
	{ "chaosResist", "chaos_resist" },
	{ "spectreReservation", "spectre_reservation" },
	{ "companionReservation", "companion_reservation" },
}
local OPTIONAL_NUM = {
	{ "armour", "armour" },
	{ "evasion", "evasion" },
	{ "energyShield", "energy_shield" },
	-- PoB2 0.5.4 起：伴侣形态的分抗性覆盖（缺失 = 沿用本体抗性）。
	{ "companionFireResist", "companion_fire_resist" },
	{ "companionColdResist", "companion_cold_resist" },
	{ "companionLightningResist", "companion_lightning_resist" },
	{ "companionChaosResist", "companion_chaos_resist" },
}
local OPTIONAL_STR = {
	{ "limit", "limit" },
	{ "monsterCategory", "monster_category" },
	{ "weaponType1", "weapon_type1" },
	{ "weaponType2", "weapon_type2" },
}
-- 已知键全集（出现未知键即报错——vendor schema 漂移显式暴露，R3）
local KNOWN_KEYS = {
	name = true,
	monsterTags = true,
	baseDamageIgnoresAttackSpeed = true,
	spawnLocation = true,
	skillList = true,
	modList = true,
	extraFlags = true,
}
for _, spec in ipairs(REQUIRED_NUM) do
	KNOWN_KEYS[spec[1]] = true
end
for _, spec in ipairs(OPTIONAL_NUM) do
	KNOWN_KEYS[spec[1]] = true
end
for _, spec in ipairs(OPTIONAL_STR) do
	KNOWN_KEYS[spec[1]] = true
end

local function emitMinion(id, m)
	for k in pairs(m) do
		if not KNOWN_KEYS[k] then
			error("unknown minion key '" .. tostring(k) .. "' on " .. id .. " (schema 演化，先扩 MinionEntryDef)")
		end
	end
	local parts = {
		'"id":"' .. jsonEscape(id) .. '"',
		'"name":' .. jsonScalar(m.name, id .. ".name"),
	}
	if type(m.monsterTags) == "table" and #m.monsterTags > 0 then
		parts[#parts + 1] = '"monster_tags":' .. jsonStrArray(m.monsterTags)
	end
	for _, spec in ipairs(REQUIRED_NUM) do
		local v = m[spec[1]]
		if type(v) ~= "number" then
			error("missing/non-number field " .. spec[1] .. " on " .. id)
		end
		parts[#parts + 1] = '"' .. spec[2] .. '":' .. jsonNum(v)
	end
	if m.baseDamageIgnoresAttackSpeed then
		parts[#parts + 1] = '"base_damage_ignores_attack_speed":true'
	end
	for _, spec in ipairs(OPTIONAL_STR) do
		local v = m[spec[1]]
		if v ~= nil then
			parts[#parts + 1] = '"' .. spec[2] .. '":' .. jsonScalar(v, id .. "." .. spec[1])
		end
	end
	for _, spec in ipairs(OPTIONAL_NUM) do
		local v = m[spec[1]]
		if v ~= nil then
			parts[#parts + 1] = '"' .. spec[2] .. '":' .. jsonScalar(v, id .. "." .. spec[1])
		end
	end
	if type(m.spawnLocation) == "table" and #m.spawnLocation > 0 then
		parts[#parts + 1] = '"spawn_location":' .. jsonStrArray(m.spawnLocation)
	end
	if type(m.skillList) == "table" and #m.skillList > 0 then
		parts[#parts + 1] = '"skill_list":' .. jsonStrArray(m.skillList)
	end
	if type(m.extraFlags) == "table" then
		local flags = {}
		for k, v in pairs(m.extraFlags) do
			if v == true then
				flags[#flags + 1] = k
			elseif v ~= false and v ~= nil then
				error("non-boolean extraFlags value on " .. id .. " key " .. tostring(k))
			end
		end
		table.sort(flags)
		if #flags > 0 then
			parts[#parts + 1] = '"extra_flags":' .. jsonStrArray(flags)
		end
	end
	if type(m.modList) == "table" and #m.modList > 0 then
		local modParts = {}
		for i, mm in ipairs(m.modList) do
			modParts[i] = encodeMod(mm, id .. ".modList[" .. i .. "]")
		end
		parts[#parts + 1] = '"mod_list":[' .. table.concat(modParts, ",") .. "]"
	end
	print("{" .. table.concat(parts, ",") .. "}")
end

for id, m in pairs(minions) do
	if type(m) == "table" then
		emitMinion(id, m)
	end
end
