# Rake Language Support for VS Code

Syntax highlighting, snippets, compiler diagnostics, semantic highlighting, completion, symbol navigation, and a live Scratch blocks preview for `.rk` files.

The language server reuses this repository's parser and type checker. It reports the first current compiler error in the open file and refreshes diagnostics as you edit. It also colors namespaces, built-in block calls, functions, classes, enums, enum variants, fields, variables, Scratch lists, and constants by semantic role; completes Scratch namespaces and supported blocks, enum variants, class fields and visible methods, and function calls; and supports hover, references, rename, document symbols, and navigation for functions, methods, classes, enums, enum variants, fields, variables, Scratch lists, and parameters.

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

The compiler and language server support payload-free `enum` declarations with 0-based compile-time tags, typed variables and function arguments/returns, equality, enum field defaults, and `match` expressions/statements over enums and scalar values. Match arms accept literal or enum-variant patterns, an optional final `_` wildcard, value expressions, and statement blocks that return from the current function. Enum variants and bool matches are checked for exhaustiveness. Enum variant completion, hover, navigation, references, and semantic highlighting also work in match patterns. Payload variants are not implemented yet.

The extension follows the compiler's implemented syntax: `ns` namespaces, `use` imports and aliases, classes with primitive fields and methods, `public` / `private` members (public by default), module-scope typed instances created with `ClassName.new()`, `self.field` reads and writes, `self.method()` calls, `instance.field` reads and writes, `instance.method()` calls, Scratch list handles created with `SList.new()`, `fn` and `warp fn` declarations, `event <event> { ... }` handlers, `send <message>` broadcasts, function calls and return values, typed `int` / `float` / `bool` / `text` / `Scratch.Generics.Value` values, `if`, `else`, `while`, `match`, literals, operators, Scratch built-in block namespaces, braces, and `//` comments. `Scratch.Events` provides the green flag, key press, backdrop switch, sprite click, and clone start events; they can be imported with `use` and aliases. Custom messages use the same `event name { ... }` handler syntax, and multiple scripts may receive the same event or message. `new` is a regular method that returns `self`; each declared object gets its own Scratch variables and specialized method blocks. Warp methods and functions set the Scratch custom block option to run without screen refresh. Each event declaration creates an independent Scratch script. Module-scope executable statements are prepended to the sole explicit `Scratch.Events.greenFlag` handler, or remain an implicit green-flag script when none is declared. Completion suggests event members, declared custom messages after `event` and `send`, Scratch namespaces and supported blocks after each namespace segment, list methods on list handles, public fields and visible class methods, function declarations and calls, language keywords, built-in and class types in type annotations, and visible variables/constants/parameters/lists. Hover and document symbols include event handlers, messages, methods, built-in block signatures, and list resource types. Substack control blocks, remaining menu-only blocks, and extensions are not implemented yet.

Macro syntax is highlighted in the editor. The language server offers declared macros in completion, shows their expansion model on hover, and navigates from a call to its declaration. The compiler and language server accept token substitution macros declared with `macro! name (parameters...) { ... }` and invoked with `name!(arguments...)`.
