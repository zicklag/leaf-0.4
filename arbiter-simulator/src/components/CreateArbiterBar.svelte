<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';

  let newArbiterDid = $state('');
  let inputEl: HTMLInputElement | undefined = $state();

  let { currentUser } = $derived(app);

  async function handleCreateArbiter(e: Event) {
    e.preventDefault();
    if (!currentUser || !newArbiterDid.trim()) return;

    try {
      const arbiterDid = newArbiterDid.trim();
      app.simulator.createArbiter(arbiterDid, currentUser.did);
      newArbiterDid = '';
      app.refreshState();
      app.notifications.add('success', `Arbiter "${arbiterDid}" created`);
      app.selectSpace(arbiterDid, '$admin');
      setTimeout(() => inputEl?.focus(), 50);
    } catch (err) {
      app.notifications.add('error', `Failed to create arbiter: ${err}`);
    }
  }
</script>

<form class="create-arbiter-bar" onsubmit={handleCreateArbiter}>
  <span class="bar-label">Create Arbiter</span>
  <input
    type="text"
    placeholder={app.generateArbiterDid()}
    bind:value={newArbiterDid}
    bind:this={inputEl}
    disabled={!currentUser}
  />
  <button class="btn btn-primary btn-sm" type="submit" disabled={!currentUser}>
    Create
  </button>
</form>

<style>
  .create-arbiter-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .bar-label {
    font-size: 0.857rem;
    font-weight: 500;
    color: var(--text-secondary);
    white-space: nowrap;
  }

  .create-arbiter-bar input {
    flex: 1;
    max-width: 320px;
  }
</style>
