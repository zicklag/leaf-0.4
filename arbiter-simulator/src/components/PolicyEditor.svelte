<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import { EditorView, basicSetup } from 'codemirror';
  import { EditorState } from '@codemirror/state';
  import { regoLanguage } from '../lib/rego-lang';
  import { oneDark } from '@codemirror/theme-one-dark';

  let editorEl: HTMLDivElement | undefined;
  let editorView: EditorView | undefined;
  let validationMsg = $state<string | null>(null);
  let validationOk = $state(false);
  let isDirty = $state(false);
  let validateTimer: ReturnType<typeof setTimeout> | undefined;

  // Build the CodeMirror extensions
  function makeExtensions() {
    const theme = app.darkTheme ? [oneDark] : [];

    return [
      basicSetup,
      regoLanguage,
      ...theme,
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          isDirty = true;
          validationMsg = null;
          debounceValidate(update.state.doc.toString());
        }
      }),
      EditorView.theme({
        '&': { height: '100%' },
        '.cm-scroller': { overflow: 'auto' },
        '.cm-content': { fontFamily: 'ui-monospace, "SF Mono", "Cascadia Code", "JetBrains Mono", Menlo, monospace', fontSize: '13px', lineHeight: '1.5' },
        '.cm-gutters': { fontFamily: 'ui-monospace, "SF Mono", "Cascadia Code", "JetBrains Mono", Menlo, monospace', fontSize: '11px' },
      }),
    ];
  }

  function debounceValidate(code: string) {
    clearTimeout(validateTimer);
    validateTimer = setTimeout(() => {
      const err = app.validatePolicy(code);
      if (err) {
        validationMsg = err;
        validationOk = false;
      } else {
        validationMsg = 'Policy is valid ✓';
        validationOk = true;
      }
    }, 800);
  }

  function handleApply() {
    if (!editorView) return;
    const code = editorView.state.doc.toString();
    app.setPolicy(code);
    isDirty = false;
    app.notifications.add('success', 'Policy applied to all arbiters');
  }

  function handleReset() {
    if (!editorView) return;
    const defaultPolicy = app.getDefaultPolicy();
    editorView.dispatch({
      changes: { from: 0, to: editorView.state.doc.length, insert: defaultPolicy },
    });
    validationMsg = null;
    validationOk = false;
    isDirty = true;
  }

  // Init editor on mount
  $effect(() => {
    if (!editorEl) return;

    const state = EditorState.create({
      doc: app.policy,
      extensions: makeExtensions(),
    });

    editorView = new EditorView({
      state,
      parent: editorEl,
    });

    return () => {
      editorView?.destroy();
      editorView = undefined;
    };
  });
</script>

<div class="policy-editor">
  <div class="editor-toolbar">
    <div class="toolbar-left">
      <h3>Policy Editor</h3>
      <span class="editor-hint">
        Edit the Rego policy applied to all arbiters.
        {#if isDirty}
          <span class="dirty-indicator">● unsaved</span>
        {/if}
      </span>
    </div>
    <div class="toolbar-right">
      {#if validationMsg}
        <span class="validation-badge" class:valid={validationOk} class:invalid={!validationOk}>
          {validationOk ? '✓' : '✗'} {validationMsg}
        </span>
      {/if}
      <button class="btn btn-sm" onclick={handleReset}>↺ Reset</button>
      <button
        class="btn btn-primary btn-sm"
        onclick={handleApply}
        disabled={!validationOk && validationMsg !== null}
      >
        Apply to All
      </button>
    </div>
  </div>

  <div class="editor-body" bind:this={editorEl}></div>
</div>

<style>
  .policy-editor {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-base);
  }

  .editor-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .toolbar-left { display: flex; align-items: center; gap: 12px; }
  .toolbar-left h3 { font-weight: 600; font-size: 0.929rem; }
  .editor-hint { font-size: 0.786rem; color: var(--text-muted); }
  .dirty-indicator { color: oklch(0.55 0.15 65); font-weight: 500; }
  .toolbar-right { display: flex; align-items: center; gap: 8px; }
  .validation-badge { font-size: 0.714rem; padding: 3px 8px; border-radius: var(--radius-xs); font-weight: 500; max-width: 300px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .validation-badge.valid { background: oklch(0.92 0.06 145 / 0.4); color: oklch(0.4 0.12 145); }
  .validation-badge.invalid { background: oklch(0.92 0.06 25 / 0.4); color: oklch(0.45 0.15 25); }

  .editor-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
</style>
