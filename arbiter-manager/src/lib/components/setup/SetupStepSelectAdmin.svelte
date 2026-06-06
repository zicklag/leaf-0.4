<script lang="ts">
  import { Button } from '@foxui/core';
  import { setupState } from '$lib/setupState.svelte';
  import { AtprotoHandlePopup, type Profile } from '@foxui/all';
  import { PUBLIC_ARBITER_URL } from '$env/static/public';
  import { auth } from '$lib/auth.svelte';
  import * as town from '$lib/lexicons/town';
  import { isAtprotoDid } from '@atproto/oauth-client-browser';
  import { xrpc } from '@atproto/lex';

  let selectedAdmin: Profile | undefined = $state(undefined);

  function goBack() {
    setupState.step = 'email-code';
  }

  async function finishSetup() {
    if (!selectedAdmin) {
      setupState.error = 'Please resolve an admin DID first';
      return;
    }

    setupState.loading = true;
    setupState.error = undefined;

    try {
      if (!auth.client) throw new Error('Not logged in');
      if (!selectedAdmin.did) throw new Error('You must select an admin.');
      if (!isAtprotoDid(auth.did)) throw new Error('Not logged in with valid DID');
      if (!setupState.appPassword) throw new Error('Must provide AppPassword');

      // Create the new arbiter!
      await xrpc(PUBLIC_ARBITER_URL, town.muni.arbiter.createAppPasswordArbiter, {
        body: {
          arbiterDid: auth.did,
          appPassword: setupState.appPassword,
          config: {
            $type: 'town.muni.arbiter.server.v1.config',
            policy: `
              package arbiter
              import rego.v1
              arbiter_xrpc_nsids := {
               	"town.muni.arbiter.getArbiterConfig",
               	"town.muni.arbiter.setArbiterConfig",
               	"town.muni.arbiter.deleteArbiter",
               	"town.muni.arbiter.createSpace",
               	"town.muni.arbiter.getSpaceConfig",
               	"town.muni.arbiter.setSpaceConfig",
               	"town.muni.arbiter.deleteSpace",
               	"town.muni.arbiter.listSpaces",
               	"town.muni.arbiter.getSpaceMembers",
               	"town.muni.arbiter.setSpaceMemberAccess",
               	"town.muni.arbiter.removeSpaceMember",
              }
              response := {"status": 403, "body": {"error": "ErrPermissionDenied"}} if not allow
              response := xrpc_local(input.operation.method, input.operation.nsid, input.operation.params) if {
               	allow
               	input.operation.nsid in arbiter_xrpc_nsids
              }
              default allow := false
              allow if input.caller.did == data.arbiter.did
              allow if input.caller.did == "${selectedAdmin.did}"
          `,
          } as any,
        },
        headers: {
          'atproto-proxy': `${auth.did}#arbiter`,
        },
      });

      setupState.step = 'complete';
      setupState.error = undefined;
      setupState.loading = false;
    } catch (e) {
      setupState.error = `Failed: ${e instanceof Error ? e.message : String(e)}`;
      setupState.loading = false;
    }
  }
</script>

<div class="max-w-lg mx-auto px-6 py-12 space-y-6">
  <div class="space-y-2">
    <h2 class="text-xl font-semibold text-base-900 dark:text-base-50">Select an Admin</h2>
    <p class="text-sm text-base-600 dark:text-base-400">
      Choose someone to have <strong>Owner</strong> access to this community's arbiter. This person will
      be able to manage spaces, members, and policies on behalf of the community account.
    </p>
  </div>

  <div class="space-y-4">
    <AtprotoHandlePopup onselected={(actor) => (selectedAdmin = actor)} />

    {#if setupState.error}
      <p class="text-sm text-red-500">{setupState.error}</p>
    {/if}
  </div>

  <div class="flex justify-between pt-2">
    <Button variant="ghost" onclick={goBack} disabled={setupState.loading}>Back</Button>
    <Button onclick={finishSetup} disabled={setupState.loading || !selectedAdmin}>
      {setupState.loading ? 'Finalizing…' : 'Complete Setup'}
    </Button>
  </div>
</div>
