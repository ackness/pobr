-- Extract tree identities, radius selectors and deterministic conquest data.
-- Numerical rolls and node identities come from the pinned vendor, never Rust.
local vendor, output = arg[1], arg[2]
assert(vendor and output, "usage: extract-passive-jewels.lua <vendor_src> <output.json>")
package.path = vendor .. "/../runtime/lua/?.lua;" .. package.path
local json = require("dkjson")
local function read(path)
    local f = assert(io.open(path, "r"))
    local value = f:read("*a")
    f:close()
    return value
end
local env = {}
local versions = assert(loadfile(vendor .. "/GameVersions.lua"))
setfenv(versions, env)
versions()
local version = assert(env.latestTreeVersion)
local tree = dofile(vendor .. "/TreeData/" .. version .. "/tree.lua")
local legion = dofile(vendor .. "/Data/TimelessJewelData/LegionPassives.lua")
local parser = read(vendor .. "/Modules/ModParser.lua")
local spec = read(vendor .. "/Classes/PassiveSpec.lua")
local result = { _meta = { schema = "passive-jewels/v1",
    source = "PoB2 GameVersions, TreeData, ModParser, LegionPassives and PassiveSpec",
    seed_tables = "unavailable",
    regen_command = "luajit pipeline/extract-passive-jewels.lua <vendor_src> <output.json>" },
    tree_version = version, class_starts = {}, ring_sizes = {},
    conquerors = {}, nodes = {}, families = {} }
for _, node in pairs(tree.nodes) do
    for _, class in ipairs(node.classesStart or {}) do
        result.class_starts[class:lower()] = assert(node.stringId)
    end
end
for pattern, index in parser:gmatch('%["([^"]+)"%]%s*=%s*{%s*mod%("JewelData",%s*"LIST",%s*{%s*key%s*=%s*"radiusIndex",%s*value%s*=%s*(%d+)') do
    local literal = pattern:gsub("%%%-", "-")
    assert(not literal:find("[%%%(%)%[%]%*%+%?%$%^]"), "non-literal ring selector: " .. pattern)
    result.ring_sizes[literal] = tonumber(index)
end
local conquerors = assert(parser:match("local conquerorList%s*=%s*(%b{})"))
for name, id, family in conquerors:gmatch('%["([^"]+)"%]%s*=%s*{%s*id%s*=%s*(%d+),%s*type%s*=%s*"([^"]+)"') do
    result.conquerors[name] = { family = family, keystone = family .. "_keystone_" .. id }
    result.families[family] = { attribute_additions = setmetatable({}, { __jsontype = "array" }) }
end
for _, node in ipairs(legion.nodes) do
    local family = node.id:match("^([^_]+)_")
    if result.families[family] then
        result.nodes[node.id] = { name = node.dn, stats = node.sd }
    end
end
-- This is a procedural upstream consumer. Fail on a changed shape instead of
-- silently carrying a stale numeric constant into a new data snapshot.
local abyss = spec:match('if conqueredBy%.conqueror%.type == "abyss" then%s*if isValueInArray%(attributes, node%.dn%) then(.-)self:ReconnectNodeToClassStart')
if result.families.abyss then
    assert(abyss, "abyss small-passive transformation changed")
    local addition = assert(abyss:match('NodeAdditionOrReplacementFromString%(node, "(.-)"%)'))
    addition = addition:gsub("\\n", "\n"):match("^%s*(.-)%s*$")
    local index = tonumber(assert(abyss:match("local legionNode = legionNodes%[(%d+)%]")))
    result.families.abyss.attribute_additions = { addition }
    result.families.abyss.small_replacement = assert(legion.nodes[index]).id
end
assert(next(result.class_starts) and next(result.ring_sizes) and next(result.conquerors))
for _, conqueror in pairs(result.conquerors) do assert(result.nodes[conqueror.keystone]) end
local keys, seen = {}, {}
local function collect_keys(value)
    for key, child in pairs(value) do
        if type(key) == "string" and not seen[key] then keys[#keys + 1] = key; seen[key] = true end
        if type(child) == "table" then collect_keys(child) end
    end
end
collect_keys(result)
table.sort(keys)
local out = assert(io.open(output, "w"))
out:write(json.encode(result, { indent = true, keyorder = keys }), "\n")
out:close()
