import * as vscode from "vscode";
import * as lc from "vscode-languageclient/node";
import * as ra from "./lsp_ext";

import { Config, prepareVSCodeConfig } from "./config";
import { createClient } from "./client";
import {
  findRustToolchainFiles,
  isCargoTomlEditor,
  isDocumentInWorkspace,
  isRustDocument,
  isRustEditor,
  LazyOutputChannel,
  log,
  type RustEditor,
} from "./util";
import type { ServerStatusParams } from "./lsp_ext";
// import {
//     type Dependency,
//     type DependencyFile,
//     RustDependenciesProvider,
//     type DependencyId,
// } from "./dependencies_provider";
// import { SyntaxTreeProvider, type SyntaxElement } from "./syntax_tree_provider";
// import { execRevealDependency } from "./commands";
import { PersistentState } from "./persistent_state";
import { bootstrap } from "./bootstrap";
// import { prepareTestExplorer } from "./test_explorer";
import { spawn } from "node:child_process";
import { text } from "node:stream/consumers";
import type { RustAnalyzerExtensionApi } from "./main";

// We only support local folders, not eg. Live Share (`vlsl:` scheme), so don't activate if
// only those are in use. We use "Empty" to represent these scenarios
// (r-a still somewhat works with Live Share, because commands are tunneled to the host)

export type Workspace =
  | { kind: "Empty" }
  | { kind: "Workspace Folder" }
  | { kind: "Detached Files"; files: vscode.TextDocument[] };

export function fetchWorkspace(): Workspace {
  const folders = (vscode.workspace.workspaceFolders || []).filter(
    (folder) => folder.uri.scheme === "file",
  );
  const rustDocuments = vscode.workspace.textDocuments.filter((document) =>
    isRustDocument(document),
  );

  return folders.length === 0
    ? rustDocuments.length === 0
      ? { kind: "Empty" }
      : { kind: "Detached Files", files: rustDocuments }
    : { kind: "Workspace Folder" };
}

export type CommandFactory = {
  enabled: (ctx: CtxInit) => Cmd;
  disabled?: (ctx: Ctx) => Cmd;
};

export type CtxInit = Ctx & {
  readonly client: lc.LanguageClient;
};

export class Ctx implements RustAnalyzerExtensionApi {
  readonly statusBar: vscode.StatusBarItem;
  readonly config: Config;
  readonly workspace: Workspace;
  readonly version: string;

  private _client: lc.LanguageClient | undefined;
  private _serverPath: string | undefined;
  private traceOutputChannel: vscode.OutputChannel | undefined;
  private testController: vscode.TestController | undefined;
  private outputChannel: vscode.OutputChannel | undefined;
  private clientSubscriptions: Disposable[];
  private state: PersistentState;
  private commandFactories: Record<string, CommandFactory>;
  private commandDisposables: Disposable[];
  private unlinkedFiles: vscode.Uri[];

  private lastStatus: ServerStatusParams | { health: "stopped" } = {
    health: "stopped",
  };
  private _serverVersion: string;
  private statusBarActiveEditorListener: Disposable;

  get serverPath(): string | undefined {
    return this._serverPath;
  }

  get serverVersion(): string | undefined {
    return this._serverVersion;
  }

  get client() {
    return this._client;
  }

  constructor(
    readonly extCtx: vscode.ExtensionContext,
    commandFactories: Record<string, CommandFactory>,
    workspace: Workspace,
  ) {
    extCtx.subscriptions.push(this);
    this.version = extCtx.extension.packageJSON.version ?? "<unknown>";
    this._serverVersion = "<not running>";
    this.config = new Config(extCtx);
    this.statusBar = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Left,
    );
    this.updateStatusBarVisibility(vscode.window.activeTextEditor);
    this.statusBarActiveEditorListener =
      vscode.window.onDidChangeActiveTextEditor((editor) =>
        this.updateStatusBarVisibility(editor),
      );
    this.workspace = workspace;
    this.clientSubscriptions = [];
    this.commandDisposables = [];
    this.commandFactories = commandFactories;
    this.unlinkedFiles = [];
    this.state = new PersistentState(extCtx.globalState);

