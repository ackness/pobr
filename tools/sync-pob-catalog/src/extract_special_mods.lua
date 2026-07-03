-- extract_special_mods.lua — sync-pob-catalog `extract-lua --what special-mods` 引导脚本
--
-- 与 extract_parser_rules.lua 同款 headless 引导（dofile HeadlessWrapper.lua，
-- cwd = vendor src/、LUA_PATH 由 Rust 侧设置），经 `debug.getupvalue` 拿到
-- parseMod 的 `specialModList` upvalue（加载后最终表，含 keystone / per-skill
-- 自动填充条目），逐 key 导出：
--
--   * 静态表条目 → 原样 dump mod 列表（flags/keywordFlags 掩码 → bit 名数组、
--     tag 内 skillType 数值 → SkillType 名，其余字段逐字转录）；
--   * 闭包条目 → 双哨兵探针（vendor 调用约定 :6518
--     `fn(tonumber(cap[1]), unpack(cap))`）推断 `$n` 占位模板；
--     **仅探纯数字捕获**（词类捕获需 enums 闭集，V0 批次不做）；
--   * 探不动 / 编码不了 → `kind:"failed"` + 原因（Rust 侧计数报表）。
--
-- 输出 JSONL 到 stdout（每行 `{"pattern":<vendor key>,"kind":...,...}`）；
-- pattern 转 regex、模板/白名单校验、去重、排序、_meta 由 Rust 侧完成。
--
-- 用法（由 Rust 侧经 stdin 注入执行；arg[1] 仅作 sanity check）：
--   cd <vendor_src>; LUA_PATH=... CI=true luajit - <vendor_src_dir>

local vendorSrc = arg and arg[1]
if not vendorSrc then
	io.stderr:write("usage: (cd <vendor_src>) luajit - <vendor_src_dir>\n")
	os.exit(2)
end

----------------------------------------------------------------------
-- headless 引导（同 extract_parser_rules.lua）
----------------------------------------------------------------------
local realPrint = print
_G.print = function() end
local okBoot, bootErr = pcall(dofile, "HeadlessWrapper.lua")
_G.print = realPrint
if not okBoot then
	io.stderr:write("headless bootstrap failed: " .. tostring(bootErr) .. "\n")
	os.exit(3)
end
assert(type(modLib) == "table" and type(modLib.parseMod) == "function", "modLib.parseMod missing after bootstrap")

local function upvalues(fn)
	local out = {}
	local i = 1
	while true do
		local name, value = debug.getupvalue(fn, i)
		if not name then
			break
		end
		out[name] = value
		i = i + 1
	end
	return out
end

local parseMod = upvalues(modLib.parseMod).parseMod
assert(type(parseMod) == "function", "inner parseMod upvalue missing")
local U = upvalues(parseMod)
assert(type(U.specialModList) == "table", "parseMod upvalue table missing: specialModList")

local function warn(msg)
	io.stderr:write("extract-special-mods: " .. msg .. "\n")
end

----------------------------------------------------------------------
-- 枚举反查（同 extract_parser_rules.lua：掩码 → bit 名、SkillType 数值 → 名）
----------------------------------------------------------------------
local function isPowerOfTwo(v)
	while v > 1 do
		if v % 2 ~= 0 then
			return false
		end
		v = v / 2
	end
	return v == 1
end

local function buildBitNames(enumTable, what)
	local bitNames = {}
	for name, value in pairs(enumTable) do
		if type(value) == "number" and value > 0 and isPowerOfTwo(value) then
			if bitNames[value] then
				if name < bitNames[value] then
					bitNames[value] = name
				end
			else
				bitNames[value] = name
			end
		end
	end
	return bitNames
end

assert(type(ModFlag) == "table" and type(KeywordFlag) == "table" and type(SkillType) == "table", "global enum tables missing")
local modFlagBits = buildBitNames(ModFlag, "ModFlag")
local keywordFlagBits = buildBitNames(KeywordFlag, "KeywordFlag")

local skillTypeName = {}
for name, value in pairs(SkillType) do
	if type(value) == "number" then
		if not skillTypeName[value] or name < skillTypeName[value] then
			skillTypeName[value] = name
		end
	end
