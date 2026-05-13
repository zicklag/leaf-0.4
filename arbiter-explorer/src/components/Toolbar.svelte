<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';

  let { users, currentUser } = $derived(app);
</script>

<header class="toolbar">
  <div class="brand">
    <span class="logo">⚖️</span>
    <span class="title">Arbiter Explorer</span>
  </div>

  <div class="actions">
    {#if users.length > 0}
      <div class="user-select">
        <select
          value={app.currentUserId ?? ''}
          onchange={(e) => app.selectUser((e.target as HTMLSelectElement).value)}
        >
          <option value="" disabled>Select user…</option>
          {#each users as u}
            <option value={u.did}>{u.label} ({u.did})</option>
          {/each}
        </select>
        {#if currentUser}
          <span class="acting-as">
            as <strong>{currentUser.label}</strong>
          </span>
        {/if}
      </div>
    {/if}

    <button class="btn btn-sm" onclick={() => app.resetAll()}>
      ↺ Reset
    </button>
  </div>
</header>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 44px;
    padding: 0 16px;
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    font-size: 0.929rem;
    color: var(--text-primary);
  }

  .logo {
    font-size: 1.143rem;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .user-select {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .user-select select {
    min-width: 160px;
  }

  .acting-as {
    font-size: 0.857rem;
    color: var(--text-secondary);
  }
</style>