    this.updateCommands("disable");
    this.setServerStatus({
      health: "stopped",
    });
  }

  async addConfiguration(
    extensionId: string,
    configuration: Record<string, unknown>,
  ): Promise<void> {
    await this.config.addExtensionConfiguration(extensionId, configuration);
  }

  dispose() {
    this.config.dispose();
    this.statusBar.dispose();
    this.statusBarActiveEditorListener.dispose();
    this.testController?.dispose();
    void this.disposeClient();
    this.commandDisposables.forEach((disposable) => disposable.dispose());
  }

  async onWorkspaceFolderChanges() {
    const workspace = fetchWorkspace();
    if (
      workspace.kind === "Detached Files" &&
      this.workspace.kind === "Detached Files"
    ) {
      if (workspace.files !== this.workspace.files) {
        if (this.client?.isRunning()) {
          // Ideally we wouldn't need to tear down the server here, but currently detached files
          // are only specified at server start
          await this.stopAndDispose();
          await this.start();
        }
        return;
      }
    }
    if (
      workspace.kind === "Workspace Folder" &&
      this.workspace.kind === "Workspace Folder"
    ) {
      return;
    }
    if (workspace.kind === "Empty") {
      await this.stopAndDispose();
      return;
    }
    if (this.client?.isRunning()) {
      await this.restart();
    }
  }

  private async getOrCreateClient() {
    if (this.workspace.kind === "Empty") {
      return;
    }

    if (!this.traceOutputChannel) {
      this.traceOutputChannel = new LazyOutputChannel("ritobin-lsp LSP Trace");
      this.pushExtCleanup(this.traceOutputChannel);
    }
    if (!this.outputChannel) {
      this.outputChannel = vscode.window.createOutputChannel(
        "ritobin-lsp Language Server",
      );
      this.pushExtCleanup(this.outputChannel);
    }

    if (!this._client) {
      this._serverPath = await this.bootstrap();
      text(
        spawn(this._serverPath, ["--version"]).stdout.setEncoding("utf-8"),
      ).then(
        (data) => {
          const prefix = `ritobin-lsp `;
          this._serverVersion = data
            .slice(data.startsWith(prefix) ? prefix.length : 0)
            .trim();
          this.refreshServerStatus();
        },
        (_) => {
          this._serverVersion = "<unknown>";
          this.refreshServerStatus();
        },
      );
      const newEnv = { ...process.env };
      for (const [k, v] of Object.entries(this.config.serverExtraEnv)) {
        if (v) {
          newEnv[k] = v;
        } else if (k in newEnv) {
          delete newEnv[k];
        }
      }
      const run: lc.Executable = {
        command: this._serverPath,
        options: { env: newEnv },
      };
      const serverOptions = {
        run,
        debug: run,
      };

      let rawInitializationOptions = this.config.cfg;

      if (this.workspace.kind === "Detached Files") {
        rawInitializationOptions = {
          detachedFiles: this.workspace.files.map((file) => file.uri.fsPath),
          ...rawInitializationOptions,
        };
      }

      const initializationOptions = prepareVSCodeConfig(
        rawInitializationOptions,
      );

      this._client = await createClient(
        this.traceOutputChannel,
        this.outputChannel,
        initializationOptions,
        serverOptions,
        this.config,
        this.unlinkedFiles,
      );
      this.pushClientCleanup(
        this._client.onNotification(ra.serverStatus, (params) =>
          this.setServerStatus(params),
        ),
      );
      // this.pushClientCleanup(
      //     this._client.onNotification(ra.openServerLogs, () => {
      //         this.outputChannel!.show();
      //     }),
      // );
    }
    return this._client;
  }

  private async bootstrap(): Promise<string> {
    return bootstrap(this.extCtx, this.config, this.state).catch((err) => {
      let message = "bootstrap error. ";

      message +=
        'See the logs in "OUTPUT > Ritobin Client" (should open automatically).';
      message +=
        'To enable verbose logs, click the gear icon in the "OUTPUT" tab and select "Debug".';

      log.error("Bootstrap error", err);
      throw new Error(message);
    });
  }

  async start() {
    log.info("Starting language client");
    const client = await this.getOrCreateClient();
    if (!client) {
      return;
    }
    await client.start();
    this.updateCommands();
  }

  async restart() {
    // FIXME: We should re-use the client, that is ctx.deactivate() if none of the configs have changed
    await this.stopAndDispose();
    await this.start();
  }

  async stop() {
    if (!this._client) {
      return;
    }
    log.info("Stopping language client");
    this.updateCommands("disable");
    await this._client.stop();
  }

  async stopAndDispose() {
    if (!this._client) {
      return;
    }
    log.info("Disposing language client");
    this.updateCommands("disable");
    // we give the server 100ms to stop gracefully
    await this.client?.stop(100).catch((_) => {});
    await this.disposeClient();
  }

  private async disposeClient() {
    this.clientSubscriptions?.forEach((disposable) => disposable.dispose());
    this.clientSubscriptions = [];
    await this._client?.dispose();
    this._serverPath = undefined;
    this._client = undefined;
  }

  get activeRustEditor(): RustEditor | undefined {
    const editor = vscode.window.activeTextEditor;
    return editor && isRustEditor(editor) ? editor : undefined;
  }

  get activeCargoTomlEditor(): RustEditor | undefined {
    const editor = vscode.window.activeTextEditor;
    return editor && isCargoTomlEditor(editor) ? editor : undefined;
  }

  get extensionPath(): string {
    return this.extCtx.extensionPath;
  }

  get subscriptions(): Disposable[] {
    return this.extCtx.subscriptions;
  }

  private updateCommands(forceDisable?: "disable") {
    this.commandDisposables.forEach((disposable) => disposable.dispose());
    this.commandDisposables = [];

    const clientRunning = (!forceDisable && this._client?.isRunning()) ?? false;
    const isClientRunning = function (_ctx: Ctx): _ctx is CtxInit {
      return clientRunning;
    };

    for (const [name, factory] of Object.entries(this.commandFactories)) {
      const fullName = `ritobin-lsp.${name}`;
      let callback;
      if (isClientRunning(this)) {
        // we asserted that `client` is defined
        callback = factory.enabled(this);
      } else if (factory.disabled) {
        callback = factory.disabled(this);
      } else {
        callback = () =>
          vscode.window.showErrorMessage(
            `command ${fullName} failed: ritobin-lsp server is not running`,
          );
      }

      this.commandDisposables.push(
        vscode.commands.registerCommand(fullName, callback),
      );
    }
  }

  setServerStatus(status: ServerStatusParams | { health: "stopped" }) {
    this.lastStatus = status;
    this.updateStatusBarItem();
  }

  refreshServerStatus() {
    this.updateStatusBarItem();
  }

  private updateStatusBarItem() {
    let icon = "";
    const status = this.lastStatus;
    const statusBar = this.statusBar;
    statusBar.tooltip = new vscode.MarkdownString("", true);
    statusBar.tooltip.isTrusted = true;
    switch (status.health) {
      case "ok":
        statusBar.color = undefined;
        statusBar.backgroundColor = undefined;
        if (this.config.statusBarClickAction === "stopServer") {
          statusBar.command = "ritobin-lsp.stopServer";
        } else {
          statusBar.command = "ritobin-lsp.openLogs";
        }
        break;
      case "warning":
        statusBar.color = new vscode.ThemeColor(
          "statusBarItem.warningForeground",
        );
        statusBar.backgroundColor = new vscode.ThemeColor(
          "statusBarItem.warningBackground",
        );
        statusBar.command = "ritobin-lsp.openLogs";
        icon = "$(warning) ";
        break;
      case "error":
        statusBar.color = new vscode.ThemeColor(
          "statusBarItem.errorForeground",
        );
        statusBar.backgroundColor = new vscode.ThemeColor(
          "statusBarItem.errorBackground",
        );
        statusBar.command = "ritobin-lsp.openLogs";
        icon = "$(error) ";
        break;
      case "stopped":
        statusBar.tooltip.appendText("Server is stopped");
        statusBar.tooltip.appendMarkdown(
          "\n\n[Start server](command:ritobin-lsp.startServer)",
        );
        statusBar.color = new vscode.ThemeColor(
          "statusBarItem.warningForeground",
        );
        statusBar.backgroundColor = new vscode.ThemeColor(
          "statusBarItem.warningBackground",
        );
        statusBar.command = "ritobin-lsp.startServer";
        statusBar.text = "$(stop-circle) ritobin-lsp";
        return;
    }
    if (status.message) {
      statusBar.tooltip.appendMarkdown(status.message);
    }
    if (statusBar.tooltip.value) {
      statusBar.tooltip.appendMarkdown("\n\n---\n\n");
    }

    const toggleCheckOnSave = this.config.checkOnSave ? "Disable" : "Enable";
    statusBar.tooltip.appendMarkdown(
      `[Extension Info](command:ritobin-lsp.serverVersion "Show version and server binary info"): Version ${this.version}, Server Version ${this._serverVersion}\n\n` +
        `---\n\n` +
        `[$(terminal) Open Logs](command:ritobin-lsp.openLogs "Open the server logs")\n\n` +
        `[$(settings) ${toggleCheckOnSave} Check on Save](command:ritobin-lsp.toggleCheckOnSave "Temporarily ${toggleCheckOnSave.toLowerCase()} check on save functionality")\n\n` +
        `[$(refresh) Reload Workspace](command:ritobin-lsp.reloadWorkspace "Reload and rediscover workspaces")\n\n` +
        `[$(symbol-property) Rebuild Build Dependencies](command:ritobin-lsp.rebuildProcMacros "Rebuild build scripts and proc-macros")\n\n` +
        `[$(stop-circle) Stop server](command:ritobin-lsp.stopServer "Stop the server")\n\n` +
        `[$(debug-restart) Restart server](command:ritobin-lsp.restartServer "Restart the server")`,
    );
    if (!status.quiescent) icon = "$(loading~spin) ";
    statusBar.text = `${icon}ritobin-lsp`;
  }

  private updateStatusBarVisibility(editor: vscode.TextEditor | undefined) {
    const showStatusBar = this.config.statusBarShowStatusBar;
    if (showStatusBar == null || showStatusBar === "never") {
      this.statusBar.hide();
    } else if (showStatusBar === "always") {
      this.statusBar.show();
    } else {
      const documentSelector = showStatusBar.documentSelector;
      if (
        editor != null &&
        vscode.languages.match(documentSelector, editor.document) > 0
      ) {
        this.statusBar.show();
      } else {
        this.statusBar.hide();
      }
    }
  }

  pushExtCleanup(d: Disposable) {
    this.extCtx.subscriptions.push(d);
  }

  pushClientCleanup(d: Disposable) {
    this.clientSubscriptions.push(d);
  }
}

export interface Disposable {
  dispose(): void;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type Cmd = (...args: any[]) => unknown;
