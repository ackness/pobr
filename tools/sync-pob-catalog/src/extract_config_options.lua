-- extract_config_options.lua — ConfigOptions.lua 探针法抽取引导脚本（M3-T1 A2）
--
-- 在 PoB2 headless 环境（cwd = vendor src/，LUA_PATH 指向 runtime/lua）下
-- LoadModule("Modules/ConfigOptions")，对每个条目的 apply 闭包做
-- 「调用拦截（RecordingModList）+ 多探针拟合」：
--   * check 型：apply(true) 一次 → effects 全字面量；
--   * count/integer/float 型：多探针（1..250）→ 逐数值叶线性拟合
--     `v = a·val + b` → Input{mult,base}，饱和检测 → Clamp 包裹；
--     全探针复验不一致 → 降级 handler_id；
--   * list 型：逐选项各调一次 → option_effects（全选项一致时归并 effects）；
--   * text 型（customMods）：不探针，直接 handler_id（消费侧专用行通道）。
-- 逃逸检测：apply 读 build 参数 / 调用白名单外方法 / 运行报错 → handler_id。
--
-- 输出：stdout 每行一个条目 JSON（serde 形状 = pobr-data
-- catalog::config_def::ConfigOptionDef）；统计走 stderr。
-- Rust 侧（extract_config_options.rs）负责排序 + `_meta` + byte-stable 组装。

-- 静音 PoB 启动噪声（image/tree load 等全部走 print）。JSONL 用 io.write 直写。
_G.print = function() end
dofile("HeadlessWrapper.lua")

local dkjson = require("dkjson")

