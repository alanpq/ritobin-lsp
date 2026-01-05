import * as vscode from "vscode";
import * as lc from "vscode-languageclient";
// import * as ra from "./lsp_ext";
import * as path from "path";

import type { Cmd, CtxInit } from "./ctx";



export function onEnter(ctx: CtxInit): Cmd {
    async function handleKeypress() {
        const editor = ctx.activeRustEditor;

        if (!editor) return false;

        const client = ctx.client;
        // const lcEdits = await client
        //     .sendRequest(ra.onEnter, {
        //         textDocument: client.code2ProtocolConverter.asTextDocumentIdentifier(
        //             editor.document,
        //         ),
        //         position: client.code2ProtocolConverter.asPosition(editor.selection.active),
        //     })
        //     // eslint-disable-next-line @typescript-eslint/no-explicit-any
        //     .catch((_error: any) => {
        //         // client.handleFailedRequest(OnEnterRequest.type, error, null);
        //         return null;
        //     });
        // if (!lcEdits) return false;

        // const edits = await client.protocol2CodeConverter.asTextEdits(lcEdits);
        // await applySnippetTextEdits(editor, edits);
        return true;
    }

    return async () => {
        if (await handleKeypress()) return;

        await vscode.commands.executeCommand("default:type", { text: "\n" });
    };
}

