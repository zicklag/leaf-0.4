<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import { accessLabel, accessLevel, accessColor, shortDid, memberTypeLabel } from '../lib/utils';
  import { ALL_ACCESSES } from '../lib/types';
  import type { Access } from '../lib/types';

  let { selectedSpace, selectedSpaceMembers } = $derived(app);
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
      <div class="config-row">
        <span class="config-label">Public Members</span>
        <span
          class="config-value"
          class:active={selectedSpace.config.publicMembers}
        >
          {selectedSpace.config.publicMembers ? 'Yes' : 'No'}
        </span>
      </div>
      <div class="config-row">
        <span class="config-label">Public Records</span>
        <span
          class="config-value"
          class:active={selectedSpace.config.publicRecords}
        >
          {selectedSpace.config.publicRecords ? 'Yes' : 'No'}
        </span>
      </div>
    </section>

    <!-- Resolved Members -->
    <section class="panel-section">
      <h4>Resolved Members</h4>
      {#if selectedSpaceMembers}
        {#if selectedSpaceMembers.resolved.length === 0}
          <p class="empty-hint">No resolved members</p>
        {:else}
          <div class="member-list">
            {#each selectedSpaceMembers.resolved as member}
              <div class="member-entry">
                <span class="member-name mono truncate">
                  {member.memberType === 'MemberRemoteSpace' ? '🌐' : '👤'}
                  {shortDid(member.value)}
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

        {#if selectedSpaceMembers.missing && selectedSpaceMembers.missing.length > 0}
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

  .config-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 0;
  }

  .config-label {
    font-size: 0.857rem;
  }

  .config-value {
    font-size: 0.786rem;
    font-weight: 500;
    color: var(--text-muted);
  }

  .config-value.active {
    color: var(--accent-text);
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
