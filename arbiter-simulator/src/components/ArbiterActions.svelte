<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import { buildMessage } from '../lib/utils';

  let { currentUser, selectedArbiter } = $derived(app);

  let newSpaceKey = $state('');

  async function handleCreateSpace(e: Event) {
    e.preventDefault();
    const key = newSpaceKey.trim();
    if (!currentUser || !selectedArbiter || !key) return;
    newSpaceKey = '';
    const result = await app.dispatch(
      buildMessage(currentUser.did, selectedArbiter.did, key, {
        type: 'createSpace',
      }),
    );
    const respond = result.find((r) => r.effectType === 'respond');
    if (respond?.ok) {
      app.notifications.add('success', `Space "${key}" created`);
    } else {
      app.notifications.add('error', respond?.error ?? 'Failed to create space');
    }
  }

  async function handleDeleteArbiter() {
    if (!currentUser || !selectedArbiter) return;
    const result = await app.dispatch(
      buildMessage(currentUser.did, selectedArbiter.did, '$admin', {
        type: 'deleteArbiter',
      }),
    );
    const respond = result.find((r) => r.effectType === 'respond');
    if (respond?.ok) {
      app.notifications.add('success', 'Arbiter deleted');
      app.selectArbiter(null);
    } else {
      app.notifications.add('error', respond?.error ?? 'Failed to delete arbiter');
    }
  }
</script>

<section class="arbiter-actions">
  <div class="section-header">
    <h3>Arbiter</h3>
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

    <button
      class="btn btn-danger btn-sm delete-btn"
      onclick={handleDeleteArbiter}
    >
      Delete Arbiter
    </button>
  {/if}
</section>

<style>
  .arbiter-actions {
    padding: 16px;
    border-bottom: 1px solid var(--border-light);
    flex-shrink: 0;
  }

  .section-header {
    margin-bottom: 12px;
  }

  .section-header h3 {
    font-weight: 600;
  }

  .empty-hint {
    color: var(--text-muted);
    font-size: 0.857rem;
    font-style: italic;
  }

  .context-label {
    font-size: 0.714rem;
    font-weight: 600;
    color: var(--accent-text);
    background: var(--accent-subtle);
    padding: 4px 8px;
    border-radius: var(--radius-xs);
    margin-bottom: 12px;
  }

  .action-form {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border-light);
  }

  .action-form label {
    font-size: 0.857rem;
    font-weight: 500;
    color: var(--text-secondary);
  }

  .input-row {
    display: flex;
    gap: 6px;
  }

  .input-row input {
    flex: 1;
  }

  .delete-btn {
    width: 100%;
  }
</style>
