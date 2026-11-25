import * as vscode from "vscode";
import * as fs from "node:fs";
import * as path from "node:path";
import { LanguageClient } from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const configuredCommand = vscode.workspace.getConfiguration("rake").get<string>("serverPath", "");
  const binaryName = process.platform === "win32" ? "rake-lsp.exe" : "rake-lsp";
  const bundledCommand = context.asAbsolutePath(path.join("server", binaryName));
  const command = configuredCommand || (fs.existsSync(bundledCommand) ? bundledCommand : "rake-lsp");
  const serverOptions = { command, args: [] };
  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "rake" }],
    outputChannelName: "Rake Language Server"
  };

  client = new LanguageClient("rake", "Rake Language Server", serverOptions, clientOptions);
  context.subscriptions.push(client);
  await client.start();
}

export async function deactivate(): Promise<void> {
  await client?.stop();
}
