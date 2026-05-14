<script lang="ts">
  import UserList from './UserList.svelte';
  import ActionPanel from './ActionPanel.svelte';
</script>

<aside class="sidebar">
  <UserList />
  <ActionPanel />
</aside>

<style>
  .sidebar {
    width: 280px;
    flex-shrink: 0;
    background: var(--bg-surface);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }
</style>
