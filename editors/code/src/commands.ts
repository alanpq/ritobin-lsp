import * as vscode from "vscode";
import * as lc from "vscode-languageclient";
import * as ra from "./lsp_ext";
import * as path from "path";

import type { Cmd, CtxInit } from "./ctx";
import { log } from "./util";

export function lspStatus(ctx: CtxInit): Cmd {
  const tdcp = new (class implements vscode.TextDocumentContentProvider {
    readonly uri = vscode.Uri.parse("ritobin-status://status");
    readonly eventEmitter = new vscode.EventEmitter<vscode.Uri>();

    async provideTextDocumentContent(_uri: vscode.Uri): Promise<string> {
      if (!vscode.window.activeTextEditor) return "";
      const client = ctx.client;

      const params: ra.AnalyzerStatusParams = {};
      const doc = ctx.activeRitobinEditor?.document;
      if (doc != null) {
        params.textDocument =
          client.code2ProtocolConverter.asTextDocumentIdentifier(doc);
      }
      return await client.sendRequest(ra.analyzerStatus, params);
    }

    get onDidChange(): vscode.Event<vscode.Uri> {
      return this.eventEmitter.event;
    }
  })();

  ctx.pushExtCleanup(
    vscode.workspace.registerTextDocumentContentProvider(
      "ritobin-status",
      tdcp,
    ),
  );

  return async () => {
    const document = await vscode.workspace.openTextDocument(tdcp.uri);
    tdcp.eventEmitter.fire(tdcp.uri);
    void (await vscode.window.showTextDocument(document, {
      viewColumn: vscode.ViewColumn.Two,
      preserveFocus: true,
    }));
  };
}

export function onEnter(ctx: CtxInit): Cmd {
  async function handleKeypress() {
    const editor = ctx.activeRitobinEditor;

    if (!editor) return false;

    const client = ctx.client;
    const lcEdits = await client
      .sendRequest(ra.onEnter, {
        textDocument: client.code2ProtocolConverter.asTextDocumentIdentifier(
          editor.document,
        ),
        position: client.code2ProtocolConverter.asPosition(
          editor.selection.active,
        ),
      })
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      .catch((_error: any) => {
        // client.handleFailedRequest(OnEnterRequest.type, error, null);
        return null;
      });
    if (!lcEdits) return false;

    const edits = await client.protocol2CodeConverter.asTextEdits(lcEdits);
    log.info({ edits });
    // await applySnippetTextEdits(editor, edits);
    return true;
  }

  return async () => {
    if (await handleKeypress()) return;

    await vscode.commands.executeCommand("default:type", { text: "\n" });
  };
}
