-- Recover recipes omitted from ModItem using pinned descriptions and official IDs.
-- Accept only unambiguous single-stat descriptions; never infer a trade hash.
local M = {}

function M.load(vendor, entries, trade)
    local descriptions = {}
    for _, group in pairs(dofile(vendor .. "/Data/StatDescriptions/stat_descriptions.lua")) do
        if type(group) == "table" and group.stats and #group.stats == 1 then
            descriptions[group.stats[1]] = group[1]
        end
    end
    local tree = dofile(dofile("pipeline/vendor-tree.lua").path(vendor))
    return function(recipe)
        if not recipe.stats or #recipe.stats ~= 1 then return end
        local raw = recipe.stats[1]
        local descriptions_for_stat = descriptions[raw.id]
        if not descriptions_for_stat or #descriptions_for_stat ~= 1 then return end
        local description = descriptions_for_stat[1]
        if description.text == "Allocates {0}" and description[1] and description[1].k == "passive_hash" then
            local stats = {}
            for id, entry in pairs(entries) do
                local node_id = id:match("^explicit%.stat_%d+|(%d+)$")
                local node = node_id and tree.nodes[tonumber(node_id)]
                if node and entry.text == "Allocates " .. node.name then
                    stats[#stats + 1] = { id = id, line = entry.text, value = 1, kind = "granted_passive",
                        value_indices = setmetatable({}, { __jsontype = "array" }) }
                end
            end
            table.sort(stats, function(a, b) return a.id < b.id end)
            return stats
        end
        -- Transforms, alternate limits and composite values need their own handler.
        if #description ~= 0 or not description.limit or #description.limit ~= 1
            or description.limit[1][1] ~= "#" or description.limit[1][2] ~= "#" then return end
        local template, variables = description.text:gsub("{0}", "#")
        if variables ~= 1 then return end
        local matched
        for id, entry in pairs(entries) do
            if id == entry.id and id:match("^explicit%.stat_[%d,]+$") and entry.text == template then
                if matched then return end
                matched = entry
            end
        end
        if matched and raw.max ~= 0 then
            local hash = tonumber(matched.id:match("stat_(%d+)"))
            local line = description.text:gsub("{0}", tostring(raw.max))
            return trade.extract({ tradeHashes = { [hash] = { line } } }, "explicit", { ["explicit.stat_" .. string.format("%.0f", hash)] = matched })
        end
    end
end

return M
