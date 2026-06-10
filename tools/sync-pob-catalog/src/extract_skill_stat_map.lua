-- extract_skill_stat_map.lua — sync-pob-catalog `extract-lua --what stat-map` 的引导脚本
--
-- 在**最小 stub 环境**下加载 vendor/PathOfBuilding-PoE2 的：
--   1. Data/SkillStatMap.lua —— 954 条全局 stat → modifier 构造器映射；
--   2. Data/Skills/<file>.lua —— 各 statSet 的 `statMap` 字段（per-set 覆盖）。
-- 把每条映射**原样纯表化**为 JSONL（每行一个 JSON 对象）写到 stdout：
--
--   {"scope":"global","stat":"<id>","entry":{...}}
--   {"scope":"set","effect":"<skillId>","stat_set":N,"stat":"<id>","entry":{...}}
--
-- 抽取保真原则（M1 蓝图 T2.1）：tags / 嵌套表忠实落 JSON，**不做任何语义
-- 筛选**——支持性判定是引擎（pobr-core stat_map_engine）的职责。遇 Lua 函数值
-- 等不可序列化字段时整条标 `"_unextractable":true` 并 stderr 告警，不静默丢弃。
--
-- 确定性约定：本脚本只负责"忠实抽取 + 合法 JSON"；最终排序（BTreeMap 字典序）
-- 与整体文档（_meta + global + per_stat_set）的 byte-stable 序列化由 Rust 侧完成。
--
-- 用法（由 Rust 侧经 stdin 注入执行）：
--   luajit - <vendor_src_dir> <逗号分隔的 Data/Skills 文件名（不含 .lua 后缀）>

local vendorSrc = arg and arg[1]
local fileListArg = arg and arg[2]
if not vendorSrc or not fileListArg then
	io.stderr:write("usage: luajit - <vendor_src_dir> <file1,file2,...>\n")
	os.exit(2)
end

----------------------------------------------------------------------
-- 最小 stub 环境（与 extract_skill_overrides.lua 同形）：SkillType/ModFlag/
-- KeywordFlag 用 __index 自映射成名字字符串；OR64 把多个 flag 名收进带
-- __flagOr 标记的列表（SkillStatMap.lua 有 `OR64(KeywordFlag.Hit, ...)` 用法）。
----------------------------------------------------------------------
SkillType = setmetatable({}, { __index = function(_, k) return k end })
ModFlag = setmetatable({}, { __index = function(_, k) return k end })
KeywordFlag = setmetatable({}, { __index = function(_, k) return k end })

