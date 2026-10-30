<script lang="ts">
  import { onMount } from 'svelte';
  import { api, onSession, type Status } from '#lib/api.ts';
  import Onboarding from '#lib/components/Onboarding.svelte';
  import LockScreen from '#lib/components/LockScreen.svelte';
  import Workspace from '#lib/components/Workspace.svelte';

  let status = $state<Status | null>(null);
  // Onboarding keeps control until the user finishes it, even though the
  // vault is already unlocked after its first step.
  let onboarding = $state(false);

  onMount(() => {
    api.status().then((s) => {
      status = s;
      onboarding = s.state === 'none';
    });
    const off = onSession((s) => (status = s));
    return () => off.then((f) => f());
  });
</script>

{#if !status}
  <div class="h-full" data-tauri-drag-region></div>
{:else if onboarding}
  <Onboarding {status} ondone={async () => ((status = await api.status()), (onboarding = false))} />
{:else if status.state === 'unlocked'}
  <Workspace {status} />
{:else}
  <LockScreen {status} />
{/if}
