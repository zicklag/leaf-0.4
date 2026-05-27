<script lang="ts">
  import { Sheet, Input, Button } from '@foxui/core';

  let { open }: { open: boolean } = $props();

  let didInput = $state('');
  let error = $state<string | null>(null);
  let loading = $state(false);

  async function lookup() {
    const value = didInput.trim();
    if (!value) {
      error = 'Please enter a DID or handle';
      return;
    }

    loading = true;
    error = null;

    try {
      // Resolve handle → DID if needed
      let did = value;
      if (!value.startsWith('did:')) {
        // Could be a handle — resolve it
        const res = await fetch(`https://plc.directory/${value}`);
        if (res.ok) {
          const doc = await res.json();
          did = doc.id || doc.did || value;
        } else {
          // Try as a handle via atproto handle resolution
          const handleRes = await fetch(
            `https://resolve.handle.net/.well-known/atproto-did?handle=${encodeURIComponent(value)}`,
          );
          if (handleRes.ok) {
            did = await handleRes.text();
          }
        }
      }

      if (!did.startsWith('did:')) {
        error = `Could not resolve "${value}" to a DID`;
        return;
      }

      // Add to managed communities
      const { addManagedCommunity, selectedArbiterDid } = await import('$lib/store.svelte');
      addManagedCommunity(did, value);
      selectedArbiterDid.set(did);
      open = false;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
</script>

<Sheet
  bind:open
  title="Lookup Community"
  description="Enter a DID or handle to start managing a community."
>
  <div class="flex flex-col gap-4 py-2">
    <div class="flex flex-col gap-2">
      <label for="did-input" class="text-sm font-medium text-base-700 dark:text-base-300">
        DID or Handle
      </label>
      <Input
        id="did-input"
        bind:value={didInput}
        placeholder="did:plc:abc123 or handle.example.com"
      />
      {#if error}
        <p class="text-sm text-red-500">{error}</p>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <Button variant="secondary" onclick={() => (open = false)}>Cancel</Button>
    <Button onclick={lookup} disabled={loading}>
      {loading ? 'Looking up…' : 'Lookup &amp; Add'}
    </Button>
  {/snippet}
</Sheet>
