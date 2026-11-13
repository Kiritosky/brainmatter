<script lang="ts">
  import { onMount } from 'svelte';
  import { api, onSession, type Status } from '#lib/api.ts';
  import { theme } from '#lib/theme.svelte.ts';
  import Onboarding from '#lib/components/Onboarding.svelte';
  import LockScreen from '#lib/components/LockScreen.svelte';

  // The graph and the editor make up most of the code. The lock screen comes first, and
  // they load while it shows, so they are there by the time the password is typed.
  // Plain variables: a promise is loaded once, and starting it is no change to show.
  let workspaceModule: Promise<typeof import('#lib/components/Workspace.svelte')> | undefined;
  const loadWorkspace = () => (workspaceModule ??= import('#lib/components/Workspace.svelte'));
  let captureModule: Promise<typeof import('#lib/components/QuickCapture.svelte')> | undefined;
  const loadCapture = () => (captureModule ??= import('#lib/components/QuickCapture.svelte'));
  let trayModule: Promise<typeof import('#lib/components/TrayPanel.svelte')> | undefined;
  const loadTray = () => (trayModule ??= import('#lib/components/TrayPanel.svelte'));
  let unlockModule: Promise<typeof import('#lib/components/UnlockPrompt.svelte')> | undefined;
  const loadUnlock = () => (unlockModule ??= import('#lib/components/UnlockPrompt.svelte'));

  // Every window loads this page and is told apart by its label. In a browser, with
  // the mock, `?window=capture`, `?window=tray` or `?window=unlock` stands in for it.
  const windowName: string =
    new URLSearchParams(location.search).get('window') ??
    (window as any).__TAURI_INTERNALS__?.metadata?.currentWindow?.label ??
    'main';
  const isCapture = windowName === 'capture';
  const isTray = windowName === 'tray';
  const isUnlock = windowName === 'unlock';

  let status = $state<Status | null>(null);
  // Onboarding keeps control until the user finishes it, even though the
  // vault is already unlocked after its first step.
  let onboarding = $state(false);

  onMount(() => {
    // After the first frame, so it does not hold up the lock screen.
    if (!isCapture && !isTray && !isUnlock) requestAnimationFrame(() => setTimeout(loadWorkspace));
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
  {#if status}
    {#await loadCapture() then { default: QuickCapture }}<QuickCapture {status} />{/await}
  {/if}
{:else if isTray}
  {#if status}
    {#await loadTray() then { default: TrayPanel }}<TrayPanel {status} />{/await}
  {/if}
{:else if isUnlock}
  {#if status}
    {#await loadUnlock() then { default: UnlockPrompt }}<UnlockPrompt {status} />{/await}
  {/if}
{:else if !status}
  <div class="backdrop h-full" data-tauri-drag-region></div>
{:else if onboarding}
  <Onboarding {status} ondone={async () => ((status = await api.status()), (onboarding = false))} />
{:else if status.state === 'unlocked'}
  {#await loadWorkspace()}
    <div class="backdrop h-full" data-tauri-drag-region></div>
  {:then { default: Workspace }}
    <Workspace {status} />
  {/await}
{:else}
  <LockScreen {status} />
{/if}
