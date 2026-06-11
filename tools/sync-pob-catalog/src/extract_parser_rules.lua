-- extract_parser_rules.lua — sync-pob-catalog `extract-lua --what parser-rules` 引导脚本
--
-- 用 **pob2-oracle 同款 headless 引导**（dofile HeadlessWrapper.lua，cwd 必须是
-- vendor src/、LUA_PATH 指向 runtime/lua，由 Rust 侧设置）加载完整 PoB2 环境，
-- 然后经 `debug.getupvalue` 从 `modLib.parseMod`（cache 包装）→ 内层 `parseMod`
-- 的 upvalue 拿到 **加载后的最终规则表**（M6 蓝图 §1.9：执行而非正则啃源码，P13；
-- regen/cost 等派生表已在加载期展开，照 dump）：
--
--   formList / modNameList / modFlagList / preFlagList / modTagList
--   + 小查找表 suffixTypes / dmgTypes / penTypes / regenTypes / degenTypes
--     / costTypes / baseCostTypes / flagTypes / unsupportedModList
--
-- specialModList / skillNameList / preSkillNameList **不抽**（分别归 M5b
-- special_mods.json 与 M6-T7 generated/special_derived.json）。
--
-- 抽取约定（蓝图 §1 裁决）：
--   * pattern / 短语原样保留 Lua pattern 语法（不翻译 regex）；
--   * ModFlag / KeywordFlag 掩码 → 按 bit 升序分解为原语名数组；
--     tag 内 skillType 数值 → 反查 SkillType 名（键改 skill_type）；
--   * 闭包条目 → 双哨兵探针推断占位符模板（`$n` / `$n:cap` / `$n:negate` /
--     `$n:mult(k)` / `$n:div(k)`，字符串拼接段用 `+` 连接）；推断失败落
--     handler_id（`<段名>:<pattern 稳定 hash 前 12 位>`）并 stderr 报原因。
--
-- 输出 JSONL（每行一个 `{"section":...}` 对象）到 stdout；排序、派生字段
-- （literal/anchored）、计数自检与整体文档 byte-stable 序列化由 Rust 侧完成。
--
-- 用法（由 Rust 侧经 stdin 注入执行；arg[1] 仅作 sanity check）：
--   cd <vendor_src>; LUA_PATH=... CI=true luajit - <vendor_src_dir>

local vendorSrc = arg and arg[1]
if not vendorSrc then
	io.stderr:write("usage: (cd <vendor_src>) luajit - <vendor_src_dir>\n")
	os.exit(2)
end

----------------------------------------------------------------------
-- headless 引导（静音 PoB 的 print/ConPrintf 噪音，保持 stdout 纯 JSONL）
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
for _, name in ipairs({
	"formList", "modNameList", "modFlagList", "preFlagList", "modTagList",
	"suffixTypes", "dmgTypes", "penTypes", "regenTypes", "degenTypes",
	"costTypes", "baseCostTypes", "flagTypes", "unsupportedModList",
}) do
	assert(type(U[name]) == "table", "parseMod upvalue table missing: " .. name)
end

local function firstToUpper(str)
	return (str:gsub("^%l", string.upper))
end

local function warn(msg)
	io.stderr:write("extract-parser-rules: " .. msg .. "\n")
end

----------------------------------------------------------------------
-- 枚举反查：ModFlag/KeywordFlag bit → 名、SkillType 数值 → 名
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

-- 仅纯单 bit 进反查表；复合掩码（如 vendor 预组合的 WeaponMelee 族）经逐 bit
-- 分解还原为原语名。同 bit 多别名取字典序小者并告警。
local function buildBitNames(enumTable, what)
	local bitNames = {}
	for name, value in pairs(enumTable) do
		if type(value) == "number" and value > 0 and isPowerOfTwo(value) then
			if bitNames[value] then
				warn(what .. " bit " .. string.format("%.0f", value) .. " has aliases: " .. bitNames[value] .. " / " .. name)
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
		if skillTypeName[value] then
			warn("SkillType value " .. value .. " has aliases: " .. skillTypeName[value] .. " / " .. name)
			if name < skillTypeName[value] then
				skillTypeName[value] = name
			end
		else
			skillTypeName[value] = name
		end
	end
