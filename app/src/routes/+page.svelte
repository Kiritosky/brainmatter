<script lang="ts">
  import { onMount } from 'svelte';
  import { api, onSession, type Status } from '#lib/api.ts';
  import { theme } from '#lib/theme.svelte.ts';
  import Onboarding from '#lib/components/Onboarding.svelte';
  import LockScreen from '#lib/components/LockScreen.svelte';
  import QuickCapture from '#lib/components/QuickCapture.svelte';
  import Workspace from '#lib/components/Workspace.svelte';

  // Both windows load this page; `?window=capture` is for the browser mock.
  const isCapture =
    (window as any).__TAURI_INTERNALS__?.metadata?.currentWindow?.label === 'capture' ||
    new URLSearchParams(location.search).get('window') === 'capture';

  let status = $state<Status | null>(null);
  // Onboarding keeps control until the user finishes it, even though the
  // vault is already unlocked after its first step.
  let onboarding = $state(false);

  onMount(() => {
    api.status().then((s) => {
      status = s;
      onboarding = s.state === 'none';
      theme.apply(s.settings.theme);
    });
    const off = onSession((s) => (status = s));
    return () => off.then((f) => f());
  });
</script>

{#if isCapture}
  {#if status}<QuickCapture {status} />{/if}
{:else if !status}
  <div class="backdrop h-full" data-tauri-drag-region></div>
{:else if onboarding}
  <Onboarding {status} ondone={async () => ((status = await api.status()), (onboarding = false))} />
{:else if status.state === 'unlocked'}
  <Workspace {status} />
{:else}
  <LockScreen {status} />
{/if}
