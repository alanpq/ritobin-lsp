# ritobin-lsp

ritobin-lsp is a language server that provides IDE functionality for editing [ritobin](https://github.com/moonshadow565/ritobin) files, a custom text format to represent League of Legends .bin files. You can use it with any editor that supports the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) (VS Code, Vim, Emacs, Zed, etc.).

# Installation

## VS Code

The extension is available on the [Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=alanpq.ritobin-lsp-vs), and the Open VSX Registry. You can search for it from the extensions pane in VS Code.

### Manual

1. Download the latest `.vsix` from the [releases page](https://github.com/alanpq/ritobin-lsp/releases). Make sure to pick the architecture that matches your OS:
    - `win32-arm64`/`win32-x64`: Windows ARM / x86 64 bit
    - `darwin-arm64`/`darwin-x64`: MacOS M1 / Intel
    - `linux-arm64`/`linux-x64`: Linux ARM / x86 64 bit
    - If you don't know, you probably want `win32-x64`
2. Open the `.vsix` in VS Code.
    - You can also use the `Extensions: Install from VSIX...` command from the command palette (`Ctrl/Cmd + P`)

> [!WARNING]
> The VS Code extension is under the `ritobin-lsp-vs` release, **NOT** the `ritobin-lsp` release. The latter is for usage outside of VS Code.

## Other editors

On the [releases page](https://github.com/alanpq/ritobin-lsp/releases), the binary is available under `ritobin-lsp`.

If you're using VS Code, the extension already bundles a copy of the `ritobin-lsp` binary, so you only need the `.vsix`. For other editors, you'll need to download the binary and configure your editor.

# Usage

## VS Code
Just open a `.rito`/`.ritobin` file, or manually set the language to `Ritobin`!
You can also directly open/save `.bin` files as if they were ritobin.

> [!IMPORTANT]
> `.py` files are **not** recognised as ritobin, to not conflict with actual Python files. Rename your files to `.rito`.

> [!NOTE]
> While you can directly edit `.bin` with the extension, things like comments and specific formatting get lost in the conversion. For that reason we recommend you always save as `.ritobin`, and only convert to `.bin` when you need.

### Hashes

ritobin-lsp automatically updates hashtables, and exposes an `Unhash File` command, which will look up every hash in your file against and replace them with the original values (if known).

### Formatting
The extension registers itself as a formatter for the `Ritobin` language, so you can format as you would any other language.

## Vim/Emacs/etc.

Configure it as you would for any other language server :)

# Features
- [x] Semantic tokens (syntax highlighting)
- [x] Direct opening of `.bin` files
- [x] Formatting
- [x] Diagnostics
- [x] File unhash command
- [x] Automatic hashtable updates (with [Mimir](https://github.com/LeagueToolkit/Mimir))
- [ ] [lol-meta-classes](https://github.com/LeagueToolkit/lol-meta-classes) integration
    - [x] Class property auto-complete
    - [x] Property value auto-complete
    - [x] Class auto-complete
    - [x] Hover information
    - [x] Automatic meta dump updates
- [ ] [LoL Meta Wiki](https://meta-wiki.leaguetoolkit.dev/) integration
    - [x] Links to wiki in hover information
    - [ ] Class/property documentation
- [ ] [modpkg](https://github.com/LeagueToolkit/league-mod/tree/main/crates/ltk_modpkg) support
    - [ ] Linked bin & bin dependency resolution & related autocomplete
    - [ ] Asset resolution & related autocomplete
- [ ] And much more to come :3
