-- extract_skill_overrides.lua — sync-pob-catalog `extract-lua` 的引导脚本
--
-- 在**最小 stub 环境**下加载 vendor/PathOfBuilding-PoE2 的 Data/Skills/<file>.lua
-- （这些文件以 `local skills, mod, flag, skill = ...` 接收注入，顶层只引用
-- SkillType/ModFlag/KeywordFlag 三个全局；参考 HeadlessWrapper 的思路但不全量启动），
-- 抽取两类 per-skill 覆盖值并以 JSONL（每行一个 JSON 对象）写到 stdout：
--
--   base_multiplier         ← levels[*].baseMultiplier
--   skill_attack_speed_more ← statSets[*].baseMods 中 mod("Speed", "MORE", <number>, ...)
--   dot_is_*                ← statSets[*].baseMods 中 skill("dotIs*", true)（M4-T4 W-D1：
--                             技能 DoT 的 dotCfg flag 剥除布尔；`.dat` 无对应列）
--   explode_corpse          ← statSets[*].baseMods 中 skill("explodeCorpse", true)（M4-G：
--                             尸体爆炸基伤门控布尔，CalcOffence.lua:2213；`.dat` 无对应列）
--   implicit_stat           ← statSets[*].stats 中**任何等级行都没有数值**的条目
--                             （= GGG `.dat` GrantedEffectStatSets.ImplicitStats 列，适配器
--                             未下载；vendor 消费值恒 1，CalcTools.lua:152
--                             `statSetLevel[index] or 1`）。M4-H：**策展白名单**抽取——
--                             vendor 全量隐式 stat 4394 条多为显示/行为噪声，仅抽取
--                             PoBR statmap→calc 有消费方的 stat（见 IMPLICIT_STAT_KEYS）
--
-- 通道收窄（M1-T4.3）：critChance / attackSpeedMultiplier **不再抽取**——二者已是
-- `.dat` 表列（GrantedEffectStatSetsPerLevel 暴击两列 / GrantedEffectsPerLevel
-- AttackSpeedMultiplier），由 pobr-data-adapter 直读落 base/（历史 overlay 值
-- 3911 + 3578 条逐值一致验收后切换）。本边车仅保留 `.dat` 通道中确实拿不到的部分
-- （statSet baseMods 常量；baseMultiplier 分等级值的表列直读归 T5 多 statSet 改造）。
--
-- 确定性约定：本脚本只负责"忠实抽取 + 合法 JSON"；最终排序、数字格式与整体
-- 文档（_meta + overrides）的 byte-stable 序列化由 Rust 侧统一完成。
--
-- 用法（由 Rust 侧经 stdin 注入执行）：
--   luajit - <vendor_src_dir> <逗号分隔的技能文件名（不含 .lua 后缀）>

local vendorSrc = arg and arg[1]
local fileListArg = arg and arg[2]
if not vendorSrc or not fileListArg then
	io.stderr:write("usage: luajit - <vendor_src_dir> <file1,file2,...>\n")
	os.exit(2)
end

----------------------------------------------------------------------
-- 最小 stub 环境：值的具体语义无关紧要（skillTypes 只拿来当 table key、
-- flags 只透传），用 __index 自映射即可让数据文件顶层求值通过。
----------------------------------------------------------------------
SkillType = setmetatable({}, { __index = function(_, k) return k end })
ModFlag = setmetatable({}, { __index = function(_, k) return k end })
KeywordFlag = setmetatable({}, { __index = function(_, k) return k end })

-- 与 vendor Modules/Data.lua 的 makeSkillMod/makeFlagMod/makeSkillDataMod 同形：
-- 把 mod(...) 调用捕获为纯表，便于之后按 name/type 过滤。
local function makeSkillMod(modName, modType, modVal, flags, keywordFlags, ...)
	return {
		name = modName,
		type = modType,
		value = modVal,
		flags = flags or 0,
		keywordFlags = keywordFlags or 0,
		...,
	}
end
local function makeFlagMod(modName, ...)
	return makeSkillMod(modName, "FLAG", true, 0, 0, ...)
end
local function makeSkillDataMod(dataKey, dataValue, ...)
	return makeSkillMod("SkillData", "LIST", { key = dataKey, value = dataValue }, 0, 0, ...)
end

