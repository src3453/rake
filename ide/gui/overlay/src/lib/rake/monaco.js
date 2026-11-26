// Loads the Monaco editor from the IDE server (/monaco, the AMD build of monaco-editor)
// and registers the Rake and Rake Scene languages.

let monacoPromise = null;

const loadScript = src => new Promise((resolve, reject) => {
    const script = document.createElement('script');
    script.src = src;
    script.onload = resolve;
    script.onerror = () => reject(new Error(`Cannot load ${src}`));
    document.head.appendChild(script);
});

const RAKE_KEYWORDS = [
    'var', 'const', 'fn', 'warp', 'return', 'if', 'else', 'while', 'match', 'class', 'enum', 'ns', 'use', 'as',
    'public', 'private', 'self', 'event', 'send', 'macro'
];
const RAKE_TYPES = ['int', 'float', 'bool', 'text'];

const registerLanguages = monaco => {
    monaco.languages.register({id: 'rake', extensions: ['.rk'], aliases: ['Rake']});
    monaco.languages.register({id: 'rake-scene', extensions: ['.rks'], aliases: ['Rake Scene']});

    monaco.languages.setLanguageConfiguration('rake', {
        comments: {lineComment: '//'},
        brackets: [['{', '}'], ['(', ')']],
        autoClosingPairs: [{open: '{', close: '}'}, {open: '(', close: ')'}, {open: '"', close: '"', notIn: ['string']}],
        surroundingPairs: [{open: '{', close: '}'}, {open: '(', close: ')'}, {open: '"', close: '"'}],
        indentationRules: {increaseIndentPattern: /\{[^}"']*$/, decreaseIndentPattern: /^\s*\}/}
    });
    monaco.languages.setLanguageConfiguration('rake-scene', {
        comments: {lineComment: '//'},
        brackets: [['{', '}'], ['(', ')'], ['[', ']']],
        autoClosingPairs: [
            {open: '{', close: '}'}, {open: '(', close: ')'}, {open: '[', close: ']'},
            {open: '"', close: '"', notIn: ['string']}
        ],
        surroundingPairs: [{open: '{', close: '}'}, {open: '(', close: ')'}, {open: '[', close: ']'}],
        indentationRules: {increaseIndentPattern: /[{[(][^}\])"']*$/, decreaseIndentPattern: /^\s*[}\])]/}
    });

    monaco.languages.setMonarchTokensProvider('rake', {
        keywords: RAKE_KEYWORDS,
        typeKeywords: RAKE_TYPES,
        tokenizer: {
            root: [
                [/\/\/.*$/, 'comment'],
                [/\bScratch(\.[A-Z]\w*)*/, 'namespace'],
                [/[A-Za-z_]\w*(?=\s*!\s*\()/, 'macro'],
                [/[a-z_]\w*(?=\s*\()/, {cases: {'@keywords': 'keyword', '@default': 'function'}}],
                [/[A-Z]\w*/, 'type.identifier'],
                [/[a-z_]\w*/, {cases: {
                    '@keywords': 'keyword', '@typeKeywords': 'type', 'true|false': 'constant', '@default': 'identifier'
                }}],
                [/"/, 'string', '@string'],
                [/\d+(\.\d+)?/, 'number'],
                [/==|!=|<=|>=|&&|\|\||\+=|-=|->|=>|[+\-*/<>=!]/, 'operator'],
                [/[{}()]/, '@brackets']
            ],
            string: [
                [/[^\\"]+/, 'string'],
                [/\\./, 'string.escape'],
                [/"/, 'string', '@pop']
            ]
        }
    });
    monaco.languages.setMonarchTokensProvider('rake-scene', {
        tokenizer: {
            root: [
                [/\/\/.*$/, 'comment'],
                [/\b(scene)(\s+)([A-Za-z_]\w*)/, ['keyword', '', 'namespace']],
                [/\b(sprite)(\s+)([A-Za-z_]\w*)/, ['keyword', '', 'type.identifier']],
                [/\b(stage|true|false)\b/, 'keyword'],
                [/[A-Za-z_]\w*(?=\s*:)/, 'variable'],
                [/"/, 'string', '@string'],
                [/-?\d+(\.\d+)?/, 'number'],
                [/[{}()[\]]/, '@brackets']
            ],
            string: [
                [/[^\\"]+/, 'string'],
                [/\\./, 'string.escape'],
                [/"/, 'string', '@pop']
            ]
        }
    });

    // Colors close to Scratch block categories so code and blocks read alike.
    const rules = dark => [
        {token: 'keyword', foreground: dark ? 'ffab19' : 'cf8b17'},
        {token: 'namespace', foreground: dark ? '5cb1d6' : '2e8eb8'},
        {token: 'function', foreground: dark ? 'ff8cb4' : 'd65c86'},
        {token: 'macro', foreground: dark ? 'ff8cb4' : 'd65c86'},
        {token: 'type', foreground: dark ? '59c059' : '389438'},
        {token: 'type.identifier', foreground: dark ? '59c059' : '389438'},
        {token: 'variable', foreground: dark ? 'ff8c1a' : 'db6e00'},
        {token: 'constant', foreground: dark ? 'ffab19' : 'cf8b17'},
        {token: 'enumMember', foreground: dark ? 'cf63cf' : 'a63fa6'},
        {token: 'comment', foreground: dark ? '7f8c99' : '7a8691', fontStyle: 'italic'}
    ];
    monaco.editor.defineTheme('rake-light', {base: 'vs', inherit: true, rules: rules(false), colors: {}});
    monaco.editor.defineTheme('rake-dark', {
        base: 'vs-dark', inherit: true, rules: rules(true), colors: {'editor.background': '#1e1e1e'}
    });
};

/** Load Monaco once; resolves to the `monaco` namespace. */
export const loadMonaco = () => {
    if (monacoPromise) return monacoPromise;
    monacoPromise = (async () => {
        const base = `${location.origin}/monaco/vs`;
        // Monaco is served from the IDE server's own origin, so the worker script loads directly
        // (it finds the rest of Monaco relative to its own URL).
        window.MonacoEnvironment = {
            getWorkerUrl: () => `${base}/base/worker/workerMain.js`
        };
        await loadScript(`${base}/loader.js`);
        const amdRequire = window.require;
        const language = (navigator.language || 'en').toLowerCase();
        amdRequire.config({
            paths: {vs: base},
            'vs/nls': language.startsWith('ja') ? {availableLanguages: {'*': 'ja'}} : {}
        });
        const monaco = await new Promise((resolve, reject) => {
            amdRequire(['vs/editor/editor.main'], () => resolve(window.monaco), reject);
        });
        // Hide the AMD globals again so scripts loaded later (extensions, addons) are not
        // mistaken for AMD modules.
        window.rakeAmd = {require: window.require, define: window.define};
        window.define = undefined;
        window.require = undefined;
        registerLanguages(monaco);
        return monaco;
    })();
    return monacoPromise;
};

export const languageForPath = relativePath => {
    if (relativePath.endsWith('.rks')) return 'rake-scene';
    if (relativePath.endsWith('.rk')) return 'rake';
    if (relativePath.endsWith('.json')) return 'json';
    if (relativePath.endsWith('.svg')) return 'xml';
    if (relativePath.endsWith('.md')) return 'markdown';
    return 'plaintext';
};