-- SkillStatMap.lua 另有 `bit.bor(ModFlag.A, ModFlag.B)` 用法（tag 字段
-- `modFlags`）——同样收成名字列表（luajit 内建 bit 库被本 stub 遮蔽）。
local function flagOr(...)
	local out = { __flagOr = true }
	for _, v in ipairs({ ... }) do
		if type(v) == "table" and v.__flagOr then
			for _, inner in ipairs(v) do
				out[#out + 1] = inner
			end
		elseif type(v) == "string" then
			out[#out + 1] = v
		elseif v ~= 0 then
			error("flagOr: unsupported flag operand " .. tostring(v))
		end
	end
	return out
end
OR64 = flagOr
bit = { bor = flagOr }

-- 与 vendor Modules/Data.lua 的 makeSkillMod/makeFlagMod/makeSkillDataMod 同形：
-- 把构造器调用捕获为纯表，并记录调用的是哪个构造器（__kind）。
local function makeSkillMod(kind, modName, modType, modVal, flags, keywordFlags, ...)
	return {
		__kind = kind,
		name = modName,
		type = modType,
		value = modVal,
		flags = flags or 0,
		keywordFlags = keywordFlags or 0,
		...,
	}
end
local function modCtor(modName, modType, modVal, flags, keywordFlags, ...)
	return makeSkillMod("mod", modName, modType, modVal, flags, keywordFlags, ...)
end
local function flagCtor(modName, ...)
	return makeSkillMod("flag", modName, "FLAG", true, 0, 0, ...)
end
local function skillCtor(dataKey, dataValue, ...)
	return makeSkillMod("skill_data", "SkillData", "LIST", { key = dataKey, value = dataValue }, 0, 0, ...)
end

----------------------------------------------------------------------
-- 加载目标数据文件（vendor 只读，不做任何修改）
----------------------------------------------------------------------
local function loadVendorChunk(relPath)
	local path = vendorSrc .. "/" .. relPath
	local chunk, loadErr = loadfile(path)
	if not chunk then
		io.stderr:write("cannot load " .. path .. ": " .. tostring(loadErr) .. "\n")
		os.exit(3)
	end
	return chunk
end

-- 全局映射：SkillStatMap.lua 以 `local mod, flag, skill = ...` 接收注入并 return 表。
local okGlobal, globalMap = pcall(loadVendorChunk("Data/SkillStatMap.lua"), modCtor, flagCtor, skillCtor)
if not okGlobal then
	io.stderr:write("error executing Data/SkillStatMap.lua: " .. tostring(globalMap) .. "\n")
	os.exit(3)
end

-- per-set 覆盖：Data/Skills/<file>.lua 以 `local skills, mod, flag, skill = ...` 接收注入。
local skills = {}
for fileName in string.gmatch(fileListArg, "[^,]+") do
	local chunk = loadVendorChunk("Data/Skills/" .. fileName .. ".lua")
	local ok, runErr = pcall(chunk, skills, modCtor, flagCtor, skillCtor)
	if not ok then
		io.stderr:write("error executing Data/Skills/" .. fileName .. ".lua: " .. tostring(runErr) .. "\n")
		os.exit(3)
	end
end

----------------------------------------------------------------------
-- JSON 工具（通用纯值编码器：string/number/bool/数组表/对象表；函数值不可
-- 表示——编码失败由调用方整条标 _unextractable）。
----------------------------------------------------------------------
local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

-- %.17g 保证 f64 精度无损往返；Rust 侧 serde_json 会重排为最短表示
local function jsonNum(v)
	if v ~= v or v == math.huge or v == -math.huge then
		error("non-finite number in stat map data")
	end
	return string.format("%.17g", v)
end

-- 表是否为纯数组（仅连续整数键 1..n）
local function isArrayLike(t)
	local n = 0
	for k in pairs(t) do
		if type(k) ~= "number" or k % 1 ~= 0 or k < 1 then
			return false
		end
		n = n + 1
	end
	return n == #t
end

local function sortedStringKeys(t)
	local keys = {}
	for k in pairs(t) do
		if type(k) ~= "string" then
			error("non-string key in table value: " .. tostring(k))
		end
		keys[#keys + 1] = k
	end
	table.sort(keys)
	return keys
end

local encodeMod -- 前向声明（value 表内可嵌套 mod 构造器捕获，如 MinionModifier LIST）

-- 任意纯值 → JSON 文本；遇不可序列化类型（function/userdata/混合键表）抛错。
local function encodeValue(v)
	local tv = type(v)
	if tv == "string" then
		return '"' .. jsonEscape(v) .. '"'
	elseif tv == "number" then
		return jsonNum(v)
	elseif tv == "boolean" then
		return v and "true" or "false"
	elseif tv == "table" then
		-- 嵌套 mod 构造器捕获（如 `mod("MinionModifier","LIST",{ mod = mod(...) })`
		-- 的内层 mod）→ 按 mod 形状编码（含 kind/tags），保持嵌套语义可读。
		if v.__kind then
			return encodeMod(v)
		end
		-- flagOr 捕获（OR64 / bit.bor 的 flag 名集合）→ 名字数组
		if v.__flagOr then
			local parts = {}
			for i, name in ipairs(v) do
				parts[i] = '"' .. jsonEscape(name) .. '"'
			end
			return "[" .. table.concat(parts, ",") .. "]"
		end
		if isArrayLike(v) then
			local parts = {}
			for i, item in ipairs(v) do
				parts[i] = encodeValue(item)
			end
			return "[" .. table.concat(parts, ",") .. "]"
		end
		local parts = {}
		for _, k in ipairs(sortedStringKeys(v)) do
			parts[#parts + 1] = '"' .. jsonEscape(k) .. '":' .. encodeValue(v[k])
		end
		return "{" .. table.concat(parts, ",") .. "}"
	end
	error("unserializable value of type " .. tv)
end

----------------------------------------------------------------------
-- 条目序列化：vendor statMap 条目 → schema skill_stat_map/v1 的 entry JSON
----------------------------------------------------------------------
-- flags / keywordFlags 归一化：0 → 空列表；名字字符串 → 单元素；OR64 列表展开。
local function encodeFlagList(v)
	if v == 0 or v == nil then
		return "[]"
	elseif type(v) == "string" then
		return '["' .. jsonEscape(v) .. '"]'
	elseif type(v) == "table" and v.__flagOr then
		local parts = {}
		for i, name in ipairs(v) do
			parts[i] = '"' .. jsonEscape(name) .. '"'
		end
		return "[" .. table.concat(parts, ",") .. "]"
	end
	error("unsupported flags value " .. tostring(v))
end

local encodeElement -- 前向声明（group 递归）

-- 单个 mod 构造器捕获（带 __kind）→ JSON
encodeMod = function(m)
	local parts = { '"kind":"' .. m.__kind .. '"' }
	-- name / mod_type 偶有 vendor 笔误缺位（如 sup_str CorruptingCry 条目
	-- `mod("...", nil, 0, KeywordFlag.Warcry)` 漏 type）——忠实省略，由引擎归
	-- Unsupported，不在抽取期整条丢弃。
	if m.name ~= nil then
		parts[#parts + 1] = '"name":"' .. jsonEscape(m.name) .. '"'
	end
	if m.type ~= nil then
		parts[#parts + 1] = '"mod_type":"' .. jsonEscape(m.type) .. '"'
	end
	if m.value ~= nil then
		parts[#parts + 1] = '"value":' .. encodeValue(m.value)
	end
	local flags = encodeFlagList(m.flags)
	if flags ~= "[]" then
		parts[#parts + 1] = '"flags":' .. flags
	end
	local kwFlags = encodeFlagList(m.keywordFlags)
	if kwFlags ~= "[]" then
		parts[#parts + 1] = '"keyword_flags":' .. kwFlags
	end
	-- 数组部分 = 构造器变长参数捕获的 tag 表，原样纯表化
	if #m > 0 then
		local tagParts = {}
		for i, tag in ipairs(m) do
			if type(tag) ~= "table" then
				error("non-table tag on mod " .. tostring(m.name))
			end
			tagParts[i] = encodeValue(tag)
		end
		parts[#parts + 1] = '"tags":[' .. table.concat(tagParts, ",") .. "]"
	end
	return "{" .. table.concat(parts, ",") .. "}"
end

-- group（无 __kind 的嵌套表）：数组部分 = 嵌套 mod，命名键 = 组级 merge 参数
local function encodeGroup(g)
	local parts = { '"kind":"group"' }
	for _, key in ipairs({ "div", "mult", "base", "value" }) do
		if g[key] ~= nil then
			parts[#parts + 1] = '"' .. key .. '":' .. encodeValue(g[key])
		end
	end
	if g.scalar ~= nil then
		parts[#parts + 1] = '"scalar":' .. encodeValue(g.scalar)
	end
	-- 组内未知命名键 → 抛错（整条归 _unextractable，宁可上报不可静默）
	for k in pairs(g) do
		if
			type(k) == "string"
			and k ~= "div"
			and k ~= "mult"
			and k ~= "base"
			and k ~= "value"
			and k ~= "scalar"
		then
			error("unknown group key `" .. k .. "`")
		end
	end
	local modParts = {}
	for i, m in ipairs(g) do
		modParts[i] = encodeElement(m)
	end
	parts[#parts + 1] = '"mods":[' .. table.concat(modParts, ",") .. "]"
	return "{" .. table.concat(parts, ",") .. "}"
end

encodeElement = function(elem)
	if type(elem) ~= "table" then
		error("non-table statMap element")
	end
	if elem.__kind then
		return encodeMod(elem)
	end
	return encodeGroup(elem)
end

-- 一条 statMap 条目 → entry JSON。编码失败（函数值/未知形状）→ 返回
-- `{"_unextractable":true}` 并把原因写 stderr。
local function encodeEntry(where, stat, entry)
	local ok, json = pcall(function()
		local parts = {}
		for _, key in ipairs({ "div", "mult", "base", "value" }) do
			if entry[key] ~= nil then
				if type(entry[key]) ~= "number" then
					error("entry param `" .. key .. "` is not a number")
				end
				parts[#parts + 1] = '"' .. key .. '":' .. jsonNum(entry[key])
			end
		end
		-- skillFlag（如 `skill_can_fire_arrows` → `skillFlag = "arrow"`，由 PoB2
		-- statSet flags 路径消费而非 merge 公式）原样保留。
		if entry.skillFlag ~= nil then
			if type(entry.skillFlag) ~= "string" then
				error("entry param `skillFlag` is not a string")
			end
			parts[#parts + 1] = '"skill_flag":"' .. jsonEscape(entry.skillFlag) .. '"'
		end
		-- 条目级未知命名键 → 抛错（vendor merge 只读 div/mult/base/value）
		for k in pairs(entry) do
			if
				type(k) == "string"
				and k ~= "div"
				and k ~= "mult"
				and k ~= "base"
				and k ~= "value"
				and k ~= "skillFlag"
			then
				error("unknown entry key `" .. k .. "`")
			end
		end
		local modParts = {}
		for i, elem in ipairs(entry) do
			modParts[i] = encodeElement(elem)
		end
		if #modParts > 0 then
			parts[#parts + 1] = '"mods":[' .. table.concat(modParts, ",") .. "]"
		end
		return "{" .. table.concat(parts, ",") .. "}"
	end)
	if ok then
		return json
	end
	io.stderr:write("warning: unextractable statMap entry " .. where .. " `" .. stat .. "`: " .. tostring(json) .. "\n")
	return '{"_unextractable":true}'
end

----------------------------------------------------------------------
-- 输出 JSONL（顺序无关；Rust 侧 BTreeMap 统一排序）
----------------------------------------------------------------------
for stat, entry in pairs(globalMap) do
	print('{"scope":"global","stat":"' .. jsonEscape(stat) .. '","entry":' .. encodeEntry("global", stat, entry) .. "}")
end

for skillId, skill in pairs(skills) do
	if type(skill) == "table" and type(skill.statSets) == "table" then
		for setIndex, set in pairs(skill.statSets) do
			if type(set) == "table" and type(set.statMap) == "table" then
				for stat, entry in pairs(set.statMap) do
					print(
						'{"scope":"set","effect":"'
							.. jsonEscape(skillId)
							.. '","stat_set":'
							.. string.format("%d", setIndex)
							.. ',"stat":"'
							.. jsonEscape(stat)
							.. '","entry":'
							.. encodeEntry(skillId .. "#" .. tostring(setIndex), stat, entry)
							.. "}"
					)
				end
			end
		end
	end
end
