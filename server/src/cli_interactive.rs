#![cfg(feature = "python-cli")]
//! `--interactive` CLI mode: build the knowledge database once (like `--parse`, but kept alive),
//! then either drop into a Python console or run a `--script` file against it. Both paths share
//! the same native `odoo_ls` module (see `python_api.rs`).

use std::io::Write;

use rustpython_vm::builtins::PyBaseExceptionRef;
use rustpython_vm::{AsObject, Interpreter, Settings, VirtualMachine, compiler::Mode, scope::Scope};
use tracing::error;

use crate::args::Cli;
use crate::cli_backend::build_and_init_odoo;
use crate::python_api;

pub struct InteractiveBackend {
    cli: Cli,
}

impl InteractiveBackend {
    pub fn new(cli: Cli) -> Self {
        InteractiveBackend { cli }
    }

    pub fn run(self) {
        let Some((server, s, r)) = build_and_init_odoo(&self.cli) else {
            return;
        };
        python_api::install(server, s, r);
        python_api::drain_pending_messages();

        let builder = Interpreter::builder(Settings::default());
        let ctx = builder.ctx.clone();
        let mut module_defs = rustpython_stdlib::stdlib_module_defs(&ctx);
        module_defs.push(python_api::odoo_ls::module_def(&ctx));
        let interp = builder
            .add_native_modules(&module_defs)
            .add_frozen_modules(rustpython_pylib::FROZEN_STDLIB)
            .build();

        let script = self.cli.script.clone();
        let script_output = self.cli.script_output.clone();
        let leaked_exc = interp.enter(|vm| match &script {
            Some(script_path) => run_script(vm, script_path, script_output.as_deref()),
            None => run_repl(vm),
        });
        let exit_code = interp.finalize(leaked_exc);
        if exit_code != 0 {
            std::process::exit(exit_code as i32);
        }
    }
}

/// Runs `source` as a whole module (no auto-printing of the last expression, no incomplete-input
/// handling) — used for `--script` files and the tiny stdout-redirection bootstrap.
fn run_exec(vm: &VirtualMachine, source: &str, path: &str, scope: Scope) -> Option<PyBaseExceptionRef> {
    let code = match vm.compile(source, Mode::Exec, path.to_owned()) {
        Ok(code) => code,
        Err(err) => {
            let exc = vm.new_syntax_error(&err, Some(source));
            vm.print_exception(exc);
            return None;
        }
    };
    match vm.run_code_obj(code, scope) {
        Ok(_) => None,
        Err(exc) if exc.fast_isinstance(vm.ctx.exceptions.system_exit) => Some(exc),
        Err(exc) => {
            vm.print_exception(exc);
            None
        }
    }
}

fn run_script(vm: &VirtualMachine, script_path: &str, output_path: Option<&str>) -> Option<PyBaseExceptionRef> {
    let source = match std::fs::read_to_string(script_path) {
        Ok(source) => source,
        Err(e) => {
            error!("Unable to read script {}: {}", script_path, e);
            return None;
        }
    };
    let scope = vm.new_scope_with_builtins();

    if let Some(output_path) = output_path {
        let redirect_src = format!("import sys\nsys.stdout = open({output_path:?}, 'w')\n");
        if let Some(exc) = run_exec(vm, &redirect_src, "<redirect>", scope.clone()) {
            return Some(exc);
        }
    }

    run_exec(vm, &source, script_path, scope)
}

/// A minimal interactive console: accumulates lines into a block and only compiles/runs once
/// either (a) a single, non-blank line was entered with nothing already accumulated (the common
/// one-liner case), or (b) a blank line is entered while a multi-line block is being accumulated
/// (mirroring a real Python console, which waits for a blank line to close e.g. a `def`/`if`
/// body rather than running as soon as the block happens to already be syntactically complete).
fn run_repl(vm: &VirtualMachine) -> Option<PyBaseExceptionRef> {
    let scope = vm.new_scope_with_builtins();
    let stdin = std::io::stdin();
    let mut buffer = String::new();

    loop {
        print!("{}", if buffer.is_empty() { ">>> " } else { "... " });
        let _ = std::io::stdout().flush();

        let mut line = String::new();
        if stdin.read_line(&mut line).unwrap_or(0) == 0 {
            println!();
            return None; // EOF (Ctrl-D): end the session like a real console.
        }
        let line = line.trim_end_matches(['\n', '\r']);
        let blank = line.trim().is_empty();

        if buffer.is_empty() && blank {
            continue;
        }
        if !blank {
            buffer.push_str(line);
            buffer.push('\n');
        }

        match vm.compile(&buffer, Mode::Single, "<stdin>".to_owned()) {
            Ok(code) => {
                let is_single_line = !buffer.trim_end_matches('\n').contains('\n');
                if !blank && !is_single_line {
                    // Compiles, but more lines might still be meant to extend this block
                    // (e.g. a second statement in a function body) — wait for a blank line.
                    continue;
                }
                buffer.clear();
                match vm.run_code_obj(code, scope.clone()) {
                    Ok(_) => {}
                    Err(exc) if exc.fast_isinstance(vm.ctx.exceptions.system_exit) => return Some(exc),
                    Err(exc) => vm.print_exception(exc),
                }
            }
            Err(err) => {
                if blank {
                    // The user asked to run what's there and it's still not valid: report it.
                    let exc = vm.new_syntax_error(&err, Some(&buffer));
                    vm.print_exception(exc);
                    buffer.clear();
                }
                // Otherwise assume the input is just incomplete so far and keep accumulating.
            }
        }
    }
}
