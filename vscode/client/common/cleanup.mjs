import { execSync } from "child_process";
import * as os from 'os';

try {
    // Keep as a failsafe for open servers
    switch (os.type()) {
        case 'Windows_NT':
            execSync('taskkill /F /IM odoo_ls_server.exe')
            break;
        case 'Darwin':
        case 'Linux':
            execSync("for KILLPID in `ps ax | grep 'odoo_ls_server' | awk ' { print $1;}'`; do kill -15 $KILLPID; done");
            break;

    }
}
catch (err) {
    console.log(err)
}