<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';

  let { items } = $derived(app.notifications);
</script>

{#if items.length > 0}
  <div class="notifications" role="alert" aria-live="polite">
    {#each items as notif}
      <div
        class="notification"
        class:success={notif.type === 'success'}
        class:error={notif.type === 'error'}
        class:info={notif.type === 'info'}
      >
        <span class="notif-icon">
          {notif.type === 'success' ? '✓' : notif.type === 'error' ? '✗' : 'ℹ'}
        </span>
        <span class="notif-message">{notif.message}</span>
        <button
          class="notif-dismiss"
          onclick={() => app.notifications.dismiss(notif.id)}
        >
          ×
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .notifications {
    position: fixed;
    top: 56px;
    right: 16px;
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 360px;
  }

  .notification {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-radius: var(--radius-md);
    background: var(--bg-raised);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-md);
    animation: slideIn 200ms var(--ease-out);
    font-size: 0.857rem;
  }

  .notification.success {
    border-left: 3px solid oklch(0.55 0.15 145);
  }

  .notification.error {
    border-left: 3px solid oklch(0.5 0.15 20);
  }

  .notification.info {
    border-left: 3px solid var(--accent);
  }

  @keyframes slideIn {
    from {
      opacity: 0;
      transform: translateY(-8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .notif-icon {
    font-weight: 700;
    flex-shrink: 0;
  }

  .notification.success .notif-icon {
    color: oklch(0.55 0.15 145);
  }

  .notification.error .notif-icon {
    color: oklch(0.5 0.15 20);
  }

  .notification.info .notif-icon {
    color: var(--accent);
  }

  .notif-message {
    flex: 1;
  }

  .notif-dismiss {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-muted);
    font-size: 1rem;
    padding: 0 2px;
    line-height: 1;
  }

  .notif-dismiss:hover {
    color: var(--text-primary);
  }
</style>