end

-- 掩码 → bit 升序名数组（掩码不保留 vendor bor 书写序；bit 序确定性等价）
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
-- JSON 编码（确定性：对象键字典序；整数 %d、其余 %.17g）
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
		error("non-finite number in parser rules")
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

-- tag 表编码：skillType/modFlags/keywordFlags 三个键做枚举/掩码反查并改
-- snake_case（蓝图 §1.2/§1.3 schema 例），其余键名原样转录、值递归编码。
local encodeTagValue
local function encodeTagTable(tag)
	local keys = {}
	for k in pairs(tag) do
		if type(k) ~= "string" then
			error("non-string key in tag table: " .. tostring(k))
		end
		keys[#keys + 1] = k
	end
	table.sort(keys)
	local parts = {}
	for _, k in ipairs(keys) do
		local v = tag[k]
		if k == "skillType" and type(v) == "number" then
			local name = skillTypeName[v] or error("unknown SkillType value " .. v)
			parts[#parts + 1] = '"skill_type":' .. jsonStr(name)
		elseif k == "modFlags" and type(v) == "number" then
			local names = decomposeMask(v, modFlagBits, "ModFlag")
			local items = {}
			for i, n in ipairs(names) do
				items[i] = jsonStr(n)
			end
			parts[#parts + 1] = '"mod_flags":[' .. table.concat(items, ",") .. "]"
		elseif k == "keywordFlags" and type(v) == "number" then
			local names = decomposeMask(v, keywordFlagBits, "KeywordFlag")
			local items = {}
			for i, n in ipairs(names) do
				items[i] = jsonStr(n)
			end
			parts[#parts + 1] = '"keyword_flags":[' .. table.concat(items, ",") .. "]"
		else
			parts[#parts + 1] = jsonStr(k) .. ":" .. encodeTagValue(v)
		end
	end
	return "{" .. table.concat(parts, ",") .. "}"
end

encodeTagValue = function(v)
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
				parts[i] = encodeTagValue(item)
			end
			return "[" .. table.concat(parts, ",") .. "]"
		end
		return encodeTagTable(v)
	end
	error("unserializable value of type " .. tv)
end

----------------------------------------------------------------------
-- 效果字段编码（schema RuleEffectsDef，蓝图 §1.4 字段全集）
----------------------------------------------------------------------
local KNOWN_ENTRY_KEYS = {
	tag = true, tagList = true, flags = true, keywordFlags = true,
	playerTag = true, playerTagList = true,
	addToMinion = true, addToMinionTag = true, addToSkill = true,
	addToAura = true, onlyAddToBanners = true, newAura = true,
	newAuraOnlyAllies = true, applyToEnemy = true, actorEnemy = true,
	modSuffix = true,
}

-- flags/keywordFlags 字段：数值掩码 → 名数组（探针模板不会落在 flag 字段，
-- 落了即视为不可编码、由调用方降级 handler）
local function encodeFlagMask(v, bitNames, what)
	if type(v) ~= "number" then
		error(what .. " field is not a numeric mask")
	end
	local names = decomposeMask(v, bitNames, what)
	local items = {}
	for i, n in ipairs(names) do
		items[i] = jsonStr(n)
	end
	return "[" .. table.concat(items, ",") .. "]"
end

-- entry 表 → schema 字段 JSON 片段列表（不含 phrase/pattern）。
-- names = 数组部分（modNameList 的名集；其余表必须为空数组部分）。
local function effectsJsonFields(entry)
	local parts = {}
	-- 数组部分 = names
	if #entry > 0 then
		local items = {}
		for i, n in ipairs(entry) do
			if type(n) ~= "string" then
				error("non-string name in entry array part")
			end
			items[i] = jsonStr(n)
		end
		parts[#parts + 1] = '"names":[' .. table.concat(items, ",") .. "]"
	end
	for k in pairs(entry) do
		if type(k) == "string" and not KNOWN_ENTRY_KEYS[k] then
			error("unknown entry key `" .. k .. "`")
		end
	end
	if entry.flags and entry.flags ~= 0 then
		parts[#parts + 1] = '"flags":' .. encodeFlagMask(entry.flags, modFlagBits, "ModFlag")
	end
	if entry.keywordFlags and entry.keywordFlags ~= 0 then
		parts[#parts + 1] = '"keyword_flags":' .. encodeFlagMask(entry.keywordFlags, keywordFlagBits, "KeywordFlag")
	end
	-- 单 tag 与 tagList 归一为 tags 数组（顺序 = [tag] ++ tagList）
	local tags = {}
	if entry.tag then
		tags[#tags + 1] = entry.tag
	end
	if entry.tagList then
		for _, t in ipairs(entry.tagList) do
			tags[#tags + 1] = t
		end
	end
	if #tags > 0 then
		local items = {}
		for i, t in ipairs(tags) do
			items[i] = encodeTagTable(t)
		end
		parts[#parts + 1] = '"tags":[' .. table.concat(items, ",") .. "]"
	end
	local playerTags = {}
	if entry.playerTag then
		playerTags[#playerTags + 1] = entry.playerTag
	end
	if entry.playerTagList then
		for _, t in ipairs(entry.playerTagList) do
			playerTags[#playerTags + 1] = t
		end
	end
	if #playerTags > 0 then
		local items = {}
		for i, t in ipairs(playerTags) do
			items[i] = encodeTagTable(t)
		end
		parts[#parts + 1] = '"player_tags":[' .. table.concat(items, ",") .. "]"
	end
	if entry.addToMinion then
		parts[#parts + 1] = '"add_to_minion":true'
	end
	if entry.addToMinionTag then
		parts[#parts + 1] = '"add_to_minion_tags":[' .. encodeTagTable(entry.addToMinionTag) .. "]"
	end
	if entry.addToAura then
		parts[#parts + 1] = '"add_to_aura":true'
	end
	if entry.onlyAddToBanners then
		parts[#parts + 1] = '"only_add_to_banners":true'
	end
	if entry.newAura then
		parts[#parts + 1] = '"new_aura":true'
	end
	if entry.newAuraOnlyAllies then
		parts[#parts + 1] = '"new_aura_only_allies":true'
	end
	if entry.addToSkill then
		parts[#parts + 1] = '"add_to_skill":' .. encodeTagTable(entry.addToSkill)
	end
	if entry.applyToEnemy then
		parts[#parts + 1] = '"apply_to_enemy":true'
	end
	if entry.actorEnemy then
		parts[#parts + 1] = '"actor_enemy":true'
	end
	if entry.modSuffix then
		parts[#parts + 1] = '"mod_suffix":' .. jsonStr(entry.modSuffix)
	end
	return parts
end

----------------------------------------------------------------------
-- 闭包探针推断（蓝图 §1.9：双哨兵 + 结构 diff → 占位符模板）
----------------------------------------------------------------------
local NUM_A = { "73", "79", "83", "89", "101" }
local NUM_B = { "97", "103", "107", "109", "113" }
local STR_A = { "qzxa", "qzxb", "qzxc", "qzxd", "qzxe" }
local STR_B = { "wvka", "wvkb", "wvkc", "wvkd", "wvke" }

-- 从 Lua pattern 解析捕获组类型："num"（含 %d）/ "str"（字母类）/ "any"（.+ 等）
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

-- 哨兵 pattern 稳定 hash（FNV 不便于 double 精度，用双 djb2 模大素数拼 12 hex）
local function stableHash12(s)
	local h1, h2 = 5381, 52711
	for i = 1, #s do
		local c = s:byte(i)
		h1 = (h1 * 33 + c) % 2147483647
		h2 = (h2 * 33 + c) % 2147483629
	end
	return (string.format("%08x%08x", h1, h2)):sub(1, 12)
end

-- 联合遍历两次探针输出，推断模板值；失败返回 nil + 原因。
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
				-- 加性偏移（如 vendor `threshold = num - 1`）→ $i:base(c)
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
		-- 按哨兵原文 / 首字母大写形切分模板段
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
		-- 用 B 哨兵实例化验证 + 字面段保留字检查
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

-- kind: "tag_phrase"（vendor 调用约定：首参为数字化 cap1）/ "pre_flag"
local function probeClosure(kind, pattern, fn)
	local slots = captureSlots(pattern)
	local function buildCaps(numSet, strSet, anyAsNum)
		local caps = {}
		for i, t in ipairs(slots) do
			if t == "num" or (t == "any" and anyAsNum) then
				caps[i] = numSet[i]
			else
				caps[i] = strSet[i]
			end
		end
		return caps
	end
	local function call(caps)
		if kind == "tag_phrase" and #slots > 0 then
			local first = caps[1]:match("%d+") and tonumber(caps[1]) or caps[1]
			return pcall(fn, first, caps[1], caps[2], caps[3], caps[4], caps[5])
		end
		return pcall(fn, caps[1], caps[2], caps[3], caps[4], caps[5])
	end
	-- 先按 slot 分类探，失败则把 any 槽位翻成数字重试
	for _, anyAsNum in ipairs({ false, true }) do
		local capsA = buildCaps(NUM_A, STR_A, anyAsNum)
		local capsB = buildCaps(NUM_B, STR_B, anyAsNum)
		local okA, resA = call(capsA)
		local okB, resB = call(capsB)
		if okA and okB then
			if type(resA) ~= "table" or type(resB) ~= "table" then
				return nil, "closure returned non-table for sentinel captures"
			end
			local inferred, err = inferValue(resA, resB, capsA, capsB)
			if inferred then
				return inferred
			end
			return nil, err
		end
		if not (okA or okB) and anyAsNum then
			return nil, "closure call failed: " .. tostring(resA)
		end
	end
	return nil, "closure call failed under all sentinel typings"
end

----------------------------------------------------------------------
-- 各段输出
----------------------------------------------------------------------
local emit = realPrint

-- formList：value 恒为 form id 字符串
for pattern, form in pairs(U.formList) do
	if type(form) ~= "string" then
		error("formList value is not a string for " .. pattern)
	end
	emit('{"section":"forms","pattern":' .. jsonStr(pattern) .. ',"form":' .. jsonStr(form) .. "}")
end

-- modNameList：string / 名数组 / 带效果的表
for phrase, value in pairs(U.modNameList) do
	local fields
	if type(value) == "string" then
		fields = { '"names":[' .. jsonStr(value) .. "]" }
	elseif type(value) == "table" then
		fields = effectsJsonFields(value)
	else
		error("unsupported modNameList value type for " .. phrase)
	end
	emit('{"section":"name_map","phrase":' .. jsonStr(phrase) .. "," .. table.concat(fields, ",") .. "}")
end

-- modFlagList：恒为表条目
for phrase, value in pairs(U.modFlagList) do
	local fields = effectsJsonFields(value)
	emit('{"section":"flag_phrases","phrase":' .. jsonStr(phrase) .. "," .. table.concat(fields, ",") .. "}")
end

-- preFlagList / modTagList：表条目直出；闭包条目走探针推断
local function emitPatternSection(section, kindPrefix, tbl, closureKind)
	for pattern, value in pairs(tbl) do
		if type(value) == "function" then
			local inferred, err = probeClosure(closureKind, pattern, value)
			local fields, encodeErr
			if inferred then
				local ok, result = pcall(effectsJsonFields, inferred)
				if ok then
					fields = result
				else
					encodeErr = tostring(result)
				end
			end
			if fields then
				fields[#fields + 1] = '"inferred":true'
				emit('{"section":"' .. section .. '","pattern":' .. jsonStr(pattern) .. "," .. table.concat(fields, ",") .. "}")
			else
				local handlerId = kindPrefix .. ":" .. stableHash12(pattern)
				warn("closure inference failed [" .. section .. "] " .. pattern .. " → " .. handlerId .. " (" .. tostring(err or encodeErr) .. ")")
				emit('{"section":"' .. section .. '","pattern":' .. jsonStr(pattern) .. ',"handler_id":' .. jsonStr(handlerId) .. "}")
			end
		elseif type(value) == "table" then
			local fields = effectsJsonFields(value)
			if #fields > 0 then
				emit('{"section":"' .. section .. '","pattern":' .. jsonStr(pattern) .. "," .. table.concat(fields, ",") .. "}")
			else
				emit('{"section":"' .. section .. '","pattern":' .. jsonStr(pattern) .. "}")
			end
		else
			error("unsupported " .. section .. " value type for " .. pattern)
		end
	end
end
emitPatternSection("pre_flags", "pre_flag", U.preFlagList, "pre_flag")
emitPatternSection("tag_phrases", "tag_phrase", U.modTagList, "tag_phrase")

-- 小查找表：短语 → 单名
local function emitPhraseValue(section, tbl)
	for phrase, value in pairs(tbl) do
		if type(value) ~= "string" then
			error(section .. " value is not a string for " .. phrase)
		end
		emit('{"section":"' .. section .. '","phrase":' .. jsonStr(phrase) .. ',"value":' .. jsonStr(value) .. "}")
	end
end
emitPhraseValue("suffix_types", U.suffixTypes)
emitPhraseValue("damage_types", U.dmgTypes)
emitPhraseValue("pen_types", U.penTypes)

-- 小查找表：短语 → 名集（resource 派生四表，值 string 或数组）
local function emitPhraseNames(section, tbl)
	for phrase, value in pairs(tbl) do
		local names = {}
		if type(value) == "string" then
			names[1] = jsonStr(value)
		elseif type(value) == "table" then
			for i, n in ipairs(value) do
				names[i] = jsonStr(n)
			end
		else
			error(section .. " value type unsupported for " .. phrase)
		end
		emit('{"section":"' .. section .. '","phrase":' .. jsonStr(phrase) .. ',"names":[' .. table.concat(names, ",") .. "]}")
	end
end
emitPhraseNames("regen_types", U.regenTypes)
emitPhraseNames("degen_types", U.degenTypes)
emitPhraseNames("cost_types_map", U.costTypes)
emitPhraseNames("base_cost_types", U.baseCostTypes)

-- flagTypes：string（condition）或 hexproof 特例 mod 表
for phrase, value in pairs(U.flagTypes) do
	if type(value) == "string" then
		emit('{"section":"flag_types","phrase":' .. jsonStr(phrase) .. ',"condition":' .. jsonStr(value) .. "}")
	elseif type(value) == "table" then
		assert(type(value.name) == "string" and type(value.type) == "string" and type(value.value) == "number",
			"unexpected flagTypes mod shape for " .. phrase)
		emit('{"section":"flag_types","phrase":' .. jsonStr(phrase)
			.. ',"mod":{"name":' .. jsonStr(value.name)
			.. ',"mod_type":' .. jsonStr(value.type)
			.. ',"value":' .. jsonNum(value.value) .. "}}")
	else
		error("unsupported flagTypes value for " .. phrase)
	end
end

-- unsupportedModList：键集
for phrase, flag in pairs(U.unsupportedModList) do
	if flag then
		emit('{"section":"unsupported","phrase":' .. jsonStr(phrase) .. "}")
	end
end
