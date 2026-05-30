<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import MonacoEditor from './MonacoEditor.svelte';

  let {
    selectedArbiterDid,
    selectedSpaceKey,
    currentUser,
    serverState,
  } = $derived(app);

  let selectedArbiter = $derived(
    serverState?.arbiters.find((a) => a.did === selectedArbiterDid) ?? null,
  );

  let selectedSpace = $derived(
    selectedArbiter?.spaces.find((s) => s.key === selectedSpaceKey) ?? null,
  );

  // ── Config JSON ───────────────────────────────────────────────────────────
  // We keep an "override" value for when the user is editing, and fall back
  // to a $derived when idle / selection changes. This way Monaco always gets
  // the correct value up front — no empty flash.
  let configOverride = $state<string | null>(null);
  let configDirty = $state(false);
  let configError = $state<string | null>(null);

  let configDisplayValue = $derived.by(() => {
    if (configOverride !== null) return configOverride;
    if (!selectedSpace) return '{}';
    try {
      return JSON.stringify(selectedSpace.config ?? {}, null, 2);
    } catch { return '{}'; }
  });

  // Reset override when selection changes
  $effect(() => {
    // Access selectedSpace to track it
    const _space = selectedSpace;
    configOverride = null;
    configDirty = false;
    configError = null;
  });

  function handleConfigChange(val: string) {
    configOverride = val;
    configDirty = true;
    configError = null;
  }

  async function saveConfig() {
    if (!currentUser || !selectedArbiterDid || !selectedSpace || !selectedSpaceKey) return;

    const activeVal = configOverride ?? configDisplayValue;
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(activeVal);
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        throw new Error('Config must be a JSON object');
      }
    } catch (e) {
      app.notifications.add('error', `Invalid JSON: ${e}`);
      return;
    }

    const spaceType = selectedSpace.spaceType || 'town.muni.arbiter.config.space';
    const result = await app.runOp(
      selectedArbiterDid,
      currentUser.did,
      (log) => app.simulator.setSpaceConfig(
        selectedArbiterDid!,
        currentUser!.did,
        { spaceKey: selectedSpaceKey, spaceType, config: parsed },
        log,
      ),
    );

    if (result.status === 'ok') {
      app.notifications.add('success', 'Config saved');
      configOverride = null;
      configDirty = false;
    } else {
      app.notifications.add('error', result.error ?? 'Failed to save config');
    }
  }

  // ── Members JSON ──────────────────────────────────────────────────────────
  let membersOverride = $state<string | null>(null);
  let membersDirty = $state(false);
  let membersError = $state<string | null>(null);

  let membersDisplayValue = $derived.by(() => {
    if (membersOverride !== null) return membersOverride;
    if (!selectedSpace) return '[]';
    try {
      return JSON.stringify(selectedSpace.members ?? [], null, 2);
    } catch { return '[]'; }
  });

  $effect(() => {
    const _space = selectedSpace;
    membersOverride = null;
    membersDirty = false;
    membersError = null;
  });

  function handleMembersChange(val: string) {
    membersOverride = val;
    membersDirty = true;
    membersError = null;
  }

  async function saveMembers() {
    if (!currentUser || !selectedArbiterDid || !selectedSpace || !selectedSpaceKey) return;

    const activeVal = membersOverride ?? membersDisplayValue;
    let parsed: Array<{ did: string; access: Record<string, unknown> }>;
    try {
      parsed = JSON.parse(activeVal);
      if (!Array.isArray(parsed)) {
        throw new Error('Members must be a JSON array');
      }
      for (const m of parsed) {
        if (typeof m.did !== 'string') throw new Error('Each member must have a "did" string field');
        if (typeof m.access !== 'object' || m.access === null) throw new Error('Each member must have an "access" object field');
      }
    } catch (e) {
      app.notifications.add('error', `Invalid JSON: ${e}`);
      return;
    }

    const spaceType = selectedSpace.spaceType || 'town.muni.arbiter.config.space';

    // Diff against existing members: remove old, set new
    const existingDids = new Set(selectedSpace.members.map((m) => m.did));
    const newDids = new Set(parsed.map((m) => m.did));

    for (const existingDid of existingDids) {
      if (!newDids.has(existingDid)) {
        const result = await app.runOp(
          selectedArbiterDid,
          currentUser.did,
          (log) => app.simulator.removeSpaceMember(
            selectedArbiterDid!,
            currentUser!.did,
            { spaceKey: selectedSpaceKey, spaceType, memberDid: existingDid },
            log,
          ),
        );
        if (result.status === 'error') {
          app.notifications.add('error', `Failed to remove ${existingDid}: ${result.error}`);
        }
      }
    }

    let hasError = false;
    for (const m of parsed) {
      const result = await app.runOp(
        selectedArbiterDid,
        currentUser.did,
        (log) => app.simulator.setSpaceMemberAccess(
          selectedArbiterDid!,
          currentUser!.did,
          { spaceKey: selectedSpaceKey, spaceType, memberDid: m.did, access: m.access },
          log,
        ),
      );
      if (result.status === 'error') {
        hasError = true;
        app.notifications.add('error', `Failed to set ${m.did}: ${result.error}`);
      }
    }

    if (!hasError) {
      app.notifications.add('success', 'Members saved');
      membersOverride = null;
      membersDirty = false;
    }
  }

  async function clearMembers() {
    if (!currentUser || !selectedArbiterDid || !selectedSpace || !selectedSpaceKey) return;
    if (!confirm('Remove all members from this space?')) return;

    const spaceType = selectedSpace.spaceType || 'town.muni.arbiter.config.space';
    for (const m of selectedSpace.members) {
      await app.runOp(
        selectedArbiterDid,
        currentUser.did,
        (log) => app.simulator.removeSpaceMember(
          selectedArbiterDid!,
          currentUser!.did,
          { spaceKey: selectedSpaceKey, spaceType, memberDid: m.did },
          log,
        ),
      );
    }
    app.notifications.add('success', 'All members cleared');
  }
