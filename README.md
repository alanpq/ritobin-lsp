# ritobin-lsp

ritobin-lsp is a language server that provides IDE functionality for editing [ritobin](https://github.com/moonshadow565/ritobin) files, a custom text format to represent League of Legends .bin files. You can use it with any editor that supports the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/) (VS Code, Vim, Emacs, Zed, etc.).

# Installation
If you're using VS Code, the extension bundles a copy of the `ritobin-lsp` binary. For other editors, you'll need to download the binary and configure your editor.

# Features
- [x] Semantic tokens (syntax highlighting)
- [x] Formatting
- [x] Diagnostics
- [x] File unhash command
- [ ] Automatic hash updates
- [x] [lol-meta-classes](https://github.com/LeagueToolkit/lol-meta-classes) integration
    - [x] Class property auto-complete
    - [x] Class auto-complete
    - [x] Hover information
    - [x] Automatic meta dump updates
- [ ] [LoL Meta Wiki](https://meta-wiki.leaguetoolkit.dev/) integration
    - [x] Links to wiki in hover information
    - [ ] Class/property documentation
- [ ] And much more to come :3
