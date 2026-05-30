<script lang="ts">
  import { onMount } from 'svelte';
  import { monaco, applyMonacoTheme } from '../lib/monaco';
  import { app } from '../lib/simulation-store.svelte';

  interface Props {
    value: string;
    language: string;
    onchange?: (value: string) => void;
    /** Optional extra Monaco IEditorOptions */
    options?: Record<string, unknown>;
  }

  let { value, language, onchange, options = {} }: Props = $props();

  let editorContainer: HTMLDivElement | undefined = $state();
  let editor: monaco.editor.IStandaloneCodeEditor | undefined;

  // Sync the external value into the editor when it changes externally.
  // The guard (v !== editor.getValue()) prevents feedback loops: when the
  // user types, the model already has the new value, so we skip the setValue.
  $effect(() => {
    const v = value;
    if (editor && v !== editor.getValue()) {
      editor.setValue(v);
    }
  });

  // Sync dark theme
  $effect(() => {
    applyMonacoTheme(app.darkTheme);
  });

  onMount(() => {
    if (!editorContainer) return;

    editor = monaco.editor.create(editorContainer, {
      value,
      language,
      theme: app.darkTheme ? 'vs-dark' : 'vs',
      fontSize: 13,
      lineNumbers: 'on',
      minimap: { enabled: false },
      wordWrap: 'on',
      scrollBeyondLastLine: false,
      automaticLayout: true,
      renderWhitespace: 'selection',
      bracketPairColorization: { enabled: true },
      ...options,
    });

    // Report edits back to the parent
    editor.onDidChangeModelContent(() => {
      const newValue = editor!.getValue();
      onchange?.(newValue);
    });

    return () => {
      editor?.dispose();
      editor = undefined;
    };
  });
</script>

<div
  bind:this={editorContainer}
  class="monaco-container"
  role="application"
></div>

<style>
  .monaco-container {
    width: 100%;
    height: 100%;
    min-height: 0;
  }
</style>
