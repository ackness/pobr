-- PoB2 headless oracle：buildSkillInstanceStats 的**品质叠加段**中间值对拍（M1-T1）。
--
-- 对给定 (skillId, level, quality) 在 PoB2 真实数据 + 真实框架函数
-- （calcLib.buildSkillInstanceStats，Modules/CalcTools.lua:138）下分别以
-- quality=Q 与 quality=0 各跑一遍，输出两表的**逐 stat 差值** = 品质贡献
-- （即 PoBR `BuildData::effect_stats(..).quality` 段应得的值）。
--
-- 纯包装：不修改任何 vendor 源码。须从 vendor src/ 目录运行（同 oracle.lua）：
--   cd vendor/PathOfBuilding-PoE2/src
--   LUA_PATH="../runtime/lua/?.lua;../runtime/lua/?/init.lua;./?.lua;;" CI=true \
--     luajit ../../../tools/pob2-oracle/quality_stats.lua <skillId> <level> <quality> [...]
--
-- 参数为 (skillId, level, quality) 三元组的重复序列；JSON 输出到 stdout。

if #arg < 3 or #arg % 3 ~= 0 then
	io.stderr:write("usage: quality_stats.lua <skillId> <level> <quality> [<skillId> <level> <quality> ...]\n")
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

assert(type(calcLib) == "table" and calcLib.buildSkillInstanceStats, "calcLib not loaded")
assert(type(data) == "table" and data.skills, "data.skills not loaded")

local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

local out = { "[" }
local first = true
for i = 1, #queries, 3 do
	local skillId, level, quality = queries[i], tonumber(queries[i + 1]), tonumber(queries[i + 2])
	local grantedEffect = data.skills[skillId]
	assert(grantedEffect, "unknown skill id: " .. tostring(skillId))
	local statSet = grantedEffect.statSets[1]
	assert(statSet, "skill has no statSets[1]: " .. skillId)
	local withQ = calcLib.buildSkillInstanceStats(
		{ level = level, quality = quality, actorLevel = 90 }, grantedEffect, statSet)
	local without = calcLib.buildSkillInstanceStats(
		{ level = level, quality = 0, actorLevel = 90 }, grantedEffect, statSet)
	-- 逐 stat 差值（quality 贡献）；按 stat 名排序保证输出确定性。
	local keys = {}
	for stat in pairs(withQ) do keys[#keys + 1] = stat end
	table.sort(keys)
	local parts = {}
	for _, stat in ipairs(keys) do
		local diff = (withQ[stat] or 0) - (without[stat] or 0)
		if diff ~= 0 then
			parts[#parts + 1] = string.format('"%s":%.17g', jsonEscape(stat), diff)
		end
	end
	out[#out + 1] = (first and "" or ",")
		.. string.format(
			'{"skill":"%s","level":%d,"quality":%d,"quality_contribution":{%s}}',
			jsonEscape(skillId), level, quality, table.concat(parts, ","))
	first = false
end
out[#out + 1] = "]"
realPrint(table.concat(out, "\n"))
