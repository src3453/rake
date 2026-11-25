// A small LSP client that connects Monaco to `rake-lsp` through the IDE server's /lsp
// WebSocket bridge. It covers what the Rake language server implements: diagnostics,
// completion, hover, definition, references, rename, document symbols, and semantic tokens.

const LANGUAGES = ['rake', 'rake-scene'];

const toMonacoRange = (monaco, range) => new monaco.Range(
    range.start.line + 1, range.start.character + 1, range.end.line + 1, range.end.character + 1
);

const toLspPosition = position => ({line: position.lineNumber - 1, character: position.column - 1});

const markdown = contents => {
    if (!contents) return [];
    if (Array.isArray(contents)) return contents.flatMap(markdown);
    if (typeof contents === 'string') return [{value: contents}];
    if (contents.kind) return [{value: contents.value}];
    if (contents.language) return [{value: `\`\`\`${contents.language}\n${contents.value}\n\`\`\``}];
    return [];
};

// LSP CompletionItemKind (1-based) to Monaco CompletionItemKind.
const completionKind = (monaco, kind) => {
    const K = monaco.languages.CompletionItemKind;
    return ({
        1: K.Text, 2: K.Method, 3: K.Function, 4: K.Constructor, 5: K.Field, 6: K.Variable, 7: K.Class,
        8: K.Interface, 9: K.Module, 10: K.Property, 11: K.Unit, 12: K.Value, 13: K.Enum, 14: K.Keyword,
        15: K.Snippet, 16: K.Color, 17: K.File, 18: K.Reference, 19: K.Folder, 20: K.EnumMember,
        21: K.Constant, 22: K.Struct, 23: K.Event, 24: K.Operator, 25: K.TypeParameter
    })[kind] || K.Text;
};

export class RakeLanguageClient {
    constructor (monaco, {onStatus = () => {}, openUri = () => {}} = {}) {
        this.monaco = monaco;
        this.onStatus = onStatus;
        this.openUri = openUri;
        this.nextId = 1;
        this.pending = new Map();
        this.versions = new Map();
        this.diagnostics = new Map();
        this.diagnosticListeners = new Set();
        this.disposables = [];
        this.legend = null;
        this.ready = null;
        this.closed = false;
    }

    connect () {
        const url = `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/lsp`;
        this.onStatus('connecting');
        this.socket = new WebSocket(url);
        this.socket.onmessage = event => this.handle(JSON.parse(event.data));
        this.socket.onclose = () => {
            for (const {reject} of this.pending.values()) reject(new Error('language server disconnected'));
            this.pending.clear();
            this.onStatus('disconnected');
            if (!this.closed) {
                // Reconnect and reopen documents if the server restarts.
                setTimeout(() => {
                    if (!this.closed) this.start();
                }, 2000);
            }
        };
        this.ready = new Promise((resolve, reject) => {
            this.socket.onopen = () => resolve();
            this.socket.onerror = () => reject(new Error('cannot connect to the language server'));
        });
        return this.ready;
    }

    async start () {
        await this.connect();
        const result = await this.request('initialize', {
            processId: null,
            rootUri: null,
            capabilities: {textDocument: {publishDiagnostics: {}, completion: {completionItem: {snippetSupport: true}}}}
        });
        this.capabilities = result.capabilities || {};
        this.legend = (this.capabilities.semanticTokensProvider || {}).legend || null;
        this.notify('initialized', {});
        if (!this.disposables.length) this.registerProviders();
        // (Re)open documents that were already in the editor.
        this.versions.clear();
        for (const model of this.monaco.editor.getModels()) this.openModel(model);
        this.onStatus('ready');
    }

    dispose () {
        this.closed = true;
        this.disposables.forEach(disposable => disposable.dispose());
        this.disposables = [];
        if (this.socket) this.socket.close();
    }

    send (message) {
        if (this.socket && this.socket.readyState === WebSocket.OPEN) {
            this.socket.send(JSON.stringify({jsonrpc: '2.0', ...message}));
        }
    }