----------------------------------------------------------------------
-- 加载目标数据文件（vendor 只读，不做任何修改）
----------------------------------------------------------------------
local skills = {}
for fileName in string.gmatch(fileListArg, "[^,]+") do
	local path = vendorSrc .. "/Data/Skills/" .. fileName .. ".lua"
	local chunk, loadErr = loadfile(path)
	if not chunk then
		io.stderr:write("cannot load " .. path .. ": " .. tostring(loadErr) .. "\n")
		os.exit(3)
	end
	local ok, runErr = pcall(chunk, skills, makeSkillMod, makeFlagMod, makeSkillDataMod)
	if not ok then
		io.stderr:write("error executing " .. path .. ": " .. tostring(runErr) .. "\n")
		os.exit(3)
	end
end

----------------------------------------------------------------------
-- JSON 工具（仅需字符串转义 + 数字；结构很浅，无需通用编码器）
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
		error("non-finite number in skill data")
	end
	return string.format("%.17g", v)
end

local function sortedNumericKeys(t)
	local keys = {}
	for k in pairs(t) do
		if type(k) == "number" then
			keys[#keys + 1] = k
		end
	end
	table.sort(keys)
	return keys
end

----------------------------------------------------------------------
-- 抽取并输出 JSONL
----------------------------------------------------------------------
-- levels 内 vendor 字段名 → 入库 stat 名（M1-T4.3 收窄：crit/attspd 改表列直读，见头注）
local LEVEL_STATS = {
	{ vendorKey = "baseMultiplier", stat = "base_multiplier" },
}

local function emitLevelStat(skillId, skill, vendorKey, statName)
	local levels = skill.levels
	if type(levels) ~= "table" then
		return
	end
	local levelKeys = sortedNumericKeys(levels)
	local rows = {}
	for _, lvl in ipairs(levelKeys) do
		local entry = levels[lvl]
		if type(entry) == "table" and type(entry[vendorKey]) == "number" then
			rows[#rows + 1] = { level = lvl, value = entry[vendorKey] }
		end
	end
	if #rows == 0 then
		return
	end
	-- 压缩条件：该 stat 在 vendor **所有等级**均出现且同值 → 压缩为单 value
	-- （消费侧把单 value 应用到该技能全部等级行）。只覆盖部分等级（如
	-- RisenArbalestSnipe 仅 L1 有 baseMultiplier）或值随等级变化 → 保留
	-- per_level 明细，避免把缺失等级误标为有值。
	local constant = #rows == #levelKeys
	for i = 2, #rows do
		if rows[i].value ~= rows[1].value then
			constant = false
			break
		end
	end
	local prefix = '{"skill":"' .. jsonEscape(skillId) .. '","stat":"' .. statName .. '"'
	if constant then
		print(prefix .. ',"value":' .. jsonNum(rows[1].value) .. "}")
	else
		local parts = {}
		for i, row in ipairs(rows) do
			parts[i] = "[" .. jsonNum(row.level) .. "," .. jsonNum(row.value) .. "]"
		end
		print(prefix .. ',"per_level":[' .. table.concat(parts, ",") .. "]}")
	end
end

-- statSets[*].baseMods 中的 Speed MORE（如 Flicker Strike 的 285）。
-- 同一 statSet 出现多条时只取第一条并告警——入库 schema 当前是单值。
local function emitStatSetSpeedMore(skillId, skill)
	local statSets = skill.statSets
	if type(statSets) ~= "table" then
		return
	end
	for _, setIndex in ipairs(sortedNumericKeys(statSets)) do
		local set = statSets[setIndex]
		local found = nil
		if type(set) == "table" and type(set.baseMods) == "table" then
			for _, m in ipairs(set.baseMods) do
				if type(m) == "table" and m.name == "Speed" and m.type == "MORE" and type(m.value) == "number" then
					if found then
						io.stderr:write(
							"warning: multiple Speed MORE baseMods on "
								.. skillId
								.. " statSet "
								.. setIndex
								.. ", keeping first\n"
						)
					else
						found = m.value
					end
				end
			end
		end
		if found then
			print(
				'{"skill":"'
					.. jsonEscape(skillId)
					.. '","stat":"skill_attack_speed_more","stat_set":'
					.. string.format("%d", setIndex)
					.. ',"value":'
					.. jsonNum(found)
					.. "}"
			)
		end
	end
end

-- statSets[*].baseMods 中的 skill("dotIs*", true) 布尔（M4-T4 W-D1）。
-- vendor 0.18.0 全量 Data/Skills 仅 TornadoShotPlayer statSets[2] 一处
-- （dotIsArea）；dotIsSpell/dotIsProjectile 走 SkillStatMap（stat 驱动，
-- 已入 overlay/skill_stat_map.json 的 skill_data kind），不经本通道。
-- 抽取按固定键序保证确定性；value 恒 1（vendor 只标 true）。
local DOT_FLAG_KEYS = {
	{ vendorKey = "dotIsArea", stat = "dot_is_area" },
	{ vendorKey = "dotIsProjectile", stat = "dot_is_projectile" },
	{ vendorKey = "dotIsSpell", stat = "dot_is_spell" },
	{ vendorKey = "dotIsAttack", stat = "dot_is_attack" },
	{ vendorKey = "dotIsHit", stat = "dot_is_hit" },
	-- M4-G：尸体爆炸门控（vendor `skill("explodeCorpse", true)`，0.18.0 全量
	-- Data/Skills 共 6 技能：DetonateDeadPlayer / CorpseCloudPlayer /
	-- DetonateLivingPlayer / AncientGiftsPlayer / TriggeredLightningDetonateDeadPlayer /
	-- TriggeredFieryDeathPlayer）。复用 baseMods 布尔抽取通道。
	{ vendorKey = "explodeCorpse", stat = "explode_corpse" },
}

local function emitStatSetDotFlags(skillId, skill)
	local statSets = skill.statSets
	if type(statSets) ~= "table" then
		return
	end
	for _, setIndex in ipairs(sortedNumericKeys(statSets)) do
		local set = statSets[setIndex]
		if type(set) == "table" and type(set.baseMods) == "table" then
			for _, spec in ipairs(DOT_FLAG_KEYS) do
				local found = false
				for _, m in ipairs(set.baseMods) do
					-- skill(...) 经 makeSkillDataMod 捕获为
					-- { name = "SkillData", type = "LIST", value = { key, value } }。
					if
						type(m) == "table"
						and m.name == "SkillData"
						and type(m.value) == "table"
						and m.value.key == spec.vendorKey
						and m.value.value == true
					then
						found = true
					end
				end
				if found then
					print(
						'{"skill":"'
							.. jsonEscape(skillId)
							.. '","stat":"'
							.. spec.stat
							.. '","stat_set":'
							.. string.format("%d", setIndex)
							.. ',"value":1}'
					)
				end
			end
		end
	end
end

-- M4-H：隐式 stat 策展白名单（PoBR statmap→calc 消费链已接通的 stat id）。
-- 当前 = crit/lucky flag 族（statmap → BifurcateCrit / LuckyHits，消费点
-- calc::crit::resolve_crit / calc::damage::lucky_hit_chance）：
--   attacks_roll_crits_twice             （Garukhan's Resolve，sup_dex.lua:2421）
--   attack_damage_is_lucky_if_surrounded （Defy I/II，sup_str）
--   no_critical_strike_multiplier        （statmap → NoCritMultiplier；当前 vendor
--                                          无隐式实例，前瞻保留）
local IMPLICIT_STAT_KEYS = {
	"attack_damage_is_lucky_if_surrounded",
	"attacks_roll_crits_twice",
	"no_critical_strike_multiplier",
}

local function emitStatSetImplicitStats(skillId, skill)
	local statSets = skill.statSets
	if type(statSets) ~= "table" then
		return
	end
	local wanted = {}
	for _, key in ipairs(IMPLICIT_STAT_KEYS) do
		wanted[key] = true
	end
	for _, setIndex in ipairs(sortedNumericKeys(statSets)) do
		local set = statSets[setIndex]
		if type(set) == "table" and type(set.stats) == "table" then
			-- 收集该 set 中无任何等级数值的白名单 stat（按白名单固定序输出，确定性）。
			local implicit = {}
			for index, stat in ipairs(set.stats) do
				if wanted[stat] then
					local hasValue = false
					if type(set.levels) == "table" then
						for _, lvl in pairs(set.levels) do
							if type(lvl) == "table" and lvl[index] ~= nil then
								hasValue = true
								break
							end
						end
					end
					if not hasValue then
						implicit[stat] = true
					end
				end
			end
			for _, key in ipairs(IMPLICIT_STAT_KEYS) do
				if implicit[key] then
					print(
						'{"skill":"'
							.. jsonEscape(skillId)
							.. '","stat":"implicit_stat","stat_set":'
							.. string.format("%d", setIndex)
							.. ',"stat_id":"'
							.. jsonEscape(key)
							.. '"}'
					)
				end
			end
		end
	end
end

for skillId, skill in pairs(skills) do
	if type(skill) == "table" then
		for _, spec in ipairs(LEVEL_STATS) do
			emitLevelStat(skillId, skill, spec.vendorKey, spec.stat)
		end
		emitStatSetSpeedMore(skillId, skill)
		emitStatSetDotFlags(skillId, skill)
		emitStatSetImplicitStats(skillId, skill)
	end
end
