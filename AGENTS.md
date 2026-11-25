# Repository instructions

## Keep the language tools in sync

When changing Rake's language syntax, semantics, type checking, diagnostics, or generated Scratch behavior, update the Language Server and the VS Code extension in the same change so they match the compiler. Review the relevant LSP behavior (diagnostics, completion, hover, navigation, and semantic tokens) and extension behavior (syntax highlighting, snippets, commands, and documentation), and update each affected part.
