-- PoB2 headless oracle：SkillStatMap 注入后 modList 对拍（M1-T2b，蓝图 T2.3 oracle 抽样）。
--
-- 对给定 (effectId, stat, value) 探针，用 PoB2 真实框架函数
-- `calcs.mergeSkillInstanceMods`（Modules/CalcActiveSkill.lua:82）+ 真实 statMap
-- 数据（Data/SkillStatMap.lua 全局表 / Data/Skills/*.lua per-statSet 覆盖，经
-- metatable 链）跑一次受控合并，dump 注入后 modList（PoB2 原始名 / type / 合并值 /
-- flags / keywordFlags / tags）为 JSON。受控值经 `extraStats` 参数注入——合成
-- statSet 自身无 stat（buildSkillInstanceStats 返回空表），合并结果即该单条 stat
-- 的映射产物，可逐 stat 归因。
--
-- effectId 取 "GLOBAL" 时直接探全局表 data.skillStatMap；否则取
-- data.skills[effectId].statSets[1].statMap（默认 set，与 PoBR 引擎
-- set_key=None → "1" 的口径一致；vendor SkillsTab.lua:354 缺省 statSetIndex=1）。
--
-- 纯包装：不修改任何 vendor 源码。须从 vendor src/ 目录运行（同 oracle.lua）：
--   cd vendor/PathOfBuilding-PoE2/src
--   LUA_PATH="../runtime/lua/?.lua;../runtime/lua/?/init.lua;./?.lua;;" CI=true \
--     luajit ../../../tools/pob2-oracle/statmap_oracle.lua <effectId> <stat> <value> [...]
--
-- 参数为 (effectId, stat, value) 三元组的重复序列；JSON 数组输出到 stdout。

if #arg < 3 or #arg % 3 ~= 0 then
	io.stderr:write("usage: statmap_oracle.lua <effectId|GLOBAL> <stat> <value> [...]\n")
	os.exit(2)
end

-- HeadlessWrapper 启动过程可能改写全局 arg——先快照。
local queries = {}
for i = 1, #arg do
	queries[i] = arg[i]
end

-- 静音 headless 启动噪声（同 oracle.lua 做法）。
local realPrint = print
_G.print = function() end
dofile("HeadlessWrapper.lua")
_G.print = realPrint

-- `calcs` 是模块局部表（Modules/Calcs.lua:13-17 模式）——按同样方式把
-- CalcActiveSkill 装载进自己的表（LoadModule 幂等，纯函数注册，不动全局状态）。
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

-- 通用值序列化（number / string / boolean / 嵌套表；表按 string 键排序 + 数组部
-- 分保序，输出确定性）。
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
		-- 数组部分。
		local arr = {}
		for i, item in ipairs(v) do
			arr[#arr + 1] = encodeValue(item, depth + 1)
		end
		if #arr > 0 then
			parts[#parts + 1] = '"_list":[' .. table.concat(arr, ",") .. "]"
		end
		-- string 键部分（排序）。
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

-- 序列化一条注入后 mod：名字 / 聚合类型 / 合并值 / flags / keywordFlags / tags
-- （tag 为 mod 表的数组部分）。
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

-- 单条探针：合成 statSet（statMap 引用真实表，自身无 stat / baseMods），经
-- extraStats 注入受控 (stat, value)，跑真实 mergeSkillInstanceMods。
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
