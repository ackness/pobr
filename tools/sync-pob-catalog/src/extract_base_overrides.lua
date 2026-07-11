-- extract_base_overrides.lua — sync-pob-catalog `extract-bases` 的引导脚本
--
-- 在**最小 stub 环境**下加载 vendor/PathOfBuilding-PoE2 的 Data/Bases/<file>.lua
-- （这些文件以 `local itemBases = ...` 接收注入，顶层为纯表字面量赋值，无其它
-- 全局依赖），抽取两个 GGG `.dat` 路线不可得的基底列并以 JSONL 写到 stdout：
--
--   block_chance   ← itemBases[name].armour.BlockChance     （`ShieldTypes.Block`）
--   spirit         ← itemBases[name].spirit                  （`ItemSpirit.SpiritGranted`）
--   reload_time_ms ← itemBases[name].weapon.ReloadTimeBase×1000（`WeaponTypes.ReloadTime`，
--                    M4-T4 W-D2 弩装填；vendor 值为秒，毫秒整数入库）
--   charm_buff     ← itemBases[name].charm.buff（charm 基底固有 buff 词条数组，
--                    如 Ruby Charm "+25% to Fire Resistance"；空串占位过滤）
--   tags           ← itemBases[name].tags（完整基底 tag 集：GGG `.it` 元数据继承链
--                    合并产物，.dat 的 BaseItemTypes.Tags 只有末端 tag（如
--                    dex_int_armour），缺 body_armour/armour 等类目 tag——词缀
--                    spawn weight 判定必需；键排序保证确定性）
--
-- 确定性约定：本脚本只负责「忠实抽取 + 合法 JSON」；最终按 name 排序与整体
-- 文档（_meta + overrides）的 byte-stable 序列化由 Rust 侧统一完成。
--
-- 用法（由 Rust 侧经 stdin 注入执行）：
--   luajit - <vendor_src_dir> <逗号分隔的基底文件名（不含 .lua 后缀）>

local vendorSrc = arg and arg[1]
local fileListArg = arg and arg[2]
if not vendorSrc or not fileListArg then
	io.stderr:write("usage: luajit - <vendor_src_dir> <file1,file2,...>\n")
	os.exit(2)
end

----------------------------------------------------------------------
-- 加载目标数据文件（vendor 只读，不做任何修改）
----------------------------------------------------------------------
local itemBases = {}
for fileName in string.gmatch(fileListArg, "[^,]+") do
	local path = vendorSrc .. "/Data/Bases/" .. fileName .. ".lua"
	local chunk, loadErr = loadfile(path)
	if not chunk then
		io.stderr:write("cannot load " .. path .. ": " .. tostring(loadErr) .. "\n")
		os.exit(3)
	end
	local ok, runErr = pcall(chunk, itemBases)
	if not ok then
		io.stderr:write("error executing " .. path .. ": " .. tostring(runErr) .. "\n")
		os.exit(3)
	end
end

----------------------------------------------------------------------
-- JSON 工具（仅需字符串转义 + 数字；结构很浅，无需通用编码器）
----------------------------------------------------------------------
local function jsonEscape(s)
	return (s:gsub('[%z\1-\31\\"]', function(c)
		local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
		return map[c] or string.format("\\u%04x", c:byte())
	end))
end

-- %.17g 保证 f64 精度无损往返；Rust 侧 serde_json 会重排为最短表示
local function jsonNum(v)
	if v ~= v or v == math.huge or v == -math.huge then
		error("non-finite number in base item data")
	end
	return string.format("%.17g", v)
end

----------------------------------------------------------------------
-- 抽取并输出 JSONL（只输出至少携带一个目标字段的基底）
----------------------------------------------------------------------
for name, base in pairs(itemBases) do
	if type(base) == "table" then
		local blockChance = nil
		if type(base.armour) == "table" and type(base.armour.BlockChance) == "number" then
			blockChance = base.armour.BlockChance
		end
		local spirit = nil
		if type(base.spirit) == "number" then
			spirit = base.spirit
		end
		local reloadMs = nil
		if type(base.weapon) == "table" and type(base.weapon.ReloadTimeBase) == "number" then
			-- vendor 秒值（bases.lua 导出时已 /1000 round 两位）→ 毫秒整数。
			reloadMs = math.floor(base.weapon.ReloadTimeBase * 1000 + 0.5)
		end
		-- charm 基底固有 buff 词条数组（如 Ruby Charm "+25% to Fire Resistance"）；
		-- 过滤空串（Cleansing Charm 的占位 buff = { "" }）。无有效行 → nil。
		local charmBuff = nil
		if type(base.charm) == "table" and type(base.charm.buff) == "table" then
			local lines = {}
			for _, line in ipairs(base.charm.buff) do
				if type(line) == "string" and line ~= "" then
					lines[#lines + 1] = '"' .. jsonEscape(line) .. '"'
				end
			end
			if #lines > 0 then
				charmBuff = "[" .. table.concat(lines, ",") .. "]"
			end
		end
		-- 完整 tag 集（键排序去随机性；空表不输出）。
		local tagsJson = nil
		if type(base.tags) == "table" then
			local keys = {}
			for tag, enabled in pairs(base.tags) do
				if enabled and type(tag) == "string" then
					keys[#keys + 1] = tag
				end
			end
			table.sort(keys)
			if #keys > 0 then
				for i, tag in ipairs(keys) do
					keys[i] = '"' .. jsonEscape(tag) .. '"'
				end
				tagsJson = "[" .. table.concat(keys, ",") .. "]"
			end
		end
		if blockChance or spirit or reloadMs or charmBuff or tagsJson then
			local parts = { '"name":"' .. jsonEscape(name) .. '"' }
			if blockChance then
				parts[#parts + 1] = '"block_chance":' .. jsonNum(blockChance)
			end
			if spirit then
				parts[#parts + 1] = '"spirit":' .. string.format("%d", spirit)
			end
			if reloadMs then
				parts[#parts + 1] = '"reload_time_ms":' .. string.format("%d", reloadMs)
			end
			if charmBuff then
				parts[#parts + 1] = '"charm_buff":' .. charmBuff
			end
			if tagsJson then
				parts[#parts + 1] = '"tags":' .. tagsJson
			end
			print("{" .. table.concat(parts, ",") .. "}")
		end
	end
end