end

local function decomposeMask(mask, bitNames, what)
	local names = {}
	local bitVal = 1
	while mask > 0 do
		if mask % 2 == 1 then
			local name = bitNames[bitVal]
			if not name then
				error("unmapped " .. what .. " bit " .. string.format("%.0f", bitVal))
			end
			names[#names + 1] = name
			mask = mask - 1
		end
		mask = mask / 2
		bitVal = bitVal * 2
	end
	return names
end

----------------------------------------------------------------------
-- JSON 编码（同 extract_parser_rules.lua：键字典序；整数 %d、其余 %.17g）
----------------------------------------------------------------------
local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

local function jsonStr(s)
	return '"' .. jsonEscape(s) .. '"'
end

local function jsonNum(v)
	if v ~= v or v == math.huge or v == -math.huge then
		error("non-finite number in special mods")
	end
	if v % 1 == 0 and math.abs(v) < 2 ^ 53 then
		return string.format("%d", v)
	end
	return string.format("%.17g", v)
end

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

-- 通用 raw 编码：mod / tag / LIST 值统一走这里。特判三个键：
--   skillType 数值 → SkillType 名；flags/keywordFlags 掩码 → bit 名数组；
--   source（vendor 源标注）→ 丢弃。其余键名与值逐字转录，Rust 侧做白名单校验。
local encodeRaw
encodeRaw = function(v)
	local tv = type(v)
	if tv == "string" then
		return jsonStr(v)
	elseif tv == "number" then
		return jsonNum(v)
	elseif tv == "boolean" then
		return v and "true" or "false"
	elseif tv == "table" then
		if isArrayLike(v) then
			local parts = {}
			for i, item in ipairs(v) do
				parts[i] = encodeRaw(item)
			end
			return "[" .. table.concat(parts, ",") .. "]"
		end
		local keys = {}
		for k in pairs(v) do
			if type(k) == "string" then
				keys[#keys + 1] = k
			elseif type(k) ~= "number" then
				error("non-string/number key in table: " .. tostring(k))
			end
		end
		table.sort(keys)
		local parts = {}
		for _, k in ipairs(keys) do
			local val = v[k]
			if k == "source" then
				-- vendor 源标注，不进 schema
			elseif k == "skillType" and type(val) == "number" then
				local name = skillTypeName[val] or error("unknown SkillType value " .. val)
				parts[#parts + 1] = '"skillType":' .. jsonStr(name)
			elseif (k == "flags" or k == "keywordFlags") and type(val) == "number" then
				local bits = k == "flags" and modFlagBits or keywordFlagBits
				local names = decomposeMask(val, bits, k)
				local items = {}
				for i, n in ipairs(names) do
					items[i] = jsonStr(n)
				end
				parts[#parts + 1] = jsonStr(k) .. ":[" .. table.concat(items, ",") .. "]"
			else
				parts[#parts + 1] = jsonStr(k) .. ":" .. encodeRaw(val)
			end
		end
		-- 数组部分（mod 表的 tag 列表）→ "tags" 键
		if #v > 0 then
			local items = {}
			for i, item in ipairs(v) do
				items[i] = encodeRaw(item)
			end
			parts[#parts + 1] = '"tags":[' .. table.concat(items, ",") .. "]"
		end
		return "{" .. table.concat(parts, ",") .. "}"
	end
	error("unserializable value of type " .. tv)
end

----------------------------------------------------------------------
-- 捕获槽分类 + 双哨兵探针（同 extract_parser_rules.lua，闭包推断）
----------------------------------------------------------------------
local NUM_A = { "73", "79", "83", "89", "101" }
local NUM_B = { "97", "103", "107", "109", "113" }

local function captureSlots(pattern)
	local slots = {}
	local i = 1
	local n = #pattern
	while i <= n do
		local c = pattern:sub(i, i)
		if c == "%" then
			i = i + 2
		elseif c == "(" then
			local j = pattern:find(")", i + 1, true) or error("unbalanced capture in " .. pattern)
			local content = pattern:sub(i + 1, j - 1)
			if content:find("%d", 1, true) then
				slots[#slots + 1] = "num"
			elseif content:find("%a", 1, true) or content:find("%D", 1, true) or content:find("%l", 1, true) then
				slots[#slots + 1] = "str"
			else
				slots[#slots + 1] = "any"
			end
			i = j + 1
		else
			i = i + 1
		end
	end
	return slots
end

local function firstToUpper(str)
	return (str:gsub("^%l", string.upper))
end

-- 联合遍历两次探针输出推断模板值（同 extract_parser_rules.lua inferValue，
-- 含 $n / $n:negate / $n:base(c) / $n:mult(k) / $n:div(k) 与字符串模板段）。
local inferValue
inferValue = function(a, b, capsA, capsB)
	local ta, tb = type(a), type(b)
	if ta ~= tb then
		return nil, "type mismatch (" .. ta .. " vs " .. tb .. ")"
	end
	if ta == "boolean" then
		if a == b then
			return a
		end
		return nil, "boolean differs between probes"
	end
	if ta == "number" then
		if a == b then
			return a
		end
		for i = 1, #capsA do
			local na, nb = tonumber(capsA[i]), tonumber(capsB[i])
			if na and nb then
				if a == na and b == nb then
					return "$" .. i
				end
				if a == -na and b == -nb then
					return "$" .. i .. ":negate"
				end
				local c = a - na
				if c ~= 0 and c % 1 == 0 and b - nb == c then
					return "$" .. i .. ":base(" .. string.format("%d", c) .. ")"
				end
				if na ~= 0 and a ~= 0 then
					local k = a / na
					if k % 1 == 0 and b == k * nb then
						return "$" .. i .. ":mult(" .. string.format("%d", k) .. ")"
					end
					local d = na / a
					if d % 1 == 0 and d ~= 0 and b * d == nb then
						return "$" .. i .. ":div(" .. string.format("%d", d) .. ")"
					end
				end
			end
		end
		return nil, "number not derivable from captures (" .. tostring(a) .. " / " .. tostring(b) .. ")"
	end
	if ta == "string" then
		if a == b then
			return a
		end
		local segs = {}
		local lit = ""
		local pos = 1
		while pos <= #a do
			local matched = false
			for i = 1, #capsA do
				local raw = capsA[i]
				local forms = { { firstToUpper(raw), "$" .. i .. ":cap" }, { raw, "$" .. i } }
				for _, form in ipairs(forms) do
					local s, ref = form[1], form[2]
					if #s > 0 and a:sub(pos, pos + #s - 1) == s then
						if #lit > 0 then
							segs[#segs + 1] = lit
							lit = ""
						end
						segs[#segs + 1] = ref
						pos = pos + #s
						matched = true
						break
					end
				end
				if matched then
					break
				end
			end
			if not matched then
				lit = lit .. a:sub(pos, pos)
				pos = pos + 1
			end
		end
		if #lit > 0 then
			segs[#segs + 1] = lit
		end
		local outB = ""
		local hasRef = false
		for _, seg in ipairs(segs) do
			local iRaw = seg:match("^%$(%d)$")
			local iCap = seg:match("^%$(%d):cap$")
			if iRaw then
				outB = outB .. capsB[tonumber(iRaw)]
				hasRef = true
			elseif iCap then
				outB = outB .. firstToUpper(capsB[tonumber(iCap)])
				hasRef = true
			else
				if seg:find("+", 1, true) or seg:find("$", 1, true) then
					return nil, "literal segment contains reserved char: " .. seg
				end
				outB = outB .. seg
			end
		end
		if not hasRef then
			return nil, "string differs without sentinel trace (" .. a .. " / " .. b .. ")"
		end
		if outB ~= b then
			return nil, "template verification failed (" .. a .. " / " .. b .. ")"
		end
		return table.concat(segs, "+")
	end
	if ta == "table" then
		local out = {}
		for k, va in pairs(a) do
			local vb = b[k]
			if vb == nil then
				return nil, "key only in probe A: " .. tostring(k)
			end
			local r, err = inferValue(va, vb, capsA, capsB)
			if r == nil and err then
				return nil, err
			end
			out[k] = r
		end
		for k in pairs(b) do
			if a[k] == nil then
				return nil, "key only in probe B: " .. tostring(k)
			end
		end
		return out
	end
	return nil, "unsupported value type " .. ta
end

-- vendor 调用约定（ModParser.lua:6518）：fn(tonumber(cap[1]), unpack(cap))
local function callSpecial(fn, caps)
	local ok, res, extra = pcall(fn, tonumber(caps[1] or ""), unpack(caps))
	if not ok then
		return nil, "closure call failed: " .. tostring(res)
	end
	if extra ~= nil then
		return nil, "closure returned extra value (partial-parse semantics)"
	end
	if type(res) ~= "table" then
		return nil, "closure returned non-table (" .. type(res) .. ")"
	end
	return res
end

----------------------------------------------------------------------
-- V2s5 词类捕获字典探针：词槽逐词试调闭包，命中集 = enums 闭集候选。
-- 域字典按 197 条 nonnumeric 语料归纳（伤害类型/资源/属性/充能/异常/
-- 技能种类/诅咒/分数词/部位/装备类/无时珠宝阵营/可选前缀）；字典外的
-- 开放词汇（触发技能名等）自然落空 → enum_no_dict_hit / 开放集在 Rust
-- 侧按 dict_size 判定跳过。
----------------------------------------------------------------------
local DICT_DAMAGE = { "physical", "fire", "cold", "lightning", "chaos", "elemental" }
local DICT_ALL = {
	-- 伤害类型
	"physical", "fire", "cold", "lightning", "chaos", "elemental",
	-- 资源/池
	"life", "mana", "energy shield", "spirit", "rage",
	-- 属性
	"strength", "dexterity", "intelligence", "attributes",
	-- 充能
	"power", "frenzy", "endurance",
	-- 异常/状态（含词形变体）
	"ignite", "ignited", "burning", "chill", "chilled", "freeze", "frozen",
	"shock", "shocked", "electrocute", "electrocuted", "bleed", "bleeding",
	"poison", "poisoned", "stun", "stunned", "daze", "dazed", "blind",
	"blinded", "maim", "maimed", "hinder", "hindered", "intimidate",
	"wither", "withered", "corrupted blood",
	-- 技能种类
	"attack", "attacks", "spell", "spells", "melee", "projectile", "area",
	"minion", "minions", "totem", "trap", "mine", "brand", "curse", "aura",
	"warcry", "herald", "mark",
	-- 诅咒（PoE2 + 传统词形）
	"flammability", "hypothermia", "conductivity", "despair", "enfeeble",
	"temporal chains", "vulnerability", "elemental weakness", "punishment",
	"frostbite",
	-- 分数词
	"quarter", "third", "half", "fifth", "tenth",
	-- 部位/方向
	"left", "right",
	-- 装备类
	"wand", "wands", "staff", "staves", "sceptre", "sceptres", "bow", "bows",
	"crossbow", "crossbows", "axe", "axes", "sword", "swords", "mace",
	"maces", "claw", "claws", "dagger", "daggers", "spear", "spears",
	"flail", "flails", "quiver", "quivers", "shield", "shields", "focus",
	"helmet", "gloves", "boots", "body armour", "ring", "rings", "amulet",
	"belt",
	-- 防御
	"armour", "evasion", "evasion rating", "accuracy",
	-- 无时珠宝阵营
	"karui", "maraketh", "templar", "vaal", "eternal empire",
	-- 可选前缀捕获（`(i?t?e?m? ?)` 等）的两个合法取值
	"item ", "also ", "",
}

----------------------------------------------------------------------
-- 主循环：逐 key 导出
----------------------------------------------------------------------
local emit = realPrint

local function emitEntry(pattern, kind, modsJson)
	emit('{"pattern":' .. jsonStr(pattern) .. ',"kind":' .. jsonStr(kind) .. ',"mods":' .. modsJson .. "}")
end

local function emitFailed(pattern, reason)
	emit('{"pattern":' .. jsonStr(pattern) .. ',"kind":"failed","reason":' .. jsonStr(reason) .. "}")
end

local total = 0
for pattern, value in pairs(U.specialModList) do
	total = total + 1
	if type(pattern) ~= "string" then
		warn("non-string specialModList key skipped: " .. tostring(pattern))
	elseif type(value) == "table" then
		local ok, json = pcall(encodeRaw, value)
		if ok then
			emitEntry(pattern, "static", json)
		else
			emitFailed(pattern, "encode failed: " .. tostring(json))
		end
	elseif type(value) == "function" then
		local slots = captureSlots(pattern)
		local nonNum = false
		for _, t in ipairs(slots) do
			if t ~= "num" then
				nonNum = true
			end
		end
		if nonNum then
			local wordSlots = {}
			for i, t in ipairs(slots) do
				if t ~= "num" then
					wordSlots[#wordSlots + 1] = i
				end
			end
			if #wordSlots > 3 then
				emitFailed(pattern, "nonnumeric_capture")
			else
				-- 3 词槽只跑伤害类型域（全字典立方组合数过大；语料里的
				-- 3 词槽 pattern 全是元素/伤害类型互换形态）。
				local dict = (#wordSlots >= 3) and DICT_DAMAGE or DICT_ALL
				local variants = {}
				local combo = {}
				local function probeCombo()
					local capsA, capsB = {}, {}
					local wi, ni = 0, 0
					for i = 1, #slots do
						if slots[i] == "num" then
							ni = ni + 1
							capsA[i] = NUM_A[ni]
							capsB[i] = NUM_B[ni]
						else
							wi = wi + 1
							capsA[i] = combo[wi]
							capsB[i] = combo[wi]
						end
					end
					local resA = callSpecial(value, capsA)
					if not resA then
						return
					end
					local resB = callSpecial(value, capsB)
					if not resB then
						return
					end
					local inferred = inferValue(resA, resB, capsA, capsB)
					if not inferred then
						return
					end
					local ok, json = pcall(encodeRaw, inferred)
					if not ok then
						return
					end
					local ws = {}
					for i, w in ipairs(combo) do
						ws[i] = jsonStr(w)
					end
					variants[#variants + 1] = '{"words":['
						.. table.concat(ws, ",")
						.. '],"mods":'
						.. json
						.. "}"
				end
				local function walk(depth)
					if depth > #wordSlots then
						probeCombo()
						return
					end
					for _, w in ipairs(dict) do
						combo[depth] = w
						walk(depth + 1)
					end
				end
				walk(1)
				if #variants == 0 then
					emitFailed(pattern, "enum_no_dict_hit")
				else
					local si = {}
					for i, s in ipairs(wordSlots) do
						si[i] = tostring(s)
					end
					emit('{"pattern":' .. jsonStr(pattern) .. ',"kind":"enum","dict_size":' .. #dict
						.. ',"word_slots":[' .. table.concat(si, ",")
						.. '],"variants":[' .. table.concat(variants, ",") .. "]}")
				end
			end
		else
			local capsA, capsB = {}, {}
			for i = 1, #slots do
				capsA[i] = NUM_A[i]
				capsB[i] = NUM_B[i]
			end
			local resA, errA = callSpecial(value, capsA)
			if not resA then
				emitFailed(pattern, errA)
			elseif #slots == 0 then
				-- 无捕获：单次求值即静态结果
				local ok, json = pcall(encodeRaw, resA)
				if ok then
					emitEntry(pattern, "inferred", json)
				else
					emitFailed(pattern, "encode failed: " .. tostring(json))
				end
			else
				local resB, errB = callSpecial(value, capsB)
				if not resB then
					emitFailed(pattern, errB)
				else
					local inferred, err = inferValue(resA, resB, capsA, capsB)
					if not inferred then
						emitFailed(pattern, "probe: " .. tostring(err))
					else
						local ok, json = pcall(encodeRaw, inferred)
						if ok then
							emitEntry(pattern, "inferred", json)
						else
							emitFailed(pattern, "encode failed: " .. tostring(json))
						end
					end
				end
			end
		end
	else
		warn("unsupported specialModList value type for " .. pattern .. ": " .. type(value))
		emitFailed(pattern, "unsupported value type " .. type(value))
	end
end
warn("specialModList keys visited: " .. total)
