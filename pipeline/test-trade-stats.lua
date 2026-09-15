-- Run from the repository root: luajit pipeline/test-trade-stats.lua
local trade = dofile("pipeline/trade-stats.lua")
local function extract(text, lines)
    return trade.extract({ tradeHashes = { [123] = lines } }, "explicit", {
        ["explicit.stat_123"] = { id = "explicit.stat_123", text = text },
    })[1]
end
local flat = extract("Adds # to # Fire Damage", { "Adds (2-6) to (12-30) Fire Damage" })
assert(flat.value == 18 and flat.value_indices[1] == 0 and flat.value_indices[2] == 1)
local wrapped = extract("Inflict Anaemia on Hit Anaemia allows +# Corrupted Blood debuffs to be inflicted on enemies", {
    "Inflict Anaemia on Hit", "Anaemia allows +(2-3) Corrupted Blood debuffs to be inflicted on enemies",
})
assert(wrapped.value == 3 and #wrapped.source_lines == 2 and not wrapped.line:find("\n"))
local conditional = extract("Every 4 seconds, gain #% increased Damage", { "Every 4 seconds, gain (10-20)% increased Damage" })
assert(conditional.value == 20 and conditional.value_indices[1] == 1 and #conditional.value_indices == 1)
assert(not extract("Every 4 seconds, gain #% increased Damage", { "Every 6 seconds, gain 20% increased Damage" }))
assert(not extract("Gain # Charges", { "Gain Charges" }))
local flag = extract("Your Hits can't be Evaded", { "Your Hits can't be Evaded" })
assert(flag.value == 1 and #flag.value_indices == 0 and getmetatable(flag.value_indices).__jsontype == "array")
local rolled_flag = extract("Dazes on Hit", { "(5-10)% chance to Daze on Hit" })
assert(rolled_flag.value == 1 and #rolled_flag.value_indices == 0 and rolled_flag.trade_line == "Dazes on Hit")
local arrow = extract("Bow Attacks fire # additional Arrows", { "Bow Attacks fire an additional Arrow" })
assert(arrow.value == 1 and #arrow.value_indices == 0)
local inverse = extract("#% increased Mana Cost", { "(5-10)% reduced Mana Cost" })
assert(inverse.value == -10 and inverse.trade_line == "-10% increased Mana Cost")
local mixed = extract("#% increased Damage while Regeneration is reduced", { "20% increased Damage while Regeneration is reduced" })
assert(mixed.value == 20)
assert(extract("#% increased Attack Speed (Local)", { "15% reduced Attack Speed" }).value == -15)
local alias = extract("#% increased Spirit Reservation Efficiency of Skills", { "(8-12)% increased Spirit Reservation Efficiency" })
assert(alias.trade_line == "12% increased Spirit Reservation Efficiency of Skills")
assert(not extract("Grants Level # Fireball", { "Grants Level 20 Fireball" }))
assert(not extract("# Damage per # Mana per # Strength", { "10 Damage per 20 Mana per 30 Strength" }))
local options = {
    ["explicit.stat_123|1"] = { id = "explicit.stat_123|1", text = "Upgrades Radius to Medium" },
    ["explicit.stat_123|2"] = { id = "explicit.stat_123|2", text = "Upgrades Radius to Large" },
}
assert(trade.extract({ tradeHashes = { [123] = { "Upgrades Radius to Medium" } } }, "explicit", options)[1].id == "explicit.stat_123|1")
assert(#trade.extract({ tradeHashes = { [123] = { "Unknown Radius" } } }, "explicit", options) == 0)
print("trade stats: compound, conditional, flag, inverse and alias checks passed")
