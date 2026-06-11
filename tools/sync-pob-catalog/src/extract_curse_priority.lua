-- extract_curse_priority.lua — `extract-lua --what curse-priority` 的引导脚本
--
-- `data.cursePriority` 是 vendor Modules/Data.lua:274 起的**纯数据表字面量**
-- （per-curse 基值 + SocketPriorityBase + 装备槽名权重 +
-- CurseFromAura/CurseFromEquipment；消费侧 = determineCursePriority，
-- CalcPerform.lua:454-485）。Data.lua 整体依赖完整 PoB 环境无法 dofile，
-- 本脚本沿用 extract_catalysts.lua 的做法：读源码文本，用 `%b{}` 平衡匹配
-- 截取 `data.cursePriority = {...}` 表字面量后 loadstring 执行（字面量自身
-- 是合法 Lua，luajit 求值而非正则啃值；截取失败 = 源码结构变化，显式报错）。
--
-- 输出 JSONL：每行 `{"name":...,"priority":N}`（vendor 平铺 k=v 原样转发）；
-- 分类（curse 基值 / 槽权重 / 来源权重）、排序与 `_meta` 组装在 Rust 侧
-- （extract_curse_priority.rs）。
--
-- 用法：luajit - <vendor_src_dir> Data（文件名固定 Modules/Data.lua）

local vendorSrc = arg and arg[1]
if not vendorSrc then
	io.stderr:write("usage: luajit - <vendor_src_dir> Data\n")
	os.exit(2)
end

local path = vendorSrc .. "/Modules/Data.lua"
local f, openErr = io.open(path, "r")
if not f then
	io.stderr:write("cannot open " .. path .. ": " .. tostring(openErr) .. "\n")
	os.exit(3)
end
local source = f:read("*a")
f:close()

local literal = source:match("data%.cursePriority = (%b{})")
if not literal then
	io.stderr:write("cannot locate `data.cursePriority = {...}` in " .. path .. "\n")
	os.exit(3)
end
local chunk, loadErr = loadstring("return " .. literal)
if not chunk then
	io.stderr:write("cannot load cursePriority literal: " .. tostring(loadErr) .. "\n")
	os.exit(3)
end
local cursePriority = chunk()

local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

local count = 0
for name, priority in pairs(cursePriority) do
	-- vendor 表恒为 string → 正整数；形态漂移（嵌套表 / 浮点）显式报错而非静默丢弃
	if type(name) ~= "string" or type(priority) ~= "number" or priority ~= math.floor(priority) then
		io.stderr:write(
			"unexpected cursePriority entry: " .. tostring(name) .. " = " .. tostring(priority) .. "\n"
		)
		os.exit(3)
	end
	print('{"name":"' .. jsonEscape(name) .. '","priority":' .. string.format("%d", priority) .. "}")
	count = count + 1
end
if count == 0 then
	io.stderr:write("cursePriority table is empty\n")
	os.exit(3)
end
