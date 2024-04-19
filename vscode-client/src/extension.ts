import { ExtensionContext, window, commands } from "vscode";
import { Executable, LanguageClient, LanguageClientOptions, ServerOptions } from "vscode-languageclient/node";

let client: LanguageClient;

export async function activate(context: ExtensionContext) {
  const disposable = commands.registerCommand("fluxion.helloWorld", () => {
    window.showInformationMessage("Hello World!");
  });
  context.subscriptions.push(disposable);

  const restartServerCommand = commands.registerCommand("fluxion.restartServer", () => {
    if (client) {
      client.stop().then(() => {
        client.start();
        window.showInformationMessage("Fluxion server restarted.");
      });
    }
  });
  context.subscriptions.push(restartServerCommand);

  const command = process.env.SERVER_PATH || "fluxion-lsp";
  const run: Executable = {
    command,
    options: {
      env: {
        ...process.env,
        RUST_LOG: "debug",
      },
    },
  };
  const serverOptions: ServerOptions = {
    run,
    debug: run,
  };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "python" }],
  };

  client = new LanguageClient("fluxion-lsp", "Fluxion", serverOptions, clientOptions);
  console.log("server started 2");

  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
