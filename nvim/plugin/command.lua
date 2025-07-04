local command = {}
command.target = "0.4.0"
command.bin_path = ""

local download = function(url, output_path, asset_type)
    local stdout = vim.uv.new_pipe(false)
    local stderr = vim.uv.new_pipe(false)

    local handle
    local args, cmd
    if asset_type == "file" then
        if vim.fn.filereadable(output_path) == 1 then
            vim.api.nvim_echo({{"Delete previous file"}}, true, {})
            vim.uv.fs_unlink(output_path)
        end
        cmd = "wget"
        args = { "-q", "-O", output_path, url }
    else
        cmd = "git"
        args = { "clone", "-q", url, output_path}
    end
    vim.api.nvim_echo({{"Starting download from: " .. url}}, true, {})
    vim.api.nvim_echo({{"Saving to: " .. output_path}}, true, {})

    handle = vim.uv.spawn(cmd, {
        args = args,
        stdio = { nil, stdout, stderr },
    }, function(code, signal)
        stdout:close()
        stderr:close()
        handle:close()

        local msg = {"\nDownload successful!"}
        if code ~= 0 then
            msg = {"\nDownload failed with exit code " .. code}
        else
            print(vim.uv.fs_chmod(output_path, 493))
        end
        vim.schedule(function()
            vim.api.nvim_echo({msg}, true, {})
        end)
    end)

    stdout:read_start(function(err, data)
        assert(not err, err)
        if data then
            io.write(data)
            io.flush()
        end
    end)

    stderr:read_start(function(err, data)
        assert(not err, err)
        if data then
            io.write("\nError: " .. data)
            io.flush()
        end
    end)
end

local download_requirements = function()
    local bin_dir_path = vim.fn.stdpath('data') .. '/odoo'
    local bin_path = bin_dir_path .. '/odoo_ls_server'
    if vim.fn.isdirectory(bin_dir_path) == 0 then
        os.execute('mkdir -p ' .. bin_dir_path)
    end
    download("https://github.com/odoo/odoo-ls/releases/download/" .. command.target .. "/odoo_ls_server", bin_path, 'file')
    if vim.fn.executable('git') == 1 then
        local path = bin_dir_path .. '/typeshed'
        if vim.fn.isdirectory(path) == 0 then
            download('https://github.com/python/typeshed.git', path, 'repo')
        else
            vim.api.nvim_echo({{"typeshed already downloaded"}}, true, {})
        end
    else
        vim.api.nvim_err_writeln("git needed to download python typeshed")
    end
    vim.cmd.LspRestart('odools')
end

local odoo_command = function(opts)
    if opts.fargs and opts.fargs[1] == "install" then
        download_requirements()
    end
end

vim.api.nvim_create_user_command('Odools', odoo_command, {
    nargs = 1,
    complete = function()
        -- return completion candidates as a list-like table
        return { "install" }
    end,
})

return command
