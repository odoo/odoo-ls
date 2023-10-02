import { execSync } from "child_process";
try {
    // Keep as a failsafe for open servers
    execSync("for KILLPID in `ps ax | grep 'clean-odoo-lsp' | awk ' { print $1;}'`; do kill -15 $KILLPID; done");
}
catch (err) {
    console.log(err)
}