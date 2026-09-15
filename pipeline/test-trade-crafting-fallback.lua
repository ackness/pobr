-- Requires the pinned vendor; run from the repository root.
local vendor = "vendor/PathOfBuilding-PoE2/src"
local trade = dofile("pipeline/trade-stats.lua")
local entries = trade.load(vendor)
local recover = dofile("pipeline/trade-crafting-fallback.lua").load(vendor, entries, trade)
local chance = recover({ stats = {{ id = "surpassing_chance_%_to_gain_1_puppeteer_stack_on_using_command_skill", min = 30, max = 50 }} })
assert(#chance == 1 and chance[1].id == "explicit.stat_2840930496" and chance[1].value == 50)
assert(chance[1].line == "50% Surpassing Chance to gain a Puppet Master stack whenever you use a Command Skill")
local nodes = recover({ stats = {{ id = "mod_granted_passive_hash_essence", min = 0, max = 0 }} })
assert(#nodes == 875)
for _, stat in ipairs(nodes) do
    assert(stat.id:match("^explicit%.stat_2954116742|%d+$") and stat.value == 1 and #stat.value_indices == 0)
end
assert(not entries["explicit.stat_2954116742"] and not entries["explicit.stat_7338"])
assert(not recover({ stats = {{ id = "unknown", min = 1, max = 2 }} }))
assert(not recover({ stats = {{ id = "mod_granted_passive_hash_essence" }, { id = "another_stat" }} }))
print("trade crafting fallback: numeric rolls, passive options and missing data checks passed")
