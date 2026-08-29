import * as vscode from "vscode";
import * as lc from "vscode-languageclient/node";

import { BIN_EDITOR_VIEW_TYPE, RitobinBinEditorProvider } from "./bin_editor";
import {
  BIN_SCHEME,
  RitobinBinDecorationProvider,
  RitobinBinFs,
} from "./bin_fs";
import { registerBinReadonlyFlow } from "./bin_readonly";
import * as commands from "./commands";
import { type CommandFactory, Ctx, fetchWorkspace } from "./ctx";
import * as diagnostics from "./diagnostics";
import {
  installExplorerIntegration,
  maybePromptExplorerIntegration,
  uninstallExplorerIntegration,
} from "./explorer_integration";
import { guard } from "./ide_utils";
// import { activateTaskProvider } from "./tasks";
import { isRitobinDocument, log, setContextValue } from "./util";
// import { initializeDebugSessionTrackingAndRebuild } from "./debug";

const RITOBIN_PROJECT_CONTEXT_NAME = "inRitobinProject";

export interface RustAnalyzerExtensionApi {
  // FIXME: this should be non-optional
  readonly client?: lc.LanguageClient;

  // Allows adding a configuration override from another extension.
  // `extensionId` is used to only merge configuration override from present
  // extensions. `configuration` is map of ritobin-lsp-specific setting
  // overrides, e.g., `{"cargo.cfgs": ["foo", "bar"]}`.
  addConfiguration(
    extensionId: string,
    configuration: Record<string, unknown>,
  ): Promise<void>;
}

export async function deactivate() {
  await setContextValue(RITOBIN_PROJECT_CONTEXT_NAME, undefined);
}

export async function activate(
  context: vscode.ExtensionContext,
): Promise<RustAnalyzerExtensionApi> {
  const ctx = new Ctx(context, createCommands(), fetchWorkspace());
  log.info("Activating...");

  // VS Code doesn't show a notification when an extension fails to activate
  // so we do it ourselves.
  const api = await activateServer(ctx).catch((err) => {
    void vscode.window.showErrorMessage(
      `Cannot activate ritobin-lsp extension: ${err.message}`,
    );
    throw err;
  });
  await setContextValue(RITOBIN_PROJECT_CONTEXT_NAME, true);
  return api;
}

async function activateServer(ctx: Ctx): Promise<RustAnalyzerExtensionApi> {
  if (ctx.workspace.kind === "Workspace Folder") {
    //     ctx.pushExtCleanup(activateTaskProvider(ctx.config));
  }

  // Register the ritobin-bin virtual filesystem and the *.bin custom editor
  // redirect before anything can open a virtual document (incl. hot-exit
  // restore of dirty virtual docs).
  const binFs = new RitobinBinFs(ctx);
  ctx.pushExtCleanup(
    vscode.workspace.registerFileSystemProvider(BIN_SCHEME, binFs, {
      isCaseSensitive: false,
    }),
  );
  vscode.workspace.onDidCloseTextDocument(
    (doc) => binFs.handleDidCloseTextDocument(doc),
    null,
    ctx.subscriptions,
  );
  ctx.pushExtCleanup(
    vscode.window.registerCustomEditorProvider(
      BIN_EDITOR_VIEW_TYPE,
      new RitobinBinEditorProvider(),
      { supportsMultipleEditorsPerDocument: true },
    ),
  );
  ctx.pushExtCleanup(
    vscode.window.registerFileDecorationProvider(
      new RitobinBinDecorationProvider(),
    ),
  );
  const binStatus = vscode.languages.createLanguageStatusItem(
    "ritobin-lsp.binEditorStatus",
    { language: "ritobin", scheme: BIN_SCHEME },
  );
  binStatus.name = "Ritobin Bin Editor";
  binStatus.text = "$(file-binary) .bin (read-only)";
  binStatus.detail =
    "Deserialized League .bin in read-only mode; enable write-back to save changes";
  binStatus.command = {
    title: "Enable write-back",
    command: "ritobin-lsp.enableBinWriteBack",
  };
  ctx.pushExtCleanup(binStatus);

  registerBinReadonlyFlow(ctx, binFs);

  const diagnosticProvider = new diagnostics.TextDocumentProvider(ctx);
  ctx.pushExtCleanup(
    vscode.workspace.registerTextDocumentContentProvider(
      diagnostics.URI_SCHEME,
      diagnosticProvider,
    ),
  );

  const decorationProvider = new diagnostics.AnsiDecorationProvider(ctx);
  ctx.pushExtCleanup(decorationProvider);

  async function decorateVisibleEditors(document: vscode.TextDocument) {
    for (const editor of vscode.window.visibleTextEditors) {
      if (document === editor.document) {
        await decorationProvider.provideDecorations(editor);
      }
    }
  }

  vscode.workspace.onDidChangeTextDocument(
    async (event) => await decorateVisibleEditors(event.document),
    null,
    ctx.subscriptions,
  );
  vscode.workspace.onDidOpenTextDocument(
    decorateVisibleEditors,
    null,
    ctx.subscriptions,
  );
  vscode.window.onDidChangeActiveTextEditor(
    async (editor) => {
      if (editor) {
        diagnosticProvider.triggerUpdate(editor.document.uri);
        await decorateVisibleEditors(editor.document);
      }
    },
    null,
    ctx.subscriptions,
  );
  vscode.window.onDidChangeVisibleTextEditors(
    async (visibleEditors) => {
      for (const editor of visibleEditors) {
        diagnosticProvider.triggerUpdate(editor.document.uri);
        await decorationProvider.provideDecorations(editor);
      }
    },
    null,
    ctx.subscriptions,
  );

  vscode.workspace.onDidChangeWorkspaceFolders(
    async (_) => ctx.onWorkspaceFolderChanges(),
    null,
    ctx.subscriptions,
  );
  vscode.workspace.onDidChangeConfiguration(
    async (_) => {
      await ctx.client?.sendNotification(
        lc.DidChangeConfigurationNotification.type,
        {
          settings: "",
        },
      );
    },
    null,
    ctx.subscriptions,
  );

  if (ctx.config.debug.buildBeforeRestart) {
    // initializeDebugSessionTrackingAndRebuild(ctx);
  }

  if (ctx.config.initializeStopped) {
    ctx.setServerStatus({
      health: "stopped",
    });
  } else if (vscode.workspace.textDocuments.some(isRitobinDocument)) {
    await ctx.start();
  } else {
    const lazyStart = vscode.workspace.onDidOpenTextDocument(async (doc) => {
      if (isRitobinDocument(doc)) {
        lazyStart.dispose();
        await ctx.start();
      }
    });

    ctx.pushExtCleanup(lazyStart);
  }

  void maybePromptExplorerIntegration(ctx);

  return ctx;
}

