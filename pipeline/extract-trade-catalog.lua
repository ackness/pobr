-- Export rollable affixes and base categories for trade/combination search.
-- Source: PoB2 TradeQueryGenerator.canModSpawnForItemCategory / ModItem / Bases.
local vendor, output = arg[1], arg[2]
assert(vendor and output, "usage: extract-trade-catalog.lua <vendor_src> <out.json>")
package.path = vendor .. "/../runtime/lua/?.lua;" .. package.path
local json = require("dkjson")

local function sorted_keys(t)
    local keys = {}
    for key in pairs(t) do keys[#keys + 1] = key end
    table.sort(keys)
    return keys
end

local function maximum(line)
    return (line:gsub("%((%-?%d+%.?%d*)%-(%-?%d+%.?%d*)%)", function(_, high) return high end))
end

local categories = {
    ["Amulet"] = "accessory.amulet", ["Ring"] = "accessory.ring", ["Belt"] = "accessory.belt",
    ["Body Armour"] = "armour.chest", ["Helmet"] = "armour.helmet", ["Gloves"] = "armour.gloves",
    ["Boots"] = "armour.boots", ["Quiver"] = "armour.quiver", ["Shield"] = "armour.shield",
    ["Focus"] = "armour.focus", ["Buckler"] = "armour.buckler", ["Bow"] = "weapon.bow",
    ["Crossbow"] = "weapon.crossbow", ["Staff"] = "weapon.staff", ["Talisman"] = "weapon.talisman",
    ["One Hand Mace"] = "weapon.onemace", ["Two Hand Mace"] = "weapon.twomace",
    ["Wand"] = "weapon.wand", ["Sceptre"] = "weapon.sceptre", ["Spear"] = "weapon.spear",
    ["Flail"] = "weapon.flail", ["Jewel"] = "jewel", ["Charm"] = "flask.charm",
}
local raw_bases = {}
for _, file in ipairs({ "amulet", "belt", "body", "boots", "bow", "crossbow", "flail", "focus",
    "gloves", "helmet", "mace", "quiver", "ring", "sceptre", "shield", "spear", "staff", "talisman", "wand", "jewel", "flask" }) do
    dofile(vendor .. "/Data/Bases/" .. file .. ".lua")(raw_bases)
end
local bases = {}
for _, name in ipairs(sorted_keys(raw_bases)) do
    local base = raw_bases[name]
    local category = categories[base.type]
    if base.type == "Staff" and base.subType == "Warstaff" then category = "weapon.warstaff" end
    if base.type == "Flask" then category = base.subType == "Life" and "flask.life" or "flask.mana" end
    local domain = base.type == "Jewel" and "jewel" or base.type == "Charm" and "charm"
        or base.type == "Flask" and "flask" or "equipment"
    if category and not base.hidden and not (base.tags or {}).not_for_sale then
        local implicits = {}
        for line in (base.implicit or ""):gmatch("[^\n]+") do implicits[#implicits + 1] = maximum(line) end
        bases[#bases + 1] = { name = name, category = category, tags = sorted_keys(base.tags or {}),
            level = (base.req or {}).level or 1, implicits = implicits, domain = domain,
            affix_limit = domain == "equipment" and 3 or domain == "jewel" and 2 or 1 }
    end
end
local valid_stats = {}
for _, category in ipairs(dofile(vendor .. "/Data/TradeSiteStats.lua")) do
    for _, entry in ipairs(category.entries or {}) do
        if entry.type == "explicit" then
            local hash = entry.id:match("^explicit%.stat_(%d+)$")
            if hash then valid_stats[tonumber(hash)] = entry end
        end
    end
end
local raw_mods = {}
for _, source in ipairs({ { "equipment", "ModItem" }, { "jewel", "ModJewel" }, { "flask", "ModFlask" }, { "charm", "ModCharm" } }) do
    for id, mod in pairs(dofile(vendor .. "/Data/" .. source[2] .. ".lua")) do
        mod.domain = source[1]
        raw_mods[source[1] .. ":" .. id] = mod
    end
end
local mods = {}
for _, id in ipairs(sorted_keys(raw_mods)) do
    local mod = raw_mods[id]
    if (mod.type == "Prefix" or mod.type == "Suffix") and mod.group and #(mod.weightKey or {}) > 0 then
        local lines, weights, stats = {}, {}, {}
        for _, line in ipairs(mod) do lines[#lines + 1] = maximum(line) end
        for i, tag in ipairs(mod.weightKey) do weights[#weights + 1] = {tag, mod.weightVal[i]} end
        for _, hash in ipairs(sorted_keys(mod.tradeHashes or {})) do
            local source = mod.tradeHashes[hash]
            local entry = valid_stats[hash]
            -- Each trade weight must represent its own stat, not the entire hybrid affix.
            if entry and #source == 1 then
                local line = maximum(source[1])
                local low, high = line:match("(%-?%d+%.?%d*) to (%-?%d+%.?%d*)")
                local value = low and (tonumber(low) + tonumber(high)) / 2 or tonumber(line:match("%-?%d+%.?%d*"))
                if value and value ~= 0 then
                    local inverse = (line:find("increased") and entry.text:find("reduced"))
                        or (line:find("reduced") and entry.text:find("increased"))
                        or (line:find("more") and entry.text:find("less"))
                        or (line:find("less") and entry.text:find("more"))
                    stats[#stats + 1] = { id = entry.id, line = line, value = inverse and -value or value }
                end
            end
        end
        mods[#mods + 1] = { id = id, group = mod.group, kind = mod.type:lower(), level = mod.level or 1,
            lines = lines, weights = weights, stats = stats, domain = mod.domain }
    end
end
-- Load data constructors for level requirements and support compatibility.
-- The same minimal enum stubs are used by sync-pob-catalog's skill extractors.
SkillType = setmetatable({}, { __index = function(_, key) return key end })
local function flag_enum()
    local next_bit = 1
    return setmetatable({}, { __index = function(t, key)
        local value = next_bit
        next_bit = next_bit * 2
        rawset(t, key, value)
        return value
    end })
end
ModFlag, KeywordFlag = flag_enum(), flag_enum()
local raw_skills = {}
local function unused_mod() return {} end
for _, file in ipairs({ "act_dex", "act_int", "act_str", "sup_dex", "sup_int", "sup_str" }) do
    local constructor = assert(loadfile(vendor .. "/Data/Skills/" .. file .. ".lua"))
    local result = constructor(raw_skills, unused_mod, unused_mod, unused_mod)
    if type(result) == "function" then result(raw_skills, unused_mod, unused_mod, unused_mod) end
end
local gems = {}
local raw_gems = dofile(vendor .. "/Data/Gems.lua")
for _, id in ipairs(sorted_keys(raw_gems)) do
    local gem = raw_gems[id]
    if gem.grantedEffectId and gem.name then
        local requirements = {}
        local effect = raw_skills[gem.grantedEffectId]
        local levels = (effect or {}).levels or {}
        local maximum_level = (gem.naturalMaxLevel or 20) + (gem.gemType == "Support" and 0 or 1)
        for level = 1, maximum_level do
            if not levels[level] or levels[level].levelRequirement == nil then break end
            requirements[level] = levels[level].levelRequirement
        end
        gems[#gems + 1] = { skill_id = gem.grantedEffectId, name = gem.name, family = gem.gemFamily or gem.name,
            is_support = gem.gemType == "Support", max_level = gem.naturalMaxLevel or 20,
            level_requirements = requirements,
            is_lineage = (gem.tags or {}).lineage == true,
            compatibility_known = effect ~= nil,
            skill_types = sorted_keys((effect or {}).skillTypes or {}),
            require_skill_types = (effect or {}).requireSkillTypes or {},
            exclude_skill_types = (effect or {}).excludeSkillTypes or {},
            add_skill_types = (effect or {}).addSkillTypes or {},
            support_gems_only = (effect or {}).supportGemsOnly == true,
            cannot_be_supported = (effect or {}).cannotBeSupported == true,
            families = (effect or {}).gemFamily or { gem.gemFamily or gem.name } }
    end
end
local result = { _meta = { source = "PoB2 ModItem/ModJewel/ModFlask/ModCharm, Bases, Gems, Skills and TradeSiteStats",
    regen_command = "luajit pipeline/extract-trade-catalog.lua vendor/PathOfBuilding-PoE2/src <out>" },
    bases = bases, mods = mods, gems = gems }
-- This vendored dkjson tests truthiness when honoring keyorder. Wrap false only
-- for serialization so boolean fields keep deterministic order across processes.
local json_false = setmetatable({}, { __tojson = function() return "false" end })
for _, gem in ipairs(gems) do
    for key, value in pairs(gem) do
        if value == false then gem[key] = json_false end
    end
end
local f = assert(io.open(output, "w"))
f:write(json.encode(result, { indent = true, keyorder = {
    "_meta", "source", "regen_command", "bases", "mods", "gems", "id", "name", "category", "tags",
    "group", "kind", "level", "implicits", "domain", "affix_limit", "lines", "weights", "stats", "line", "value",
    "skill_id", "family", "is_support", "max_level", "level_requirements", "is_lineage",
    "compatibility_known", "skill_types", "require_skill_types", "exclude_skill_types", "add_skill_types",
    "support_gems_only", "cannot_be_supported", "families",
} }), "\n")
f:close()
print(string.format("trade catalog: %d bases, %d affixes", #bases, #mods))
