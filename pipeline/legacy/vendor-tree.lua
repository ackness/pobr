-- Resolve the tree selected by the pinned vendor, without a league literal.
local M = {}

function M.path(vendor)
    local environment = {}
    local versions = assert(loadfile(vendor .. "/GameVersions.lua"))
    setfenv(versions, environment)()
    local version = environment.latestTreeVersion
    assert(type(version) == "string" and version:match("^%d+_%d+$"), "invalid latestTreeVersion")
    local path = vendor .. "/TreeData/" .. version .. "/tree.lua"
    local file = assert(io.open(path, "r"), "missing selected vendor tree: " .. version)
    file:close()
    return path
end

if arg and arg[0] and arg[0]:match("vendor%-tree%.lua$") then
    print(M.path(assert(arg[1], "expected vendor src directory")))
end

return M
