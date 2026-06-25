-- tree-sitter-nono / neovim.lua
--
-- Source this to install and enable the Nono grammar, with no dependency on
-- nvim-treesitter: it compiles the committed parser and loads it through
-- Neovim's built-in tree-sitter.
--
--   :source /path/to/tree-sitter-nono/neovim.lua
--
-- or, from your init.lua:
--
--   dofile(vim.fn.expand("~/.../tree-sitter-nono/neovim.lua"))
--
-- It compiles src/parser.c to nono.so next to this file (only when missing or
-- out of date), registers the language and its highlight query, associates the
-- .nono filetype, and switches highlighting on for nono buffers.

local M = {}

local uv = vim.uv or vim.loop

-- Absolute path to the directory holding this script (the grammar root, which
-- contains src/parser.c). `:p` so it works even when sourced via a relative path.
local script = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p")
local dir = vim.fs.dirname(script)

local parser_c = vim.fs.joinpath(dir, "src", "parser.c")
local so = vim.fs.joinpath(dir, "nono.so")
local query_file = vim.fs.joinpath(dir, "queries", "highlights.scm")

-- Compile src/parser.c -> nono.so, but only when the .so is missing or older
-- than the source. Returns true if there's a usable .so afterwards.
local function ensure_compiled()
	local src_stat = uv.fs_stat(parser_c)
	if not src_stat then
		vim.notify("nono: cannot find " .. parser_c, vim.log.levels.ERROR)
		return false
	end
	local so_stat = uv.fs_stat(so)
	if so_stat and so_stat.mtime.sec >= src_stat.mtime.sec then
		return true
	end

	local cc = (vim.fn.executable("cc") == 1 and "cc")
		or (vim.fn.executable("clang") == 1 and "clang")
		or (vim.fn.executable("gcc") == 1 and "gcc")
	if not cc then
		vim.notify("nono: no C compiler (cc/clang/gcc) to build the parser", vim.log.levels.ERROR)
		return false
	end

	local result = vim
		.system({
			cc,
			"-o",
			so,
			"-shared",
			"-fPIC",
			"-Os",
			"-I",
			vim.fs.joinpath(dir, "src"),
			parser_c,
		}, { text = true })
		:wait()
	if result.code ~= 0 then
		vim.notify("nono: parser build failed\n" .. (result.stderr or ""), vim.log.levels.ERROR)
		return false
	end
	-- Only fires when a rebuild actually happened (stale or missing .so), so it's
	-- the breadcrumb you want after pulling a grammar change.
	vim.notify("nono: parser (re)compiled", vim.log.levels.INFO)
	return true
end

-- Force a clean rebuild. tree-sitter can't hot-swap a language that's already
-- loaded, so this recompiles and asks you to restart nvim to pick it up. Use it
-- after pulling a grammar change if highlighting looks stale.
function M.rebuild()
	os.remove(so)
	if ensure_compiled() then
		vim.notify("nono: rebuilt. Restart nvim (:qa) to load the new parser.", vim.log.levels.WARN)
	end
end

function M.setup()
	if not ensure_compiled() then
		return
	end

	-- Load the compiled language by explicit path, no runtimepath gymnastics.
	local ok, err = pcall(vim.treesitter.language.add, "nono", { path = so })
	if not ok then
		vim.notify("nono: failed to load parser: " .. tostring(err), vim.log.levels.ERROR)
		return
	end

	-- Register the highlight query straight from this repo.
	local f = io.open(query_file, "r")
	if f then
		local q = f:read("*a")
		f:close()
		pcall(vim.treesitter.query.set, "nono", "highlights", q)
	end

	-- .nono files are the `nono` filetype.
	vim.filetype.add({ extension = { nono = "nono" } })

	-- `:NonoRebuild` to force a recompile after a grammar change.
	vim.api.nvim_create_user_command("NonoRebuild", function()
		M.rebuild()
	end, { desc = "Recompile the Nono tree-sitter parser" })

	-- Switch highlighting on for nono buffers, now and in future.
	vim.api.nvim_create_autocmd("FileType", {
		pattern = "nono",
		callback = function(ev)
			pcall(vim.treesitter.start, ev.buf, "nono")
		end,
	})

	-- Catch any nono files already open when this was sourced: re-set their
	-- filetype, which fires the autocmd above.
	for _, buf in ipairs(vim.api.nvim_list_bufs()) do
		if vim.api.nvim_buf_is_loaded(buf) and vim.api.nvim_buf_get_name(buf):match("%.nono$") then
			vim.bo[buf].filetype = "nono"
		end
	end
end

M.setup()

return M
