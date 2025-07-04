# Neovim

Neovim client for the Odools language server

![screenshot](https://i.imgur.com/wuqsF9q.png)

## Important ⚠️
This plugin is still in its early development stage. Don't hesitate to submit bugs, issues and/or
feedbacks to improve the user experience.

## Installation
### requirement
We recommend using nvim version `0.9.0` or later. This plugin is using
[lspconfig](https://github.com/neovim/nvim-lspconfig) to connect communicate with the language
server in your beloved editor.

### downloads
 1. Install the plugin

>Installing the neovim plugin is done through the entire Odools repositery. You need to specify to your package manager the `nvim` subfolder that must be added in the runtimepath. Check the examples listed below ⚠️

Using [packer.nvim](https://github.com/wbthomason/packer.nvim)

```lua
use {
  'odoo/odoo-ls',
   requires = { {'neovim/nvim-lspconfig'} },
   rtp = 'nvim/',
}
```

Using [lazy.nvim](https://github.com/folke/lazy.nvim)

```lua
-- init.lua:
    {
    'odoo/odoo-ls',
    dependencies = { 'neovim/nvim-lspconfig' },
    config = function(plugin)
        vim.opt.rtp:append(plugin.dir .. "/nvim")
        require("lazy.core.loader").packadd(plugin.dir .. "/nvim")
    end,
    init = function(plugin)
        require("lazy.core.loader").ftdetect(plugin.dir .. "/nvim")
    end
    }

-- plugins/odoo.lua:
return {
    'odoo/odoo-ls',
    dependencies = { 'neovim/nvim-lspconfig' },
    config = function(plugin)
        vim.opt.rtp:append(plugin.dir .. "/nvim")
        require("lazy.core.loader").packadd(plugin.dir .. "/nvim")
    end,
    init = function(plugin)
        require("lazy.core.loader").ftdetect(plugin.dir .. "/nvim")
    end
    }
```

 2. Download the server executable from the release assets
 ```bash
 wget -O ~/.local/share/nvim/odoo/odoo_ls_server https://github.com/odoo/odoo-ls/releases/download/0.4.0/odoo_ls_server
 ```

 3. downloads python [typeshed](https://github.com/python/typeshed) to enrich the server with builtin python package stubs

 (2. and 3. can be done automatically via the `:Odools` [command](https://github.com/odoo/odoo-ls/tree/master/nvim/README.md#usage))


## Configuration
The plugin needs different local path and executable to be working. Here is the mandatory config
keys.

```lua
local odools = require('odools')
local h = os.getenv('HOME')
odools.setup({
    -- mandatory
    odoo_path = h .. "/src/odoo/",
    python_path = h .. "/.pyenv/shims/python3",

    -- optional
    server_path = h .. "/.local/share/nvim/odoo/odoo_ls_server",
    addons = {h .. "/src/enterprise/"},
    additional_stubs = {h .. "/src/additional_stubs/", h .. "/src/typeshed/stubs"},
    root_dir = h .. "/src/", -- working directory, odoo_path if empty
    settings = {
        autoRefresh = true,
        autoRefreshDelay = nil,
        diagMissingImportLevel = "none",
    },
})
```

## Usage
Try the command `:Odools install` to fetch the language server executable from Github as well as the
python typesheds
