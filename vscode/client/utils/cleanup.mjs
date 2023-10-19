import { execSync } from "child_process";
import * as os from 'os';

try {
    // Keep as a failsafe for open servers
    switch (os.type()) {
        case 'Windows_NT':
            execSync('taskkill /F /FI "IMAGENAME eq *clean-odoo-lsp*"')
            break;
        case 'Darwin':
        case 'Linux':
            execSync("for KILLPID in `ps ax | grep 'clean-odoo-lsp' | awk ' { print $1;}'`; do kill -15 $KILLPID; done");
            break;

    }
}
catch (err) {
    console.log(err)
}