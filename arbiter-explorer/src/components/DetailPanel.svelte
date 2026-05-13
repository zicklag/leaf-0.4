<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import { accessLabel, accessLevel, accessColor, shortDid, memberTypeLabel, buildMessage } from '../lib/utils';
  import { ALL_ACCESSES } from '../lib/types';
  import type { Access } from '../lib/types';

  let { selectedSpace, selectedSpaceMembers, selectedSpaceError, currentUser, selectedArbiterDid } = $derived(app);

  let sortedMembers = $derived(
    selectedSpaceMembers?.resolved
      ? [...selectedSpaceMembers.resolved].sort((a, b) => {
          const levelDiff = accessLevel(b.access) - accessLevel(a.access);
          if (levelDiff !== 0) return levelDiff;
          return a.value.localeCompare(b.value);
        })
      : [],
  );

  let currentUserAccess = $derived(
    currentUser && selectedSpaceMembers?.resolved
      ? selectedSpaceMembers.resolved.find(
          (m) => m.memberType === 'User' && m.value === currentUser.did,
        )?.access ?? null
      : null,
  );

  async function togglePublicMembers() {
    if (!currentUser || !selectedSpace || !selectedArbiterDid) return;
    const effects = await app.dispatch(
      buildMessage(currentUser.did, selectedArbiterDid, selectedSpace.key, {
        type: 'configureSpace',
        public_records: selectedSpace.config.publicRecords,
        public_members: !selectedSpace.config.publicMembers,
      }),
    );
    const respond = effects.find((e) => e.effectType === 'respond');
    if (respond && !respond.ok) {
      const who = currentUser?.label ?? currentUser?.did ?? 'unknown';
      app.notifications.add('error', `User "${who}": ${respond.error || 'Permission denied'}`);
    }
  }

  async function togglePublicRecords() {
    if (!currentUser || !selectedSpace || !selectedArbiterDid) return;
    const effects = await app.dispatch(
      buildMessage(currentUser.did, selectedArbiterDid, selectedSpace.key, {
        type: 'configureSpace',
        public_records: !selectedSpace.config.publicRecords,
        public_members: selectedSpace.config.publicMembers,
      }),
    );
    const respond = effects.find((e) => e.effectType === 'respond');
    if (respond && !respond.ok) {
      const who = currentUser?.label ?? currentUser?.did ?? 'unknown';
      app.notifications.add('error', `User "${who}": ${respond.error || 'Permission denied'}`);
    }
  }
</script>