const installExplorerIntegrationCommand = (ctx: Ctx) =>
  guard("Explorer integration install", async () => {
    if (await installExplorerIntegration()) {
      await ctx.persistentState.updateExplorerIntegrationPrompt("installed");
    }
  });

const uninstallExplorerIntegrationCommand = (ctx: Ctx) =>
  guard("Explorer integration uninstall", async () => {
    if (await uninstallExplorerIntegration()) {
      await ctx.persistentState.updateExplorerIntegrationPrompt("dismissed");
    }
  });

function createCommands(): Record<string, CommandFactory> {
  return {
    onEnter: {
      enabled: commands.onEnter,
      disabled: (_) => () =>
        vscode.commands.executeCommand("default:type", { text: "\n" }),
    },
    restartServer: {
      enabled: (ctx) => async () => {
        await ctx.restart();
      },
      disabled: (ctx) => async () => {
        await ctx.start();
      },
    },
    startServer: {
      enabled: (ctx) => async () => {
        await ctx.start();
      },
      disabled: (ctx) => async () => {
        await ctx.start();
      },
    },
    stopServer: {
      enabled: (ctx) => async () => {
        // FIXME: We should re-use the client, that is ctx.deactivate() if none of the configs have changed
        await ctx.stopAndDispose();
        ctx.setServerStatus({
          health: "stopped",
        });
      },
      disabled: (_) => async () => {},
    },
    lspStatus: { enabled: commands.lspStatus },
    unhash: { enabled: commands.unhash },
    installExplorerIntegration: {
      enabled: installExplorerIntegrationCommand,
      disabled: installExplorerIntegrationCommand,
    },
    uninstallExplorerIntegration: {
      enabled: uninstallExplorerIntegrationCommand,
      disabled: uninstallExplorerIntegrationCommand,
    },
    matchingBrace: {
      enabled: (_) => async () => {},
      disabled: (_) => async () => {},
    },
    moveItemUp: {
      enabled: (_) => async () => {},
      disabled: (_) => async () => {},
    },
    moveItemDown: {
      enabled: (_) => async () => {},
      disabled: (_) => async () => {},
    },
    openLogs: {
      enabled: (ctx) => async () => ctx.showOutputChannel(),
      disabled: (ctx) => async () => ctx.showOutputChannel(),
    },
    toggleLSPLogs: {
      enabled: (_) => async () => {},
      disabled: (_) => async () => {},
    },
  };
}