    request (method, params) {
        const id = this.nextId++;
        return new Promise((resolve, reject) => {
            this.pending.set(id, {resolve, reject});
            this.send({id, method, params});
        });
    }

    notify (method, params) {
        this.send({method, params});
    }

    handle (message) {
        if (message.id !== undefined && this.pending.has(message.id)) {
            const {resolve, reject} = this.pending.get(message.id);
            this.pending.delete(message.id);
            if (message.error) reject(new Error(message.error.message));
            else resolve(message.result);
            return;
        }
        if (message.method === 'textDocument/publishDiagnostics') {
            this.applyDiagnostics(message.params);
        }
    }

    applyDiagnostics ({uri, diagnostics}) {
        this.diagnostics.set(uri, diagnostics);
        const model = this.monaco.editor.getModel(this.monaco.Uri.parse(uri));
        if (model) {
            this.monaco.editor.setModelMarkers(model, 'rake-lsp', diagnostics.map(diagnostic => ({
                ...toMonacoRange(this.monaco, diagnostic.range),
                startLineNumber: diagnostic.range.start.line + 1,
                startColumn: diagnostic.range.start.character + 1,
                endLineNumber: diagnostic.range.end.line + 1,
                endColumn: Math.max(diagnostic.range.end.character + 1, diagnostic.range.start.character + 2),
                message: diagnostic.message,
                severity: diagnostic.severity === 1 ? this.monaco.MarkerSeverity.Error :
                    diagnostic.severity === 2 ? this.monaco.MarkerSeverity.Warning : this.monaco.MarkerSeverity.Info,
                source: diagnostic.source || 'rake'
            })));
        }
        this.diagnosticListeners.forEach(listener => listener(uri, diagnostics));
    }

    onDiagnostics (listener) {
        this.diagnosticListeners.add(listener);
        return () => this.diagnosticListeners.delete(listener);
    }

    tracks (model) {
        return LANGUAGES.includes(model.getLanguageId()) && model.uri.scheme === 'file';
    }

    openModel (model) {
        if (!this.tracks(model) || this.versions.has(model.uri.toString())) return;
        const uri = model.uri.toString();
        this.versions.set(uri, 1);
        this.notify('textDocument/didOpen', {
            textDocument: {uri, languageId: model.getLanguageId(), version: 1, text: model.getValue()}
        });
    }

    changeModel (model) {
        const uri = model.uri.toString();
        if (!this.versions.has(uri)) {
            this.openModel(model);
            return;
        }
        const version = this.versions.get(uri) + 1;
        this.versions.set(uri, version);
        this.notify('textDocument/didChange', {
            textDocument: {uri, version},
            contentChanges: [{text: model.getValue()}]
        });
    }

    closeModel (model) {
        const uri = model.uri.toString();
        if (!this.versions.has(uri)) return;
        this.versions.delete(uri);
        this.diagnostics.delete(uri);
        this.notify('textDocument/didClose', {textDocument: {uri}});
        this.diagnosticListeners.forEach(listener => listener(uri, []));
    }

