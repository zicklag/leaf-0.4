// ---------------------------------------------------------------------------
// Monaco Editor initialisation & Rego language registration
// ---------------------------------------------------------------------------

import * as monaco from 'monaco-editor';
import EditorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';
import JsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker';

// ── Worker setup ──────────────────────────────────────────────────────────
(self as unknown as Record<string, unknown>).MonacoEnvironment = {
  getWorker(_: unknown, label: string) {
    if (label === 'json') return new JsonWorker();
    return new EditorWorker();
  },
};

// ── Rego language definition (Monarch tokens) ────────────────────────────
monaco.languages.register({ id: 'rego' });

monaco.languages.setMonarchTokensProvider('rego', {
  defaultToken: 'identifier',

  keywords: [
    'default', 'not', 'package', 'import', 'as', 'with', 'else',
    'some', 'in', 'every', 'if', 'contains',
  ],

  typeKeywords: ['true', 'false', 'null'],

  operators: [
    '=', '!=', '<', '>', '<=', '>=',
    '+', '-', '*', '/', '%',
    ':', ':=', '|', '&',
    '&&', '||',
  ],

  // We only tokenize the built-in name patterns; full dotted resolution
  // is handled by the identifier fallthrough below.
  builtins: [
    // Aggregation
    'count', 'sum', 'max', 'min', 'product', 'any', 'all', 'sort', 'union', 'intersection',
    // Strings
    'sprintf', 'concat', 'replace', 'split', 'trim', 'trim_left', 'trim_right',
    'trim_prefix', 'trim_suffix', 'substring', 'contains', 'startswith', 'endswith',
    'indexof', 'last_indexof', 'lower', 'upper', 'format_int', 'reverse',
    // Regex
    're_match',
    // Objects
    'object.get', 'object.keys', 'object.union', 'object.remove', 'object.filter',
    // Arrays / Sets
    'array.reverse', 'array.slice', 'array.concat',
    // Type checks
    'is_number', 'is_string', 'is_boolean', 'is_array', 'is_set', 'is_object', 'is_null',
    'type_name',
    // Time
    'time.now_ns', 'time.parse_ns', 'time.parse_rfc3339_ns',
    'time.clock', 'time.date', 'time.diff',
    // Misc
    'opa.runtime', 'print', 'trace', 'walk', 'set',
    // Muni Town builtins
    'xrpc_local', 'xrpc_remote',
  ],

  tokenizer: {
    root: [
      // ── Whitespace ──
      { include: '@whitespace' },

      // ── Numbers ──
      [/\d+(\.\d+)?([eE][+-]?\d+)?/, 'number'],

      // ── Strings: double-quoted ──
      [/"/, { token: 'string.quote', next: '@stringDouble' }],

      // ── Strings: raw backtick ──
      [/`/, { token: 'string.quote', next: '@stringBacktick' }],

      // ── Comments ──
      [/#.*$/, 'comment'],

      // ── Punctuation / brackets ──
      [/[{}()\[\]]/, '@brackets'],
      [/[;,]/, 'delimiter'],

      // ── Operators ──
      [/[<>!=]=?/, 'delimiter'],
      [/&&|\|\||[+\-*/%:]/, 'delimiter'],

      // ── Identifiers ──
      // Uppercase-starting identifiers are always variables
      [/[A-Z_][a-zA-Z0-9_$]*(?:\.[a-zA-Z_$][a-zA-Z0-9_$]*)*/, 'variable'],
      // Lowercase-starting identifiers: keywords / builtins / plain identifiers
      [/[a-z][a-zA-Z0-9_$]*(?:\.[a-zA-Z_$][a-zA-Z0-9_$]*)*/, {
        cases: {
          '@keywords': 'keyword',
          '@typeKeywords': 'type',
          '@builtins': 'type.identifier',
          '@default': 'identifier',
        },
      }],
    ],

    whitespace: [
      [/[ \t\r\n]+/, 'white'],
    ],

    stringDouble: [
      [/[^"\\]+/, 'string'],
      [/\\./, 'string.escape'],
      [/"/, { token: 'string.quote', next: '@pop' }],
    ],

    stringBacktick: [
      [/[^`\\]+/, 'string'],
      [/\\./, 'string.escape'],
      [/`/, { token: 'string.quote', next: '@pop' }],
    ],
  },
});

// ── Rego completion provider (basic) ──────────────────────────────────────
monaco.languages.registerCompletionItemProvider('rego', {
  provideCompletionItems: (_model, _position) => {
    const suggestions: monaco.languages.CompletionItem[] = [
      // Keywords
      ...['package', 'import', 'default', 'if', 'else', 'not', 'some', 'in',
        'every', 'with', 'as', 'contains'].map((kw) => ({
        label: kw,
        kind: monaco.languages.CompletionItemKind.Keyword,
        insertText: kw,
        range: undefined as unknown as monaco.IRange,
      })),
      // Common builtins
      ...['count', 'sum', 'max', 'min', 'sprintf', 'concat', 'split',
        'startswith', 'endswith', 'contains', 'lower', 'upper',
        'object.get', 'object.keys', 'object.union',
        'is_string', 'is_number', 'is_array', 'is_object',
        'time.now_ns', 'time.clock', 'time.date', 'print', 'trace',
        'walk', 'set', 'xrpc_local', 'xrpc_remote',
      ].map((fn) => ({
        label: fn,
        kind: monaco.languages.CompletionItemKind.Function,
        insertText: fn,
        range: undefined as unknown as monaco.IRange,
      })),
    ];
    return { suggestions };
  },
});

// ── Dark / light theme sync ───────────────────────────────────────────────
export function applyMonacoTheme(dark: boolean): void {
  monaco.editor.setTheme(dark ? 'vs-dark' : 'vs');
}

export { monaco };
