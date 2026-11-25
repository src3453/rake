import * as vscode from "vscode";
import * as fs from "node:fs";
import * as path from "node:path";
import { LanguageClient } from "vscode-languageclient/node";

interface CompileResponse {
  ok: boolean;
  project?: ScratchProject;
  blockCount?: number;
  error?: string;
}

interface ScratchProject {
  targets: ScratchTarget[];
}

interface ScratchTarget {
  name: string;
  isStage: boolean;
  blocks: Record<string, ScratchBlock>;
}

interface ScratchBlock {
  opcode: string;
  fields?: Record<string, unknown>;
  inputs?: Record<string, unknown>;
  next?: string | null;
  topLevel?: boolean;
  x?: number;
  y?: number;
}

let client: LanguageClient | undefined;
let previewPanel: vscode.WebviewPanel | undefined;
let previewDocument: vscode.TextDocument | undefined;
let previewReady = false;
let previewRequest = 0;

function nonce(): string {
  let value = "";
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  for (let index = 0; index < 32; index += 1) {
    value += alphabet.charAt(Math.floor(Math.random() * alphabet.length));
  }
  return value;
}

function previewHtml(webview: vscode.Webview, extensionUri: vscode.Uri): string {
  const token = nonce();
  const scriptUri = webview.asWebviewUri(vscode.Uri.joinPath(extensionUri, "media", "scratch-preview.js"));
  const styleUri = webview.asWebviewUri(vscode.Uri.joinPath(extensionUri, "media", "scratch-preview.css"));
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource}; script-src 'nonce-${token}';">
  <link rel="stylesheet" href="${styleUri}">
  <title>Scratch Blocks Preview</title>
</head>
<body>
  <header><div class="heading"><span class="scratch-mark">S</span><div><strong>Scratch blocks</strong><small id="document-name">Compiled from Rake</small></div></div><span id="count" class="count">Waiting</span></header>
  <main id="canvas"><div class="empty"><div class="empty-icon">▧</div><strong>Scratch blocks will appear here</strong><span>Open a .rk file and edit it to see the compiled output.</span></div></main>
  <footer><span class="dot" id="dot"></span><span id="status">Waiting for Rake compiler</span><span class="engine">Rake → Scratch 3</span></footer>
  <script nonce="${token}" src="${scriptUri}"></script>
</body>
</html>`;
}

function sendToPreview(message: unknown): void {
  if (previewPanel && previewReady) {
    void previewPanel.webview.postMessage(message);
  }
}

async function compileForPreview(document: vscode.TextDocument): Promise<void> {
  if (!previewPanel || !client || document.languageId !== "rake") return;
  previewDocument = document;
  const request = ++previewRequest;
  sendToPreview({ type: "compiling", documentName: path.basename(document.fileName) });
  try {
    const response = await client.sendRequest<CompileResponse>("rake/compile", {
      source: document.getText()
    });
    if (request !== previewRequest || !previewPanel) return;
    if (response.ok && response.project) {
      sendToPreview({
        type: "compiled",
        documentName: path.basename(document.fileName),
        project: response.project,
        blockCount: response.blockCount ?? 0
      });
    } else {
      sendToPreview({
        type: "compileError",
        documentName: path.basename(document.fileName),
        error: response.error ?? "Compilation failed"
      });
    }
  } catch (error) {
    if (request !== previewRequest) return;
    sendToPreview({
      type: "compileError",
      documentName: path.basename(document.fileName),
      error: error instanceof Error ? error.message : String(error)
    });
  }
}

function openPreview(context: vscode.ExtensionContext): void {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== "rake") {
    void vscode.window.showInformationMessage("Open a .rk file to preview its Scratch blocks.");
    return;
  }

  if (!previewPanel) {
    previewReady = false;
    previewPanel = vscode.window.createWebviewPanel(
      "rakeScratchPreview",
      "Scratch Blocks Preview",
      vscode.ViewColumn.Beside,
      {
        enableScripts: true,
        localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, "media")],
        retainContextWhenHidden: true
      }
    );
    previewPanel.webview.html = previewHtml(previewPanel.webview, context.extensionUri);
    previewPanel.webview.onDidReceiveMessage(message => {
      if (message?.type === "ready") {
        previewReady = true;
        if (previewDocument) void compileForPreview(previewDocument);
      }
    }, undefined, context.subscriptions);
    previewPanel.onDidDispose(() => {
      previewPanel = undefined;
      previewDocument = undefined;
      previewReady = false;
      previewRequest += 1;
    }, undefined, context.subscriptions);
  }
  previewPanel.reveal(vscode.ViewColumn.Beside);
  void compileForPreview(editor.document);
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const configuredCommand = vscode.workspace.getConfiguration("rake").get<string>("serverPath", "");
  const binaryName = process.platform === "win32" ? "rake-lsp.exe" : "rake-lsp";
  const bundledCommand = context.asAbsolutePath(path.join("server", binaryName));
  const command = configuredCommand || (fs.existsSync(bundledCommand) ? bundledCommand : "rake-lsp");
  const serverOptions = { command, args: [] };
  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "rake" }, { scheme: "file", language: "rake-scene" }],
    outputChannelName: "Rake Language Server"
  };

  client = new LanguageClient("rake", "Rake Language Server", serverOptions, clientOptions);
  context.subscriptions.push(client);
  await client.start();

  context.subscriptions.push(vscode.commands.registerCommand("rake.openScratchPreview", () => openPreview(context)));
  context.subscriptions.push(vscode.workspace.onDidChangeTextDocument(event => {
    if (previewDocument?.uri.toString() === event.document.uri.toString()) {
      void compileForPreview(event.document);
    }
  }));
  context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor(editor => {
    if (previewPanel && editor?.document.languageId === "rake") {
      void compileForPreview(editor.document);
    }
  }));
}

export async function deactivate(): Promise<void> {
  previewPanel?.dispose();
  await client?.stop();
}