local varList = LoadModule("Modules/ConfigOptions")
assert(type(varList) == "table" and #varList > 0, "ConfigOptions 加载失败")

----------------------------------------------------------------------
-- 工具
----------------------------------------------------------------------

local band = bit.band
local bor = bit.bor

-- 数值规整：拟合系数去浮点尾巴（复验仍以容差判定，规整只为输出整洁）。
local function snap(x)
	local r = math.floor(x * 1e9 + 0.5) / 1e9
	return r
end

local function approxEq(a, b)
	return math.abs(a - b) <= 1e-6 * math.max(1, math.abs(a), math.abs(b))
end

-- 标记空 Lua 表按 JSON 数组编码（dkjson 缺省把空表编成 {}）。
local function markArray(t)
	return setmetatable(t, { __jsontype = "array" })
end

-- ModFlag/KeywordFlag 位 → 名称数组（贪心分解：优先位多的复合名）。
-- 分解失败（OR 不回原值）返回 nil。
local function decomposeFlags(flags, src)
	if flags == 0 then return {} end
	local candidates = {}
	for name, v in pairs(src) do
		if type(v) == "number" and v ~= 0 and band(flags, v) == v then
			candidates[#candidates + 1] = { name = name, bits = v }
		end
	end
	-- 位覆盖多者优先、同覆盖按名稳定排序
	table.sort(candidates, function(a, b)
		if a.bits ~= b.bits then return a.bits > b.bits end
		return a.name < b.name
	end)
	local acc, names = 0, {}
	for _, c in ipairs(candidates) do
		if band(acc, c.bits) ~= c.bits then
			acc = bor(acc, c.bits)
			names[#names + 1] = c.name
		end
		if acc == flags then break end
	end
	if acc ~= flags then return nil end
	table.sort(names)
	return names
end

-- tag 白名单序列化：Condition{var,neg} / Multiplier{var,div,limit,actor} /
-- ActorCondition{actor,var,neg}。白名单外形态返回 nil + 原因。
local function serializeTag(tag)
	local allowedKeys = {
		Condition = { type = true, var = true, neg = true },
		Multiplier = { type = true, var = true, div = true, limit = true, actor = true },
		ActorCondition = { type = true, actor = true, var = true, neg = true },
	}
	local allowed = allowedKeys[tag.type]
	if not allowed then
		return nil, "tag 类型 " .. tostring(tag.type) .. " 不在白名单"
	end
	for k in pairs(tag) do
		if not allowed[k] then
			return nil, "tag " .. tag.type .. " 携带白名单外字段 " .. tostring(k)
		end
	end
	if type(tag.var) ~= "string" then
		return nil, "tag " .. tag.type .. " 的 var 非字符串"
	end
	if tag.type == "Condition" then
		local out = { type = "condition", var = tag.var }
		if tag.neg then out.neg = true end
		return out
	elseif tag.type == "Multiplier" then
		local out = { type = "multiplier", var = tag.var }
		if tag.div and tag.div ~= 1 then out.div = tag.div end
		if tag.limit then out.limit = tag.limit end
		if tag.actor then out.actor = tostring(tag.actor) end
		return out
	else
		local out = { type = "actor_condition", actor = tostring(tag.actor), var = tag.var }
		if tag.neg then out.neg = true end
		return out
	end
end

----------------------------------------------------------------------
-- RecordingModList + 逃逸探针
----------------------------------------------------------------------

local function newRecorder(target, records)
	local rec = { __target = target, __records = records }
	function rec:NewMod(...)
		local mod = modLib.createMod(...)
		records[#records + 1] = { target = self.__target, mod = mod }
	end
	function rec:AddMod(mod)
		records[#records + 1] = { target = self.__target, mod = mod }
	end
	return rec
end

local function newEscapeProbe(flagbox)
	local probe = {}
	setmetatable(probe, {
		__index = function()
			flagbox.escaped = true
			return newEscapeProbe(flagbox)
		end,
		__call = function()
			flagbox.escaped = true
			return nil
		end,
	})
	return probe
end

-- 单次探针调用：返回 records 数组或 nil + 原因。
local function runProbe(entry, val)
	local records = {}
	local flagbox = { escaped = false }
	local ml = newRecorder("player", records)
	local eml = newRecorder("enemy", records)
	local ok, err = pcall(entry.apply, val, ml, eml, newEscapeProbe(flagbox))
	if not ok then
		return nil, "apply 运行报错: " .. tostring(err)
	end
	if flagbox.escaped then
		return nil, "apply 读取 build/env 状态（真逻辑）"
	end
	return records
end

----------------------------------------------------------------------
-- 记录骨架（identity）与数值叶提取
----------------------------------------------------------------------

-- 把一条 record 拆成 skeleton（数值叶以 "#" 占位的确定性字符串）+ leaves
-- （路径有序的数值叶数组）。白名单外形态返回 nil + 原因。
local function analyzeRecord(record)
	local mod = record.mod
	if type(mod) ~= "table" or type(mod.name) ~= "string" or type(mod.type) ~= "string" then
		return nil, "mod 形状非法"
	end
	local leaves = {}
	local skel = { target = record.target, name = mod.name, type = mod.type }

	-- flags / keywordFlags
	local flagNames = decomposeFlags(mod.flags or 0, ModFlag)
	if not flagNames then return nil, "ModFlag 位无法分解" end
	local kfNames = decomposeFlags(mod.keywordFlags or 0, KeywordFlag)
	if not kfNames then return nil, "KeywordFlag 位无法分解" end
	skel.flags = flagNames
	skel.keyword_flags = kfNames
	skel.source = mod.source and tostring(mod.source) or nil

	-- tags（整体进 identity；数值字段随探针变化时 identity 不同 → 上层降级）
	local tags = {}
	for _, tag in ipairs(mod) do
		if type(tag) == "table" then
			local s, reason = serializeTag(tag)
			if not s then return nil, reason end
			tags[#tags + 1] = s
		end
	end
	skel.tags = tags

	-- value
	local v = mod.value
	local tv = type(v)
	if tv == "number" then
		skel.value_shape = "number"
		leaves[#leaves + 1] = { path = "value", value = v }
	elseif tv == "boolean" then
		skel.value_shape = "bool:" .. tostring(v)
	elseif tv == "string" then
		skel.value_shape = "text:" .. v
	elseif tv == "table" then
		if type(v.key) == "string" and v.value ~= nil and v.mod == nil then
			-- SkillData 键值载荷
			local lv = v.value
			local lt = type(lv)
			skel.value_shape = "kv:" .. v.key .. ":" .. lt
			if lt == "number" then
				leaves[#leaves + 1] = { path = "kv.value", value = lv }
			elseif lt == "boolean" or lt == "string" then
				skel.value_shape = skel.value_shape .. ":" .. tostring(lv)
			else
				return nil, "键值载荷 value 类型 " .. lt .. " 不支持"
			end
			skel.kv_key = v.key
		elseif type(v.mod) == "table" and v.key == nil then
			-- 嵌套 mod（MinionModifier 等）；二层嵌套不在白名单
			local inner = v.mod
			if type(inner.value) == "table" then
				return nil, "嵌套 mod 的 value 仍为 table（二层嵌套不支持）"
			end
			local innerRec, reason = analyzeRecord({ target = record.target, mod = inner })
			if not innerRec then return nil, "嵌套 mod: " .. tostring(reason) end
			skel.value_shape = "nested"
			skel.nested = innerRec.skel
			for _, leaf in ipairs(innerRec.leaves) do
				leaves[#leaves + 1] = { path = "nested." .. leaf.path, value = leaf.value }
			end
		else
			return nil, "LIST 载荷形态不在白名单（非 key/value、非 mod 嵌套）"
		end
	else
		return nil, "mod value 类型 " .. tv .. " 不支持"
	end

	return { skel = skel, leaves = leaves, identity = dkjson.encode(skel) }
end

----------------------------------------------------------------------
-- 数值序列拟合：literal / input(mult,base) / clamp 包裹
----------------------------------------------------------------------

-- probes / values 等长数组（probes 为数值探针）。返回 ValueExpr 表或 nil。
local function fitSeries(probes, values)
	-- 全等 → literal
	local allEqual = true
	for i = 2, #values do
		if not approxEq(values[i], values[1]) then allEqual = false break end
	end
	if allEqual then
		return { kind = "literal", value = values[1] }
	end
	if #probes < 2 then return nil end

	local function tryLinear(i, j)
		if probes[i] == probes[j] then return nil end
		local a = (values[j] - values[i]) / (probes[j] - probes[i])
		local b = values[i] - a * probes[i]
		return snap(a), snap(b)
	end

	local function inputExpr(a, b)
		local expr = { kind = "input" }
		if a ~= 1 then expr.mult = a end
		if b ~= 0 then expr.base = b end
		return expr
	end

	-- 纯线性
	for i = 1, #probes - 1 do
		local a, b = tryLinear(i, i + 1)
		if a then
			local ok = true
			for k = 1, #probes do
				if not approxEq(a * probes[k] + b, values[k]) then ok = false break end
			end
			if ok then
				return inputExpr(a, b)
			end
		end
	end

	-- clamp 包线性：高端/低端饱和为常数，中段线性
	local vmax, vmin = values[1], values[1]
	for _, v in ipairs(values) do
		vmax = math.max(vmax, v)
		vmin = math.min(vmin, v)
	end
	for i = 1, #probes - 1 do
		local a, b = tryLinear(i, i + 1)
		if a and a ~= 0 then
			for _, bounds in ipairs({ { max = vmax }, { min = vmin }, { min = vmin, max = vmax } }) do
				local ok = true
				for k = 1, #probes do
					local pred = a * probes[k] + b
					if bounds.max and pred > bounds.max then pred = bounds.max end
					if bounds.min and pred < bounds.min then pred = bounds.min end
					if not approxEq(pred, values[k]) then ok = false break end
				end
				-- 至少有一个探针真的被截断，否则纯线性早已命中
				if ok then
					return {
						kind = "clamp",
						min = bounds.min,
						max = bounds.max,
						inner = inputExpr(a, b),
					}
				end
			end
		end
	end
	return nil
end

----------------------------------------------------------------------
-- skeleton + 拟合结果 → ConfigEffect JSON 表
----------------------------------------------------------------------

-- fits: path → ValueExpr 表
local function skelToValueFields(skel, fits, prefix, out)
	local shape = skel.value_shape
	if shape == "number" then
		out.value = fits[prefix .. "value"]
		if not out.value then return nil, "数值叶拟合失败" end
	elseif shape:sub(1, 5) == "bool:" then
		out.value_bool = (shape == "bool:true")
	elseif shape:sub(1, 5) == "text:" then
		out.value_text = shape:sub(6)
	elseif shape:sub(1, 3) == "kv:" then
		local scalar
		local lt = shape:match("^kv:[^:]*:(%a+)")
		if lt == "number" then
			local expr = fits[prefix .. "kv.value"]
			if not expr then return nil, "键值数值叶拟合失败" end
			scalar = { kind = "expr", expr = expr }
		elseif lt == "boolean" then
			scalar = { kind = "bool", value = shape:match(":(%a+)$") == "true" }
		else
			scalar = { kind = "text", value = shape:match("^kv:[^:]*:string:(.*)$") or "" }
		end
		out.list_value = { kind = "key_value", key = skel.kv_key, value = scalar }
	elseif shape == "nested" then
		local nested = {
			name = skel.nested.name,
			mod_type = skel.nested.type,
		}
		if #skel.nested.flags > 0 then nested.flags = skel.nested.flags end
		if #skel.nested.keyword_flags > 0 then nested.keyword_flags = skel.nested.keyword_flags end
		if #skel.nested.tags > 0 then nested.tags = skel.nested.tags end
		if skel.nested.source and skel.nested.source ~= "Config" then
			nested.source = skel.nested.source
		end
		local ok, reason = skelToValueFields(skel.nested, fits, prefix .. "nested.", nested)
		if not ok then return nil, reason end
		out.list_value = { kind = "nested_mod", mod = nested }
	else
		return nil, "未知 value_shape " .. tostring(shape)
	end
	return true
end

-- 把一个 identity 位置（skeleton + 各探针 leaves）变成 effect 表。
local function buildEffect(skel, fits, emitIf)
	local effect = {
		target = skel.target,
		name = skel.name,
		mod_type = skel.type,
	}
	if #skel.flags > 0 then effect.flags = skel.flags end
	if #skel.keyword_flags > 0 then effect.keyword_flags = skel.keyword_flags end
	if #skel.tags > 0 then effect.tags = skel.tags end
	if skel.source and skel.source ~= "Config" then effect.source = skel.source end
	if emitIf then effect.emit_if = emitIf end
	local ok, reason = skelToValueFields(skel, fits, "", effect)
	if not ok then return nil, reason end
	return effect
end

----------------------------------------------------------------------
-- 多探针归纳主流程
----------------------------------------------------------------------

-- 探针集合。小值段（1..5）必须加密：诸如 `m_min(val - 1, 3)` 的小膝点
-- clamp，若线性段只落 1 个探针则斜率欠定（拟合可通过全部探针但在
-- val=2..4 区域错误）；加密后任何候选拟合仍须通过全部探针，只会更严格。
-- 17/37 为互质参考点，100/250 探饱和段。
local PROBES = {
	count = { 1, 2, 3, 4, 5, 7, 17, 37, 100, 250 },
	countAllowZero = { 0, 1, 2, 3, 4, 5, 7, 17, 37, 100, 250 },
	integer = { 0, 1, 2, 3, 4, 5, 7, 17, 37, 100, 250 },
	float = { 0.25, 0.5, 1, 2, 3, 4, 5, 7, 17, 37, 100, 250 },
}

-- 数值型条目归纳。返回 effects 数组或 nil + 原因。
local function induceNumeric(entry, probes)
	local analyzed = {} -- probe 序 → { rec 分析数组 }
	for pi, val in ipairs(probes) do
		local records, err = runProbe(entry, val)
		if not records then return nil, err end
		local recs = {}
		for _, record in ipairs(records) do
			local a, reason = analyzeRecord(record)
			if not a then return nil, reason end
			recs[#recs + 1] = a
		end
		analyzed[pi] = recs
	end

	-- identity 序列
	local function identitySeq(recs)
		local parts = {}
		for i, a in ipairs(recs) do parts[i] = a.identity end
		return table.concat(parts, "\n")
	end
	local baseIdx = 1
	for pi = 2, #analyzed do
		if #analyzed[pi] > #analyzed[baseIdx] then baseIdx = pi end
	end
	local base = analyzed[baseIdx]
	local baseSeq = identitySeq(base)

	-- 每个探针：与 base 的位置配对（相同序列 → 1:1；子序列 → 贪心两指针）。
	-- pairIdx[pi][basePos] = 该探针中的位置（nil = 缺席）
	local pairIdx = {}
	for pi, recs in ipairs(analyzed) do
		if identitySeq(recs) == baseSeq then
			local m = {}
			for i = 1, #recs do m[i] = i end
			pairIdx[pi] = m
		else
			-- 子序列贪心匹配
			local m = {}
			local j = 1
			for i = 1, #base do
				if j <= #recs and recs[j].identity == base[i].identity then
					m[i] = j
					j = j + 1
				end
			end
			if j ~= #recs + 1 then
				return nil, "探针 effects 结构不可配对（识别为真逻辑）"
			end
			pairIdx[pi] = m
		end
	end

	-- 逐 base 位置：收集出现探针集合 + 各叶数值序列
	local effects = {}
	for i = 1, #base do
		local presentProbes, presentValuesByPath = {}, {}
		for pi, recs in ipairs(analyzed) do
			local at = pairIdx[pi][i]
			if at then
				presentProbes[#presentProbes + 1] = probes[pi]
				for _, leaf in ipairs(recs[at].leaves) do
					presentValuesByPath[leaf.path] = presentValuesByPath[leaf.path] or {}
					table.insert(presentValuesByPath[leaf.path], leaf.value)
				end
			end
		end

		-- emit_if：仅在 val > 0 探针出现 → input > 0；其它缺席模式 → 降级
		local emitIf = nil
		if #presentProbes ~= #probes then
			local matchesPositive = true
			for pi, recs in ipairs(analyzed) do
				local present = pairIdx[pi][i] ~= nil
				if (probes[pi] > 0) ~= present then matchesPositive = false break end
			end
			if not matchesPositive then
				return nil, "效果缺席模式无法归纳为 input>0 谓词"
			end
			emitIf = { kind = "cmp", on = "input", op = "gt", rhs = 0 }
		end

		local fits = {}
		for path, values in pairs(presentValuesByPath) do
			local expr = fitSeries(presentProbes, values)
			if not expr then
				return nil, "数值叶 " .. path .. " 拟合失败（非线性/非 clamp）"
			end
			fits[path] = expr
		end

		local effect, reason = buildEffect(base[i].skel, fits, emitIf)
		if not effect then return nil, reason end
		effects[#effects + 1] = effect
	end
	return effects
end

-- check 型：单探针、全字面量。
local function induceCheck(entry)
	local records, err = runProbe(entry, true)
	if not records then return nil, err end
	local effects = {}
	for _, record in ipairs(records) do
		local a, reason = analyzeRecord(record)
		if not a then return nil, reason end
		local fits = {}
		for _, leaf in ipairs(a.leaves) do
			fits[leaf.path] = { kind = "literal", value = leaf.value }
		end
		local effect, r2 = buildEffect(a.skel, fits, nil)
		if not effect then return nil, r2 end
		effects[#effects + 1] = effect
	end
	return effects
end

-- list 型：逐选项一次，effects 全字面量。返回 (sharedEffects, optionEffects)。
local function induceList(entry)
	local optionEffects = {}
	local firstJson, allEqual = nil, true
	for _, option in ipairs(entry.list or {}) do
		local records, err = runProbe(entry, option.val)
		if not records then return nil, nil, err end
		local effects = {}
		for _, record in ipairs(records) do
			local a, reason = analyzeRecord(record)
			if not a then return nil, nil, reason end
			local fits = {}
			for _, leaf in ipairs(a.leaves) do
				fits[leaf.path] = { kind = "literal", value = leaf.value }
			end
			local effect, r2 = buildEffect(a.skel, fits, nil)
			if not effect then return nil, nil, r2 end
			effects[#effects + 1] = effect
		end
		optionEffects[tostring(option.val)] = markArray(effects)
		local j = dkjson.encode(effects)
		if firstJson == nil then
			firstJson = j
		elseif j ~= firstJson then
			allEqual = false
		end
	end
	if allEqual and firstJson then
		-- 全选项一致 → 归并为共享 effects
		local anyKey = next(optionEffects)
		return optionEffects[anyKey], nil
	end
	return nil, optionEffects
end

----------------------------------------------------------------------
-- schema 字段序列化
----------------------------------------------------------------------

local function toStringArray(v)
	if v == nil then return nil end
	if type(v) == "string" then return { v } end
	if type(v) == "table" then
		local out = {}
		for _, item in ipairs(v) do out[#out + 1] = tostring(item) end
		if #out > 0 then return out end
	end
	return nil
end

local INPUT_TYPE_MAP = {
	check = "check",
	count = "count",
	countAllowZero = "count_allow_zero",
	integer = "integer",
	float = "float",
	list = "list",
	text = "text",
}

local function buildSchemaFields(entry, section)
	local def = {
		var = entry.var,
		input_type = INPUT_TYPE_MAP[entry.type],
		section = section,
	}
	if type(entry.label) == "string" then def.label = entry.label end

	local default = {}
	if type(entry.defaultState) == "boolean" then default.state_bool = entry.defaultState end
	if type(entry.defaultState) == "number" then default.state_number = entry.defaultState end
	if type(entry.defaultIndex) == "number" then default.index = entry.defaultIndex end
	if type(entry.defaultPlaceholderState) == "number" then
		default.placeholder_number = entry.defaultPlaceholderState
	end
	if next(default) then def.default = default end

	if type(entry.list) == "table" then
		local options = {}
		for _, option in ipairs(entry.list) do
			local item = { value = tostring(option.val), label = tostring(option.label) }
			if type(option.val) == "number" then item.number = option.val end
			options[#options + 1] = item
		end
		if #options > 0 then def.list_options = options end
	end

	local visibility = {}
	local visMap = {
		ifCond = "if_cond",
		ifMinionCond = "if_minion_cond",
		ifEnemyCond = "if_enemy_cond",
		ifFlag = "if_flag",
		ifMult = "if_mult",
		ifEnemyMult = "if_enemy_mult",
		ifSkill = "if_skill",
		ifSkillList = "if_skill",
		ifSkillData = "if_skill_data",
		ifMod = "if_mod",
		ifTagType = "if_tag_type",
	}
	for luaKey, jsonKey in pairs(visMap) do
		local arr = toStringArray(entry[luaKey])
		if arr then
			visibility[jsonKey] = visibility[jsonKey] or {}
			for _, item in ipairs(arr) do
				table.insert(visibility[jsonKey], item)
			end
		end
	end
	if next(visibility) then def.visibility = visibility end

	local imply = {}
	for _, c in ipairs(toStringArray(entry.implyCond) or {}) do imply[#imply + 1] = c end
	for _, c in ipairs(toStringArray(entry.implyCondList) or {}) do imply[#imply + 1] = c end
	if #imply > 0 then def.imply_conditions = imply end

	return def
end

----------------------------------------------------------------------
-- 主循环
----------------------------------------------------------------------

local stats = { total = 0, template = 0, handler = 0, no_apply = 0 }
local handlerReasons = {}
local section = ""

for _, entry in ipairs(varList) do
	if entry.section then
		section = entry.section
	end
	if entry.var and entry.type and INPUT_TYPE_MAP[entry.type] then
		stats.total = stats.total + 1
		local def = buildSchemaFields(entry, section)

		if not entry.apply then
			-- 纯 schema 条目（标量消费 / UI-only）：零 effects，无需验证。
			def.verified = true
			stats.no_apply = stats.no_apply + 1
		elseif entry.type == "text" then
			def.handler_id = "config:custom_mods"
			def.handler_reason = "text 型走消费侧专用行通道（StripEscapes + mod_parser）"
			def.verified = true
			stats.handler = stats.handler + 1
		elseif entry.type == "check" then
			local effects, reason = induceCheck(entry)
			if effects then
				if #effects > 0 then def.effects = effects end
				def.verified = true
				stats.template = stats.template + 1
			else
				def.handler_id = "config:" .. entry.var
				def.handler_reason = reason
				def.verified = false
				stats.handler = stats.handler + 1
				handlerReasons[#handlerReasons + 1] = entry.var .. ": " .. tostring(reason)
			end
		elseif entry.type == "list" then
			local shared, perOption, reason = induceList(entry)
			if shared or perOption then
				if shared and #shared > 0 then def.effects = shared end
				if perOption then def.option_effects = perOption end
				def.verified = true
				stats.template = stats.template + 1
			else
				def.handler_id = "config:" .. entry.var
				def.handler_reason = reason
				def.verified = false
				stats.handler = stats.handler + 1
				handlerReasons[#handlerReasons + 1] = entry.var .. ": " .. tostring(reason)
			end
		else
			local probes = PROBES[entry.type]
			local effects, reason
			if probes then
				effects, reason = induceNumeric(entry, probes)
			else
				reason = "未知输入类型 " .. tostring(entry.type)
			end
			if effects then
				if #effects > 0 then def.effects = effects end
				def.verified = true
				stats.template = stats.template + 1
			else
				def.handler_id = "config:" .. entry.var
				def.handler_reason = reason
				def.verified = false
				stats.handler = stats.handler + 1
				handlerReasons[#handlerReasons + 1] = entry.var .. ": " .. tostring(reason)
			end
		end

		io.write(dkjson.encode(def), "\n")
	end
end

io.stderr:write(string.format(
	"extract-config-options: 条目 %d（模板 %d / handler %d / 无 apply %d）\n",
	stats.total, stats.template, stats.handler, stats.no_apply
))
for _, line in ipairs(handlerReasons) do
	io.stderr:write("  handler: " .. line .. "\n")
end