    registerProviders () {
        const monaco = this.monaco;
        const params = (model, position) => ({
            textDocument: {uri: model.uri.toString()},
            position: toLspPosition(position)
        });
        const guard = async task => {
            try {
                return await task();
            } catch (e) {
                return null;
            }
        };
        const location = target => ({
            uri: monaco.Uri.parse(target.uri),
            range: toMonacoRange(monaco, target.range)
        });

        for (const language of LANGUAGES) {
            this.disposables.push(monaco.languages.registerCompletionItemProvider(language, {
                triggerCharacters: ['.', '"', '/', ':', ' '],
                provideCompletionItems: (model, position) => guard(async () => {
                    const result = await this.request('textDocument/completion', params(model, position));
                    const items = Array.isArray(result) ? result : ((result && result.items) || []);
                    const word = model.getWordUntilPosition(position);
                    const defaultRange = new monaco.Range(
                        position.lineNumber, word.startColumn, position.lineNumber, word.endColumn
                    );
                    return {
                        suggestions: items.map(item => ({
                            label: item.label,
                            kind: completionKind(monaco, item.kind),
                            detail: item.detail,
                            documentation: item.documentation && (typeof item.documentation === 'string' ?
                                item.documentation : {value: item.documentation.value}),
                            insertText: item.textEdit ? item.textEdit.newText : (item.insertText || item.label),
                            insertTextRules: item.insertTextFormat === 2 ?
                                monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet : undefined,
                            range: item.textEdit ? toMonacoRange(monaco, item.textEdit.range) : defaultRange,
                            sortText: item.sortText,
                            filterText: item.filterText
                        }))
                    };
                })
            }));
            this.disposables.push(monaco.languages.registerHoverProvider(language, {
                provideHover: (model, position) => guard(async () => {
                    const result = await this.request('textDocument/hover', params(model, position));
                    if (!result) return null;
                    return {
                        contents: markdown(result.contents),
                        range: result.range ? toMonacoRange(monaco, result.range) : undefined
                    };
                })
            }));
            this.disposables.push(monaco.languages.registerDefinitionProvider(language, {
                provideDefinition: (model, position) => guard(async () => {
                    const result = await this.request('textDocument/definition', params(model, position));
                    if (!result) return null;
                    const targets = Array.isArray(result) ? result : [result];
                    // Make sure the target file has a model so Monaco can peek and open it.
                    for (const target of targets) await this.openUri(target.uri, false);
                    return targets.map(location);
                })
            }));
            this.disposables.push(monaco.languages.registerReferenceProvider(language, {
                provideReferences: (model, position, context) => guard(async () => {
                    const result = await this.request('textDocument/references', {
                        ...params(model, position),
                        context: {includeDeclaration: context.includeDeclaration}
                    });
                    return (result || []).map(location);
                })
            }));
            this.disposables.push(monaco.languages.registerRenameProvider(language, {
                provideRenameEdits: (model, position, newName) => guard(async () => {
                    const result = await this.request('textDocument/rename', {...params(model, position), newName});
                    const edits = [];
                    for (const [uri, changes] of Object.entries((result && result.changes) || {})) {
                        for (const change of changes) {
                            edits.push({
                                resource: monaco.Uri.parse(uri),
                                versionId: undefined,
                                textEdit: {range: toMonacoRange(monaco, change.range), text: change.newText}
                            });
                        }
                    }
                    return {edits};
                }),
                resolveRenameLocation: (model, position) => guard(async () => {
                    const result = await this.request('textDocument/prepareRename', params(model, position));
                    if (!result) return {rejectReason: 'This symbol cannot be renamed', range: undefined, text: ''};
                    return {range: toMonacoRange(monaco, result.range), text: result.placeholder};
                })
            }));
            this.disposables.push(monaco.languages.registerDocumentSymbolProvider(language, {
                provideDocumentSymbols: model => guard(async () => {
                    const result = await this.request('textDocument/documentSymbol', {
                        textDocument: {uri: model.uri.toString()}
                    });
                    const convert = symbol => ({
                        name: symbol.name,
                        detail: symbol.detail || '',
                        kind: Math.max(0, (symbol.kind || 13) - 1),
                        tags: [],
                        range: toMonacoRange(monaco, symbol.range || symbol.location.range),
                        selectionRange: toMonacoRange(monaco, symbol.selectionRange || symbol.range || symbol.location.range),
                        children: (symbol.children || []).map(convert)
                    });
                    return (result || []).map(convert);
                })
            }));
            if (this.legend) {
                this.disposables.push(monaco.languages.registerDocumentSemanticTokensProvider(language, {
                    getLegend: () => this.legend,
                    provideDocumentSemanticTokens: model => guard(async () => {
                        const result = await this.request('textDocument/semanticTokens/full', {
                            textDocument: {uri: model.uri.toString()}
                        });
                        return {data: new Uint32Array((result && result.data) || [])};
                    }),
                    releaseDocumentSemanticTokens: () => {}
                }));
            }
        }
    }
}
