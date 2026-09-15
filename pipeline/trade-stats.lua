-- Shared interpretation of PoB2 trade hashes. A hash can describe several exported lines.
local M = {}
local opposites = { increased = "reduced", reduced = "increased", more = "less", less = "more", slower = "faster", faster = "slower" }
local function wording(line)
    return line:lower():gsub("%(%a+%)", ""):gsub("[#()0-9%-%+%.]", ""):gsub("%s+", " "):gsub("^%s+", ""):gsub("%s+$", "")
end

function M.maximum(line)
    return (line:gsub("%((%-?%d+%.?%d*)%-(%-?%d+%.?%d*)%)", function(_, high) return high end))
end

function M.load(vendor)
    local entries = {}
    for _, category in ipairs(dofile(vendor .. "/Data/TradeSiteStats.lua")) do
        for _, entry in ipairs(category.entries or {}) do
            entries[entry.id] = entry
            -- Pipes select an option (e.g. a passive node), not another stat hash.
            -- Comma-separated hashes are aliases for a single numeric trade stat.
            local namespace, hashes = entry.id:match("^(%w+)%.stat_([%d,]+)$")
            if namespace then
                for hash in hashes:gmatch("%d+") do entries[namespace .. ".stat_" .. hash] = entry end
            end
        end
    end
    return entries
end

function M.extract(mod, namespace, entries)
    local hashes, result = {}, {}
    for hash in pairs(mod.tradeHashes or {}) do hashes[#hashes + 1] = hash end
    table.sort(hashes)
    for _, hash in ipairs(hashes) do
        local id = namespace .. ".stat_" .. string.format("%.0f", hash)
        local entry = entries[id]
        local source_lines = {}
        for _, line in ipairs(mod.tradeHashes[hash]) do source_lines[#source_lines + 1] = M.maximum(line) end
        local line = table.concat(source_lines, " "):gsub("%s+", " ")
        if not entry then
            -- Fixed option wording (e.g. jewel radius) identifies one full ID.
            -- A numeric base hash alone cannot select an arbitrary option.
            for option_id, option in pairs(entries) do
                if option_id:sub(1, #id + 1) == id .. "|" and option.text == line then
                    if entry then entry = nil; break end
                    entry = option
                end
            end
        end
        if entry then
            local numbers = {}
            for number in line:gmatch("%-?%d+%.?%d*") do numbers[#numbers + 1] = tonumber(number) end
            local indices, token_count, constants_match = {}, 0, true
            for token in entry.text:gmatch("%-?[#%d][%d%.]*") do
                if token:find("#", 1, true) then
                    indices[#indices + 1] = token_count
                elseif numbers[token_count + 1] ~= tonumber(token) then
                    constants_match = false
                end
                token_count = token_count + 1
            end
            -- No-placeholder stats use one unit even when PoB exports a numeric
            -- roll. Numeric query stats require aligned variable/constant tokens.
            local singular = line == "You can apply an additional Curse" or line == "Bow Attacks fire an additional Arrow"
            if #indices <= 2 and (#indices == 0 or (#numbers == token_count and constants_match) or (singular and token_count == 1))
                and not line:find("Grants Level") and not line:find("inflict Decay") then
                local value = 0
                for _, index in ipairs(indices) do value = value + (numbers[index + 1] or 1) end
                value = #indices > 0 and value / #indices or 1
                local original, canonical = wording(line), wording(entry.text)
                local inverse = original ~= canonical and original:gsub("%a+", opposites) == canonical
                if value ~= 0 then
                    local stat = { id = entry.id, line = line, value = inverse and -value or value }
                    if #indices == 0 or #numbers == token_count then
                        local variable = 0
                        local trade_line = entry.text:gsub("#", function()
                            variable = variable + 1
                            local number = numbers[indices[variable] + 1]
                            return tostring(inverse and -number or number)
                        end):gsub("%s+", " ")
                        if trade_line ~= line then stat.trade_line = trade_line end
                    end
                    if #source_lines > 1 then stat.source_lines = source_lines end
                    -- Old packs use first-number / flat-damage-average semantics.
                    -- Record exact variable positions for compound and conditional stats.
                    if #numbers == 0 or #indices == 0 then stat.value_indices = setmetatable({}, { __jsontype = "array" })
                    elseif #numbers > 1 then stat.value_indices = indices end
                    result[#result + 1] = stat
                end
            end
        end
    end
    return result
end

return M
