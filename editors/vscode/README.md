# Rake Language Support for VS Code

Syntax highlighting, snippets, compiler diagnostics, semantic highlighting, completion, symbol navigation, and a live Scratch blocks preview for `.rk` files.

The language server reuses this repository's parser and type checker. It reports the first current compiler error in the open file and refreshes diagnostics as you edit. It also colors namespaces, built-in block calls, functions, classes, fields, variables, and constants by semantic role; completes Scratch categories and their supported blocks as well as function calls and statically known instance fields; and supports hover, references, rename, document symbols, and navigation for functions, classes, fields, variables, and parameters.

## Scratch blocks preview

Open a `.rk` file and run **Rake: Open Scratch Blocks Preview** from the Command Palette, or click the preview icon in the editor title. VS Code keeps the Rake source in the editor and opens the compiled Scratch 3 blocks beside it. The preview refreshes as the source changes and shows compiler errors when the source is invalid. It is a read-only view of the generated blocks; block editing and the full Scratch stage/editor are not included yet.

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

The extension follows the compiler's implemented syntax: `ns` namespaces, `use` imports and aliases, `class` declarations with primitive fields, statically declared module-scope instances, `instance.field` reads and writes, `fn`, function calls and return values, typed `int` / `float` / `bool` / `text` values, `if`, `else`, `while`, literals, operators, Scratch built-in block namespaces, braces, and `//` comments. Completion suggests Scratch categories and supported blocks after `Scratch.`, class fields for a statically declared instance, function declarations and calls, language keywords, built-in types in type annotations, and visible variables/constants/parameters. Hover shows built-in block signatures. Event hats, substack control blocks, remaining menu-only blocks, list operations, and extensions are not implemented yet.
