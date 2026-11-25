# Rake Language Support for VS Code

Syntax highlighting, snippets, compiler diagnostics, and keyword/type completion for `.rk` files.

The language server reuses this repository's parser and type checker. It reports the first current compiler error in the open file and refreshes diagnostics as you edit.

## Build and run from this repository

1. Install the extension dependencies and compile its client and bundled language server:

   ```sh
   cd editors/vscode
   npm install
   npm run vscode:prepublish
   ```

   This builds a server executable for the current OS and architecture and places it in the extension package.

2. Open this folder in VS Code and press F5 to launch an Extension Development Host.

3. To use a separately installed server, set `rake.serverPath` to its executable path. The default empty value uses the bundled executable.

To create a VSIX, run `npx vsce package` from `editors/vscode`. The package contains a language server built for the OS and architecture used during packaging.

## Current language support

The extension follows the compiler's implemented syntax: `var`, `const`, `if`, `else`, `while`, `int`, `float`, `bool`, boolean literals, numeric literals, operators, braces, and `//` comments. Completion currently suggests language keywords and built-in types.
