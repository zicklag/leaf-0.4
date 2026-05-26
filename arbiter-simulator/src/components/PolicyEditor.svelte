<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import { highlightRego } from '../lib/rego-highlight';

  let policyCode = $state('');
  let validationMsg = $state<string | null>(null);
  let validationOk = $state(false);
  let isDirty = $state(false);
  let validateTimer: ReturnType<typeof setTimeout> | undefined;
  let textareaEl: HTMLTextAreaElement | undefined;
  let overlayEl: HTMLDivElement | undefined;
  let lineCount = $state(1);

  // Load the default policy from the engine on mount
  $effect(() => {
    try {
      policyCode = app.policy;
      lineCount = policyCode.split('\n').length;
    } catch {
      policyCode = 'package arbiter\nimport rego.v1\ndefault allow := false';
      lineCount = 1;
    }
  });

  // Sync scroll between textarea and overlay
  function handleScroll() {
    if (overlayEl && textareaEl) {
      overlayEl.scrollTop = textareaEl.scrollTop;
      overlayEl.scrollLeft = textareaEl.scrollLeft;
    }
  }

  function handleInput() {
    isDirty = true;
    validationMsg = null;
    lineCount = policyCode.split('\n').length;

    clearTimeout(validateTimer);
    validateTimer = setTimeout(() => {
      const err = app.validatePolicy(policyCode);
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
    app.setPolicy(policyCode);
    isDirty = false;
    app.refreshState();
    app.notifications.add('success', 'Policy applied to all arbiters');
  }

  function handleReset() {
    policyCode = app.getDefaultPolicy();
    validationMsg = null;
    validationOk = false;
    isDirty = true;
    lineCount = policyCode.split('\n').length;
  }

  // Syntax-highlighted version
  let highlighted = $derived(highlightRego(policyCode));
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

  <div class="editor-body">
    <div class="editor-gutter">
      {#each Array(lineCount) as _, i}
        <span class="gutter-line">{i + 1}</span>
      {/each}
    </div>
    <div class="editor-input-area">
      <!-- Syntax highlighted overlay -->
      <div class="editor-overlay" bind:this={overlayEl}>
        {@html highlighted}
      </div>
      <!-- Transparent textarea for actual editing -->
      <textarea
        class="editor-textarea"
        bind:this={textareaEl}
        bind:value={policyCode}
        oninput={handleInput}
        onscroll={handleScroll}
        spellcheck="false"
        wrap="off"
      ></textarea>
    </div>
  </div>
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
    display: flex;
    min-height: 0;
    overflow: hidden;
    font-family: ui-monospace, "SF Mono", "Cascadia Code", "JetBrains Mono", Menlo, monospace;
    font-size: 13px;
    line-height: 1.5;
  }

  .editor-gutter {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    padding: 12px 4px 12px 8px;
    background: var(--bg-surface);
    border-right: 1px solid var(--border-light);
    user-select: none;
    min-width: 36px;
    flex-shrink: 0;
  }

  .gutter-line {
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.5;
    height: 1.5em;
  }

  .editor-input-area {
    flex: 1;
    position: relative;
    overflow: hidden;
    min-height: 0;
  }

  .editor-overlay {
    position: absolute;
    inset: 0;
    padding: 12px 16px;
    overflow: auto;
    white-space: pre;
    color: var(--text-primary);
    pointer-events: none;
    /* Make overlay scrollbar invisible - textarea handles scrolling */
    scrollbar-width: none;
  }

  .editor-overlay::-webkit-scrollbar {
    display: none;
  }

  .editor-textarea {
    /* Positioned on top of overlay, but transparent */
    position: absolute;
    inset: 0;
    padding: 12px 16px;
    border: none;
    background: transparent;
    color: transparent;
    caret-color: var(--text-primary);
    font-family: inherit;
    font-size: inherit;
    line-height: inherit;
    resize: none;
    outline: none;
    tab-size: 2;
    white-space: pre;
    overflow: auto;
  }

  .editor-textarea::placeholder {
    color: var(--text-muted);
    font-style: italic;
  }

  /* Syntax highlighting colors */
  :global(.hl-comment) { color: oklch(0.55 0.02 180); font-style: italic; }
  :global(.hl-keyword) { color: oklch(0.5 0.18 270); font-weight: 600; }
  :global(.hl-string) { color: oklch(0.55 0.15 145); }
  :global(.hl-number) { color: oklch(0.5 0.15 30); }
  :global(.hl-builtin) { color: oklch(0.5 0.18 200); font-weight: 500; }
  :global(.hl-function) { color: oklch(0.5 0.16 250); }
  :global(.hl-rule) { color: oklch(0.5 0.18 65); font-weight: 600; }
  :global(.hl-operator) { color: oklch(0.45 0.05 260); }
  :global(.hl-punctuation) { color: oklch(0.55 0.03 260); }
  :global(.hl-identifier) { color: var(--text-primary); }
</style>
