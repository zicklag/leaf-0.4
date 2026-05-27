<script lang="ts">
  import { Box } from '@foxui/core';
  import { onMount } from 'svelte';

  let { value, onChange }: { value: string; onChange?: (v: string) => void } = $props();

  let containerEl: HTMLDivElement;
  let editor: any = null;

  // Rego syntax highlighting configuration for Monaco
  const regoLanguageConfig: any = {
    defaultToken: '',
    tokenPostfix: '.rego',
    keywords: [
      'package',
      'import',
      'as',
      'data',
      'input',
      'with',
      'default',
      'true',
      'false',
      'null',
      'some',
      'in',
      'not',
      'and',
      'or',
      'all',
      'any',
      'if',
    ],
    typeKeywords: ['set', 'object', 'array', 'string', 'number', 'boolean'],
    operators: ['=', '==', '!=', '<', '<=', '>', '>=', ':', ':=', '.', '+', '-', '*', '/', '%'],
    symbols: /[=><!~?:&|+\-*/^%]+/,
    escapes: /\\(?:[abfnrtv\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,
    digits: /\d+/,
    octaldigits: /[0-7]+/,
    binarydigits: /[01]+/,
    hexdigits: /[0-9a-fA-F]+/,
    tokenizer: {
      root: [
        [/package\s+/, { token: 'keyword' }],
        [/import\s+/, { token: 'keyword' }],
        [/(default)\s+/, { token: 'keyword' }],
        [
          /[a-z_$][\w$]*/,
          {
            cases: {
              '@keywords': 'keyword',
              '@typeKeywords': 'type',
              '@default': 'identifier',
            },
          },
        ],
        { include: '@whitespace' },
        [/[{}()[\]]/, '@brackets'],
        [/[<>](?!@symbols)/, '@brackets'],
        [/@symbols/, { cases: { '@operators': 'operator', '@default': '' } }],
        [/\d*\.\d+([eE][\-+]?\d+)?/, 'number.float'],
        [/\d+/, 'number'],
        [/[;,.]/, 'delimiter'],
        [/"([^"\\]|\\.)*$/, 'string.invalid'],
        [/"/, { token: 'string.quote', bracket: '@open', next: '@string' }],
        [/'[^\\']'/, 'string'],
        [/(')(@escapes)(')/, ['string', 'string.escape', 'string']],
        [/'/, 'string.invalid'],
      ],
      comment: [
        [/[^#]+/, 'comment'],
        [/#.*$/, 'comment'],
      ],
      string: [
        [/[^\\"]+/, 'string'],
        [/@escapes/, 'string.escape'],
        [/\\./, 'string.escape.invalid'],
        [/"/, { token: 'string.quote', bracket: '@close', next: '@pop' }],
      ],
      whitespace: [
        [/[ \t\r\n]+/, ''],
        [/#.*$/, 'comment'],
      ],
    },
  };

  onMount(async () => {
    const monaco = await import('monaco-editor');

    // Register Rego language
    monaco.languages.register({ id: 'rego' });
    monaco.languages.setMonarchTokensProvider('rego', regoLanguageConfig);
    monaco.languages.setLanguageConfiguration('rego', {
      comments: { lineComment: '#' },
      brackets: [
        ['{', '}'],
        ['[', ']'],
        ['(', ')'],
      ],
      autoClosingPairs: [
        { open: '{', close: '}' },
        { open: '[', close: ']' },
        { open: '(', close: ')' },
        { open: '"', close: '"' },
        { open: "'", close: "'" },
      ],
    });

    // Create editor
    editor = monaco.editor.create(containerEl, {
      value,
      language: 'rego',
      theme: 'vs-dark',
      minimap: { enabled: false },
      automaticLayout: true,
      fontSize: 13,
      lineNumbers: 'on',
      scrollBeyondLastLine: false,
      wordWrap: 'on',
      tabSize: 2,
      padding: { top: 12, bottom: 12 },
      renderLineHighlight: 'line',
      folding: true,
    });

    // Sync changes back
    editor.onDidChangeModelContent(() => {
      const v = editor.getValue();
      onChange?.(v);
    });
  });

  // Sync external value changes into the editor (if it's been created)
  $effect(() => {
    if (editor && editor.getValue() !== value) {
      editor.setValue(value);
    }
  });
</script>

<Box class="p-0 overflow-hidden border border-base-200 dark:border-base-800">
  <div bind:this={containerEl} class="h-96"></div>
</Box>
