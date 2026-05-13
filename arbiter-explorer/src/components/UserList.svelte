<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import { userInitial } from '../lib/utils';

  let { users, currentUserId } = $derived(app);
  let newUserName = $state('');
  let showAdd = $state(false);

  function handleAdd(e: Event) {
    e.preventDefault();
    const name = newUserName.trim();
    if (!name) return;
    app.addUser(name);
    newUserName = '';
    showAdd = false;
  }
</script>

<section class="user-list">
  <div class="section-header">
    <h3>Users</h3>
    <button class="btn btn-sm" onclick={() => (showAdd = !showAdd)}>
      {showAdd ? '−' : '+'}
    </button>
  </div>

  {#if showAdd}
    <form class="add-user-form" onsubmit={handleAdd}>
      <input
        type="text"
        placeholder="User name (e.g. Alice)"
        bind:value={newUserName}
      />
      <button class="btn btn-primary btn-sm" type="submit">Add</button>
    </form>
  {/if}

  {#if users.length === 0}
    <p class="empty-hint">Create a user account to get started.</p>
  {:else}
    <ul class="user-items">
      {#each users as user}
        <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
        <li
          class="user-item"
          class:active={user.did === currentUserId}
          onclick={() => app.selectUser(user.did)}
          onkeydown={(e) => e.key === 'Enter' && app.selectUser(user.did)}
          role="button"
          tabindex="0"
        >
          <div class="avatar">{userInitial(user.label)}</div>
          <div class="user-info">
            <span class="user-name">{user.label}</span>
            <span class="user-did mono">{user.did}</span>
          </div>
          <button
            class="btn btn-sm remove-btn"
            onclick="{(e) => { e.stopPropagation(); app.removeUser(user.did); }}"
            title="Remove user"
          >
            ×
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .user-list {
    padding: 16px;
    border-bottom: 1px solid var(--border-light);
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
  }

  .section-header h3 {
    font-weight: 600;
  }

  .add-user-form {
    display: flex;
    gap: 6px;
    margin-bottom: 12px;
  }

  .add-user-form input {
    flex: 1;
  }

  .empty-hint {
    color: var(--text-muted);
    font-size: 0.857rem;
    font-style: italic;
  }

  .user-items {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .user-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: background 150ms var(--ease-out);
  }

  .user-item:hover {
    background: var(--accent-subtle);
  }

  .user-item.active {
    background: var(--accent-subtle);
    border: 1px solid var(--accent);
    padding: 7px 9px;
  }

  .avatar {
    width: 32px;
    height: 32px;
    border-radius: var(--radius-sm);
    background: var(--accent);
    color: white;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.786rem;
    font-weight: 600;
    flex-shrink: 0;
  }

  .user-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .user-name {
    font-weight: 500;
    font-size: 0.929rem;
  }

  .user-did {
    color: var(--text-muted);
    font-size: 0.714rem;
  }

  .remove-btn {
    opacity: 0;
    transition: opacity 150ms var(--ease-out);
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .user-item:hover .remove-btn {
    opacity: 1;
  }
</style>