</script>

{#if selectedSpace}
  <aside class="advanced-panel">
    <div class="panel-header">
      <div class="panel-title">
        <span class="space-icon">{selectedSpace.key === '$admin' ? '👑' : '📁'}</span>
        <h3>{selectedSpace.key}</h3>
        <span class="space-arbiter mono">{selectedArbiterDid}</span>
      </div>
      <button
        class="btn btn-sm close-btn"
        onclick={() => app.selectArbiter(app.selectedArbiterDid!)}
        title="Close"
      >
        ×
      </button>
    </div>

    <!-- Space Config JSON Editor -->
    <section class="panel-section">
      <div class="section-header">
        <h4>Space Config</h4>
        <span class="member-label">{selectedSpace.spaceType ?? ''}</span>
      </div>
      <div class="editor-wrapper">
        <MonacoEditor
          value={configDisplayValue}
          language="json"
          onchange={handleConfigChange}
        />
      </div>
      {#if configError}
        <p class="error-hint">{configError}</p>
      {/if}
      <div class="action-row">
        <button
          class="btn btn-primary btn-sm"
          onclick={saveConfig}
          disabled={!configDirty}
        >
          Save Config
        </button>
      </div>
    </section>

    <!-- Members JSON Editor (direct members) -->
    <section class="panel-section">
      <div class="section-header">
        <h4>Direct Members</h4>
        <span class="member-count">{selectedSpace.members.length}</span>
      </div>
      <div class="editor-wrapper">
        <MonacoEditor
          value={membersDisplayValue}
          language="json"
          onchange={handleMembersChange}
        />
      </div>
      {#if membersError}
        <p class="error-hint">{membersError}</p>
      {/if}
      <div class="action-row">
        <button
          class="btn btn-primary btn-sm"
          onclick={saveMembers}
          disabled={!membersDirty}
        >
          Save Members
        </button>
        <button class="btn btn-sm btn-danger-soft" onclick={clearMembers}>
          Clear All
        </button>
      </div>
    </section>

    <!-- Resolved Members (read-only reference) -->
    <section class="panel-section">
      <h4>Resolved Members</h4>
      {#if app.resolvedMembers}
        <div class="resolved-list">
          {#each app.resolvedMembers as m}
            <div class="resolved-row">
              <span class="mono truncate">{m.did}</span>
              <span class="access-badge mono" title={JSON.stringify(m.access)}>{JSON.stringify(m.access)}</span>
            </div>
          {/each}
        </div>
        {#if app.resolvedMissing && app.resolvedMissing.length > 0}
          <div class="missing-note">
            <span class="info-icon" title="The remote arbiter could not provide its member list for this space.">ⓘ</span>
            <span class="mono">{app.resolvedMissing.length} unresolved remote space{app.resolvedMissing.length !== 1 ? 's' : ''}</span>
          </div>
        {/if}
      {:else if app.resolvedError}
        <p class="error-hint">{app.resolvedError}</p>
      {:else}
        <p class="empty-hint">Select a space to see computed members</p>
      {/if}
    </section>

    <!-- Policy Check Log -->
    {#if app.lastPolicyLog}
      <section class="panel-section">
        <h4>Policy Check Log</h4>
        <div class="policy-log">
          {#each app.lastPolicyLog.steps.slice(-10) as step}
            <div class="log-line mono">{step}</div>
          {/each}
          <div class="log-result">
            <span class="log-label">Result:</span>
            <span class="mono" class:allowed={app.lastPolicyLog.allowed} class:denied={!app.lastPolicyLog.allowed}>
              {JSON.stringify(app.lastPolicyLog.result)}
            </span>
          </div>
        </div>
      </section>
    {/if}

    <section class="panel-section bottom-section">
      <div class="action-row">
        <button class="btn btn-sm" onclick={() => app.selectArbiter(selectedArbiterDid!)}>
          ← Back to Spaces
        </button>
      </div>
    </section>
  </aside>
{/if}

<style>
  .advanced-panel { width: 400px; flex-shrink: 0; background: var(--bg-surface); border-left: 1px solid var(--border); overflow-y: auto; display: flex; flex-direction: column; }
  .panel-header { display: flex; align-items: center; justify-content: space-between; padding: 14px 16px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .panel-title { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .space-icon { font-size: 1rem; flex-shrink: 0; }
  .panel-title h3 { font-weight: 600; font-size: 1rem; }
  .space-arbiter { font-size: 0.714rem; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .close-btn { flex-shrink: 0; }
  .panel-section { padding: 14px 16px; border-bottom: 1px solid var(--border-light); }
  .panel-section h4 { font-size: 0.857rem; font-weight: 600; color: var(--text-secondary); margin-bottom: 8px; text-transform: uppercase; letter-spacing: 0.04em; }
  .section-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px; }
  .section-header h4 { margin-bottom: 0; }
  .member-label { font-size: 0.714rem; padding: 1px 7px; border-radius: var(--radius-xs); background: var(--border); color: var(--text-muted); font-weight: 500; }
  .member-count { font-size: 0.714rem; padding: 1px 7px; border-radius: var(--radius-xs); background: var(--border); color: var(--text-muted); font-weight: 600; }
  .editor-wrapper { height: 160px; border: 1px solid var(--border); border-radius: var(--radius-sm); overflow: hidden; }
  .action-row { display: flex; gap: 6px; margin-top: 8px; }
  .error-hint { color: oklch(0.5 0.15 20); font-size: 0.786rem; margin-top: 4px; }
  .empty-hint { color: var(--text-muted); font-size: 0.786rem; font-style: italic; margin-top: 4px; }
  .btn-danger-soft { color: oklch(0.45 0.15 20); border-color: oklch(0.82 0.04 20); }
  .btn-danger-soft:hover { background: oklch(0.95 0.04 20); border-color: oklch(0.7 0.08 20); }
  .policy-log { display: flex; flex-direction: column; gap: 2px; max-height: 200px; overflow-y: auto; }
  .log-line { font-size: 0.714rem; padding: 2px 6px; border-radius: var(--radius-xs); background: var(--bg-base); color: var(--text-secondary); }
  .log-result { display: flex; align-items: center; gap: 6px; margin-top: 4px; padding: 4px 8px; border-radius: var(--radius-xs); background: var(--accent-subtle); font-size: 0.786rem; }
  .log-label { font-weight: 500; color: var(--text-secondary); }
  .allowed { color: oklch(0.4 0.12 145); font-weight: 600; }
  .denied { color: oklch(0.45 0.15 20); font-weight: 600; }
  .bottom-section { border-bottom: none; }

  .resolved-list { display: flex; flex-direction: column; gap: 2px; }
  .resolved-row { display: flex; align-items: center; gap: 8px; padding: 4px 8px; border-radius: var(--radius-xs); font-size: 0.786rem; }
  .resolved-row:hover { background: var(--accent-subtle); }
  .resolved-row .truncate { flex: 1; min-width: 0; }
  .access-badge { font-size: 0.643rem; padding: 2px 6px; border-radius: var(--radius-xs); background: var(--bg-base); border: 1px solid var(--border-light); color: var(--text-secondary); max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: help; }
  .missing-note { display: flex; align-items: center; gap: 6px; margin-top: 6px; padding: 4px 8px; font-size: 0.714rem; color: oklch(0.5 0.12 30); background: oklch(0.5 0.12 30 / 0.06); border-radius: var(--radius-xs); }
  .info-icon { cursor: help; }
</style>
