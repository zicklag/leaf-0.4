<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';

  let { currentUser, selectedArbiter } = $derived(app);

  let newSpaceKey = $state('');

  async function handleCreateSpace(e: Event) {
    e.preventDefault();
    const key = newSpaceKey.trim();
    if (!currentUser || !selectedArbiter || !key) return;
    newSpaceKey = '';
    const result = await app.processOperation(
      selectedArbiter.did, currentUser.did, key,
      {
        type: 'CreateSpace',
        spaceType: 'town.muni.arbiter.config.space',
        config: {
          $type: 'town.muni.arbiter.config.space',
          publicRecords: false,
          publicMembers: false,
        },
      },
    );
    if (result.status === 'ok') {
      app.notifications.add('success', `Space "${key}" created`);
    } else {
      app.notifications.add('error', result.status === 'error' ? result.error : 'Failed to create space');
    }
  }

  async function handleDeleteArbiter() {
    if (!currentUser || !selectedArbiter) return;
    const result = await app.processOperation(
      selectedArbiter.did, currentUser.did, '$admin',
      { type: 'DeleteArbiter' },
    );
    if (result.status === 'deleted') {
      app.notifications.add('success', 'Arbiter deleted');
      app.selectArbiter(null);
    } else {
      app.notifications.add('error', result.status === 'error' ? result.error : 'Failed to delete arbiter');
    }
  }
</script>

<section class="arbiter-actions">
  <div class="arbiter-actions-body">
    <div class="section-header">
      <h3>Arbiter</h3>
      {#if selectedArbiter}
        <button
          class="delete-arbiter-btn"
          onclick={handleDeleteArbiter}
          title="Delete arbiter"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="3 6 5 6 21 6"/>
            <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
            <path d="M10 11v6"/>
            <path d="M14 11v6"/>
            <path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>
          </svg>
        </button>
      {/if}
    </div>

    {#if !currentUser}
      <p class="empty-hint">Select a user to manage arbiters.</p>
    {:else if !selectedArbiter}
      <p class="empty-hint">Select an arbiter to manage spaces.</p>
    {:else}
      <div class="context-label mono">
        🏛️ {selectedArbiter.did}
      </div>

      <form class="action-form" onsubmit={handleCreateSpace}>
        <label for="space-key">Create Space</label>
        <div class="input-row">
          <input
            id="space-key"
            type="text"
            placeholder="e.g. team, project, …"
            bind:value={newSpaceKey}
          />
          <button class="btn btn-primary btn-sm" type="submit">Create</button>
        </div>
      </form>
    {/if}
  </div>
</section>

<style>
  .arbiter-actions { padding: 0; border-bottom: 1px solid var(--border-light); flex-shrink: 0; }
  .arbiter-actions-body { padding: 16px; }
  .section-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; }
  .section-header h3 { font-weight: 600; }
  .delete-arbiter-btn { display: flex; align-items: center; justify-content: center; width: 28px; height: 28px; padding: 0; border: 1px solid transparent; border-radius: var(--radius-xs); background: none; cursor: pointer; color: oklch(0.5 0.15 20); transition: all 150ms var(--ease-out); flex-shrink: 0; }
  .delete-arbiter-btn:hover { background: oklch(0.95 0.04 20); border-color: oklch(0.7 0.08 20); color: oklch(0.42 0.18 20); }
  .delete-arbiter-btn:active { transform: scale(0.92); }
  .empty-hint { color: var(--text-muted); font-size: 0.857rem; font-style: italic; }
  .context-label { font-size: 0.714rem; font-weight: 600; color: var(--accent-text); background: var(--accent-subtle); padding: 4px 8px; border-radius: var(--radius-xs); margin-bottom: 12px; }
  .action-form { display: flex; flex-direction: column; gap: 4px; }
  .action-form label { font-size: 0.857rem; font-weight: 500; color: var(--text-secondary); }
  .input-row { display: flex; gap: 6px; }
  .input-row input { flex: 1; }
</style>
