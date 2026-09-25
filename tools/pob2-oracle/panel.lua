-- Export the COMBAT convention separately from the EFFECTIVE main output.
-- Run from the pinned vendor's src directory with the same LUA_PATH as run.sh.
local input = assert(io.open(arg[1], "r"))
local xml = input:read("*a")
input:close()
local outputPath = assert(arg[2], "output path required")

print = function() end
dofile("HeadlessWrapper.lua")
loadBuildFromXML(xml, "panel parity")
-- Saved CALCS selections can differ from the exported main skill/stat set.
-- Keep all skill selections identical; only the calculation mode may change.
build.calcsTab.input.skill_number = build.mainSocketGroup
for _, group in ipairs(build.skillsTab.socketGroupList) do
	group.mainActiveSkillCalcs = group.mainActiveSkill
	for _, gem in ipairs(group.gemList) do
		for _, key in ipairs({ "statSet", "skillPart", "skillStageCount", "skillMineCount",
			"skillMinion", "skillMinionItemSet", "skillMinionSkill", "skillMinionSkillStatSetIndexLookup" }) do
			gem[key .. "Calcs"] = type(gem[key]) == "table" and copyTable(gem[key]) or gem[key]
		end
	end
end
build.calcsTab.input.misc_buffMode = "COMBAT"
build.calcsTab:BuildOutput()
assert(not build.calcsTab.calcsEnv.mode_effective)
assert(build.calcsTab.calcsEnv.mode_combat)

local keys = { "CritChance", "CritMultiplier", "Speed", "AverageDamage", "TotalDPS" }
local function offensiveStats(output)
	local stats = {}
	for _, key in ipairs(keys) do
		stats[key] = output[key]
	end
	return stats
end
local result = {
	panel = offensiveStats(build.calcsTab.calcsOutput),
	effective = offensiveStats(build.calcsTab.mainOutput),
}
-- The CALCS selector must describe the same skill as the main-panel fixture.
build.calcsTab.input.misc_buffMode = "EFFECTIVE"
build.calcsTab:BuildOutput()
result.calcsEffective = offensiveStats(build.calcsTab.calcsOutput)
local output = assert(io.open(outputPath, "w"))
output:write(require("dkjson").encode(result))
output:close()
