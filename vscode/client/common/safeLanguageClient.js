const { LanguageClient, State } = require("vscode-languageclient/node");

/**
 * A safe extension of LanguageClient that prevents errors when stopping.
 */
class SafeLanguageClient extends LanguageClient {
    async sendNotification(method, params) {
        try {
            await super.sendNotification(method, params);
        } catch (error) {
            if (this.state == State.Stopped) {
                this.info(`Notification ignored: (${method.method}) due to error (${error}) because Client is stopped`);
                return;
            }
            throw error;
        }
    }
}

module.exports.SafeLanguageClient = SafeLanguageClient;