{#if selectedSpace}
  <aside class="detail-panel">
    <div class="panel-header">
      <h3>
        {selectedSpace.key === '$admin' ? '👑' : '📁'}
        {selectedSpace.key}
      </h3>
      <button
        class="btn btn-sm"
        onclick={() => app.selectArbiter(app.selectedArbiterDid!)}
      >
        ×
      </button>
    </div>

    <!-- Space config -->
    <section class="panel-section">
      <h4>Configuration</h4>
      <button
        class="config-toggle"
        class:active={selectedSpace.config.publicMembers}
        onclick={togglePublicMembers}
      >
        <span class="config-label">Public Members</span>
        <span class="toggle-indicator">
          {selectedSpace.config.publicMembers ? 'On' : 'Off'}
        </span>
      </button>
      <button
        class="config-toggle"
        class:active={selectedSpace.config.publicRecords}
        onclick={togglePublicRecords}
      >
        <span class="config-label">Public Records</span>
        <span class="toggle-indicator">
          {selectedSpace.config.publicRecords ? 'On' : 'Off'}
        </span>
      </button>
    </section>

    <!-- Resolved Members -->
    <section class="panel-section">
      <h4>Resolved Members</h4>

      {#if selectedSpaceMembers}
        <!-- Resolving-as badge -->
        {#if currentUser}
          <div class="resolving-badge">
            <span class="badge-label">Resolving as</span>
            <span class="badge-user mono">
              👤 {currentUser.label ?? shortDid(currentUser.did)}
              {#if currentUserAccess}
                <span class="badge-access" style="color: {accessColor(currentUserAccess)}">
                  {accessLabel(currentUserAccess)}
                </span>
              {:else}
                <span class="badge-access no-access">No access</span>
              {/if}
            </span>
          </div>
        {/if}

        {#if sortedMembers.length === 0}
          <p class="empty-hint">No resolved members</p>
        {:else}
          <div class="member-list">
            {#each sortedMembers as member}
              <div class="member-entry">
                <span class="member-name mono truncate">
                  👤 {shortDid(member.value)}
                </span>
                <div class="access-bar">
                  <div
                    class="access-fill"
                    style="width: {((accessLevel(member.access) + 1) / ALL_ACCESSES.length) * 100}%; background: {accessColor(member.access)}"
                  ></div>
                </div>
                <span class="access-label" style="color: {accessColor(member.access)}">
                  {accessLabel(member.access)}
                </span>
              </div>
            {/each}
          </div>
        {/if}

        {#if selectedSpaceMembers.missing.length > 0}
          <div class="missing-section">
            <h5>Unresolved Spaces</h5>
            {#each selectedSpaceMembers.missing as ms}
              <div class="member-entry missing">
                <span class="member-name mono truncate">❓ {ms.arbiterDid}:{ms.spaceKey}</span>
                <span class="access-label muted">{accessLabel(ms.access)}</span>
              </div>
            {/each}
          </div>
        {/if}
      {:else if selectedSpaceError}
        <p class="error-hint">{selectedSpaceError}</p>
      {:else}
        <p class="empty-hint">Select a space to see computed members</p>
      {/if}
    </section>

    <!-- Access Level Legend -->
    <section class="panel-section">
      <h4>Access Levels</h4>
      <div class="legend">
        {#each ALL_ACCESSES as access}
          <div class="legend-item">
            <div
              class="legend-swatch"
              style="background: {accessColor(access)}"
            ></div>
            <span class="legend-label" style="color: {accessColor(access)}">
              {accessLabel(access)}
            </span>
          </div>
        {/each}
      </div>
    </section>
  </aside>
{/if}

<style>
  .detail-panel {
    width: 320px;
    flex-shrink: 0;
    background: var(--bg-surface);
    border-left: 1px solid var(--border);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }

  .panel-header h3 {
    font-weight: 600;
  }

  .panel-section {
    padding: 14px 16px;
    border-bottom: 1px solid var(--border-light);
  }

  .panel-section h4 {
    font-size: 0.857rem;
    font-weight: 600;
    color: var(--text-secondary);
    margin-bottom: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .panel-section h5 {
    font-size: 0.786rem;
    font-weight: 600;
    color: var(--text-muted);
    margin: 8px 0 6px;
  }

  .empty-hint {
    color: var(--text-muted);
    font-size: 0.857rem;
    font-style: italic;
  }

  .error-hint {
    color: var(--error, #c0392b);
    font-size: 0.857rem;
    line-height: 1.4;
  }

  .config-toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-xs);
    background: var(--bg-base);
    cursor: pointer;
    transition: all 150ms var(--ease-out);
    margin-bottom: 4px;
  }

  .config-toggle:hover {
    border-color: var(--accent);
    background: var(--accent-subtle);
  }

  .config-toggle.active {
    border-color: var(--accent);
    background: oklch(0.58 0.18 65 / 0.08);
  }

  .config-label {
    font-size: 0.857rem;
    font-weight: 500;
  }

  .toggle-indicator {
    font-size: 0.786rem;
    font-weight: 600;
    color: var(--text-muted);
    transition: color 150ms var(--ease-out);
  }

  .config-toggle.active .toggle-indicator {
    color: var(--accent-text);
  }

  .resolving-badge {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 8px 10px;
    margin-bottom: 10px;
    background: var(--accent-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-xs);
  }

  .badge-label {
    font-size: 0.714rem;
    font-weight: 500;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .badge-user {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.857rem;
    font-weight: 600;
  }

  .badge-access {
    font-size: 0.786rem;
    font-weight: 600;
  }

  .badge-access.no-access {
    color: var(--text-muted);
    font-weight: 400;
  }

  .member-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .member-entry {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
  }

  .member-entry.missing {
    opacity: 0.5;
  }

  .member-name {
    flex: 1;
    min-width: 0;
    font-size: 0.786rem;
  }

  .access-bar {
    width: 60px;
    height: 6px;
    background: var(--border);
    border-radius: 3px;
    overflow: hidden;
    flex-shrink: 0;
  }

  .access-fill {
    height: 100%;
    border-radius: 3px;
    transition: width 300ms var(--ease-out);
  }

  .access-label {
    font-size: 0.714rem;
    font-weight: 600;
    width: 80px;
    flex-shrink: 0;
  }

  .access-label.muted {
    color: var(--text-muted);
    font-weight: 400;
  }

  .legend {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .legend-swatch {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .legend-label {
    font-size: 0.786rem;
    font-weight: 500;
  }
</style>
