<script lang="ts">
  import { AtprotoHandlePopup, type Profile } from '@foxui/all';
  import { managedCommunities } from '$lib/store.svelte';
  import { goto } from '$app/navigation';
  import { auth } from '$lib/auth.svelte';
  import * as app from '$lib/lexicons/app';

  let {
    did: currentDid,
  }: { did: string } = $props();

  let communityProfile = $state<Profile | undefined>();
  let searchValue = $state('');

  // ── Resolve current DID to a profile ──────────────────────────────────
  $effect(() => {
    if (currentDid) resolveProfile();
  });

  async function resolveProfile() {
    try {
      if (auth.client) {
        const resp = await auth.client.xrpc(app.bsky.actor.getProfile, {
          params: { actor: currentDid as any },
        });
        const body = resp.body as any;
        communityProfile = {
          did: currentDid,
          handle: body.handle ?? currentDid,
          displayName: body.displayName ?? '',
          avatar: body.avatar ?? '',
        };
        searchValue = communityProfile.handle;
      }
    } catch {
      communityProfile = {
        did: currentDid,
        handle: currentDid,
        displayName: '',
        avatar: '',
      };
    }
  }

  // ── Handle selection from search ──────────────────────────────────────
  function onSelect(profile: Profile) {
    const newDid = profile.did;
    if (!newDid || !newDid.startsWith('did:')) return;

    managedCommunities.add(newDid, profile.handle ?? newDid);
    goto(`/dashboard/${encodeURIComponent(newDid)}`);
  }
</script>

<div class="flex items-center gap-2 w-full">
  {#if communityProfile}
    {#if communityProfile.avatar}
      <img
        src={communityProfile.avatar}
        alt=""
        class="w-7 h-7 shrink-0 rounded-full object-cover"
      />
    {:else}
      <div
        class="w-7 h-7 shrink-0 rounded-full bg-accent-200 dark:bg-accent-800 flex items-center justify-center text-xs font-semibold text-accent-700 dark:text-accent-300"
      >
        {communityProfile.handle.charAt(0).toUpperCase()}
      </div>
    {/if}
  {:else}
    <div class="w-7 h-7 shrink-0"></div>
  {/if}
  <div class="flex-1 min-w-0">
    <AtprotoHandlePopup
      bind:value={searchValue}
      onselected={onSelect}
    />
  </div>
</div>