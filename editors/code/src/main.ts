import * as vscode from "vscode";
import * as lc from "vscode-languageclient/node";

import * as commands from "./commands";
import { type CommandFactory, Ctx, fetchWorkspace } from "./ctx";
import * as diagnostics from "./diagnostics";
// import { activateTaskProvider } from "./tasks";
import { log, setContextValue } from "./util";
// import { initializeDebugSessionTrackingAndRebuild } from "./debug";

const RUST_PROJECT_CONTEXT_NAME = "inRitobinProject";

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
  await setContextValue(RUST_PROJECT_CONTEXT_NAME, undefined);
}

export async function activate(
  context: vscode.ExtensionContext,
): Promise<RustAnalyzerExtensionApi> {
  const ctx = new Ctx(context, createCommands(), fetchWorkspace());

  log.info("ASDASDSD");

  // VS Code doesn't show a notification when an extension fails to activate
  // so we do it ourselves.
  const api = await activateServer(ctx).catch((err) => {
    void vscode.window.showErrorMessage(
      `Cannot activate ritobin-lsp extension: ${err.message}`,
    );
    throw err;
  });
  await setContextValue(RUST_PROJECT_CONTEXT_NAME, true);
  return api;
}

async function activateServer(ctx: Ctx): Promise<RustAnalyzerExtensionApi> {
  if (ctx.workspace.kind === "Workspace Folder") {
    //     ctx.pushExtCleanup(activateTaskProvider(ctx.config));
  }

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
  } else {
    await ctx.start();
  }

  return ctx;
}

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
    lspStatus: {
      enabled: (_) => async () => {},
      disabled: (_) => async () => {},
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
    toggleLSPLogs: {
      enabled: (_) => async () => {},
      disabled: (_) => async () => {},
    },
  };
}

