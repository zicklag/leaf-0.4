<script lang="ts">
  import { app } from '../lib/simulation-store.svelte';
</script>

<div class="empty-state">
  <div class="empty-illustration">⚖️</div>
  <h2>Welcome to Arbiter Simulator</h2>
  <p class="empty-desc">
    Design and explore authorization structures with the real Arbiter state
    machine running in your browser.
  </p>
  <div class="steps">
    <div class="step">
      <span class="step-num">1</span>
      <div class="step-text">
        <strong>Create a user</strong>
        <span>Add a virtual account in the sidebar</span>
      </div>
    </div>
    <div class="step">
      <span class="step-num">2</span>
      <div class="step-text">
        <strong>Create an arbiter</strong>
        <span>Give it a DID and you're the first owner</span>
      </div>
    </div>
    <div class="step">
      <span class="step-num">3</span>
      <div class="step-text">
        <strong>Add spaces and members</strong>
        <span>Try local spaces, users, and remote delegations</span>
      </div>
    </div>
    <div class="step">
      <span class="step-num">4</span>
      <div class="step-text">
        <strong>See computed access</strong>
        <span>Click a space to view its resolved member list</span>
      </div>
    </div>
  </div>
</div>

<style>
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 48px 32px;
    max-width: 480px;
  }

  .empty-illustration {
    font-size: 3rem;
    margin-bottom: 16px;
  }

  h2 {
    font-size: 1.429rem;
    margin-bottom: 8px;
  }

  .empty-desc {
    color: var(--text-secondary);
    margin-bottom: 32px;
    line-height: 1.6;
  }

  .steps {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
    text-align: left;
  }

  .step {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }

  .step-num {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--accent-subtle);
    color: var(--accent-text);
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 600;
    font-size: 0.857rem;
    flex-shrink: 0;
  }

  .step-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .step-text strong {
    font-size: 0.929rem;
  }

  .step-text span {
    font-size: 0.857rem;
    color: var(--text-secondary);
  }
</style>
