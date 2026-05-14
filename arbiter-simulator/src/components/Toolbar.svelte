<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
  import { userInitial } from '../lib/utils';

  let { currentUser } = $derived(app);

  function shareLink() {
    const url = window.location.href;
    navigator.clipboard.writeText(url).then(() => {
      app.notifications.add('success', 'Link copied to clipboard!');
    }).catch(() => {
      app.notifications.add('error', 'Failed to copy link');
    });
  }

  function copyCleanLink() {
    const url = window.location.origin + window.location.pathname;
    navigator.clipboard.writeText(url).then(() => {
      app.notifications.add('success', 'Copied link to Arbiter Simulator.');
    }).catch(() => {
      app.notifications.add('error', 'Failed to copy link');
    });
  }
</script>

<header class="toolbar">
  <div class="brand" onclick={copyCleanLink} title="Copy clean link (without config)">
    <span class="logo">⚖️</span>
    <span class="title">Arbiter Simulator</span>
  </div>

  <div class="actions">
    {#if currentUser}
      <div class="acting-as">
        <span class="acting-as-label">acting as</span>
        <span class="user-avatar">{userInitial(currentUser.label)}</span>
        <span class="user-name">{currentUser.label}</span>
      </div>
    {/if}
    <button class="btn btn-sm" onclick={() => app.resetAll()}>
      ↺ Reset
    </button>
    <button class="btn btn-sm" onclick={shareLink}>
      ↗ Share
    </button>
    <button class="btn btn-sm theme-toggle" onclick={() => app.toggleTheme()} title={app.darkTheme ? 'Switch to light mode' : 'Switch to dark mode'}>
      {app.darkTheme ? '☀️' : '🌙'}
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
    cursor: pointer;
    user-select: none;
  }

  .logo {
    font-size: 1.143rem;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .acting-as {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 10px 2px 8px;
    border-radius: var(--radius-sm);
    background: var(--accent-subtle);
    border: 1px solid oklch(0.58 0.18 65 / 0.15);
  }

  .acting-as-label {
    font-size: 0.714rem;
    color: var(--text-muted);
  }

  .user-avatar {
    width: 20px;
    height: 20px;
    border-radius: var(--radius-xs);
    background: var(--accent);
    color: white;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.643rem;
    font-weight: 600;
  }

  .user-name {
    font-size: 0.857rem;
    font-weight: 500;
    color: var(--accent-text);
  }
</style>
