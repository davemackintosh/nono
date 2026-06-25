local function get_script_file()
	local str = debug.getinfo(1, "S").source:sub(2)
	-- Normalize Windows paths if necessary
	if package.config:sub(1, 1) == '\\' then
		str = str:gsub('/', '\\')
	end
	return str
end

local cwd = get_script_file()

-- Register local grammar
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
parser_config.your_language = {
	install_info = {
		url = cwd,
		files = { "src/parser.c", "src/scanner.c" }, -- Include scanner if present
		generate_requires_npm = false,
		requires_generate_from_grammar = false,
	},
	filetype = "nono",
}

-- Ensure the parser is installed/updated
vim.api.nvim_create_autocmd("User", {
	pattern = "TSUpdate",
	callback = function()
		-- Re-register in case of restart
		local pc = require("nvim-treesitter.parsers").get_parser_configs()
		if not pc.your_language then
			pc.your_language = {
				install_info = {
					url = cwd,
					files = { "src/parser.c" },
				},
			}
		end
	end,
})
