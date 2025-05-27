local util = require('odools.utils')
local M = {}

local lsp_config = require('lspconfig.configs')

if not lsp_config then
    vim.api.nvim_err_writeln("lsp_config not available")
    return
end

M.setup = function(opt)
    opt = opt or {}
    opt.python_path = opt.python_path or '/usr/bin/python3'
    opt.server_path = opt.server_path or util.get_server_path()
    util.check_config(opt)
    local odoo_path = opt.odoo_path
    opt.root_dir = opt.root_dir or odoo_path
    local odooConfig = {
        id = 1,
        name = "main config",
        validatedAddonsPaths = opt.addons or {},
        addons = opt.addons or {},
        odooPath = odoo_path,
        finalPythonPath = opt.python_path,
        additional_stubs = opt.additional_stubs or {},
    }
    local server_path = opt.server_path
    lsp_config.odools = {
        default_config = {
            name = 'odools',
            cmd = {server_path},
            root_dir = function() return vim.fn.fnamemodify(opt.server_path, ":h") end,
            workspace_folders = {
                {
                    uri = function() return opt.root_dir end,
                    name = function() return "base_workspace" end,
                },
            },
            filetypes = { 'python' },
            settings = {
                Odoo = {
                    autoRefresh = opt.settings and opt.settings.autoRefresh or true,
                    autoRefreshDelay = opt.settings and opt.settings.autoRefreshDelay or nil,
                    diagMissingImportLevel = opt.settings and opt.settings.diagMissingImportLevel or "none",
                    configurations = { mainConfig = odooConfig },
                    selectedConfiguration = "mainConfig",
                },
            },
            capabilities = {
                textDocument = {
                    workspace = {
                        symbol = {
                            dynamicRegistration = true,
                        },
                    },
                },
            },
        },
    }
    lsp_config.odools.setup {}
end
return M
