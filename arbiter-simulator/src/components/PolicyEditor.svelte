<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import MonacoEditor from './MonacoEditor.svelte';

  // ── Policy source ──────────────────────────────────────────────────────
  // app.policy returns the selected arbiter's policy (if any) or the default.
  // We track an override for when the user is editing, and reset it whenever
  // the underlying source changes (arbiter selection, applied policy, etc.).
  let override = $state<string | null>(null);
  let isDirty = $state(false);

  let displayValue = $derived.by(() => {
    if (override !== null) return override;
    return app.policy;
  });

  // Reset edits when the source changes
  let validationMsg = $state<string | null>(null);
  let validationOk = $state(false);
  let validateTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    // Track the policy source — triggers reset whenever the source arbiter
    // changes or a policy is applied externally.
    app.policy;
    override = null;
    isDirty = false;
    validationMsg = null;
    validationOk = false;
  });

  function handleChange(value: string) {
    override = value;
    isDirty = true;
    validationMsg = null;
    debounceValidate(value);
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
    app.setPolicy(override ?? displayValue);
    override = null;
    isDirty = false;
    app.notifications.add('success', 'Policy applied to all arbiters');
  }

  function handleReset() {
    override = app.getDefaultPolicy();
    isDirty = true;
    validationMsg = null;
    validationOk = false;
  }

  // ── Source label ───────────────────────────────────────────────────────
  let sourceLabel = $derived(
    app.selectedArbiter
      ? `Showing policy of ${app.selectedArbiter.did}`
      : 'Showing default policy (no arbiter selected)'
  );
</script>

<div class="policy-editor">
  <div class="editor-toolbar">
    <div class="toolbar-left">
      <h3>Policy Editor</h3>
      <span class="editor-hint">
        {sourceLabel}
        {#if isDirty}
          <span class="dirty-indicator"> ● unsaved</span>
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
    <MonacoEditor
      value={displayValue}
      language="rego"
      onchange={handleChange}
    />
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
    min-height: 0;
    overflow: hidden;
  }
</style>
