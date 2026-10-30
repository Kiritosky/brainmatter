<script lang="ts">
  import { onMount } from 'svelte';
  import { Fingerprint, Lock } from '@lucide/svelte';
  import { api, errorMessage, isApiError, type Status } from '#lib/api.ts';

  let { status }: { status: Status } = $props();

  let password = $state('');
  let recovery = $state('');
  let useRecovery = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let attempts = 0;
  let input: HTMLInputElement | undefined = $state();

  const canTouchId = $derived(status.touch_id_enrolled && status.biometry_available);

  async function run(action: () => Promise<void>) {
    busy = true;
    error = null;
    try {
      await action();
    } catch (e) {
      error = errorMessage(e);
      if (isApiError(e) && e.code === 'wrong_password') {
        // Growing delay against guessing at the keyboard.
        attempts++;
        await new Promise((r) => setTimeout(r, Math.min(8000, 250 * 2 ** attempts)));
      }
      input?.focus();
    } finally {
      busy = false;
    }
  }

  const touchId = () => run(() => api.unlockTouchId());
  const submit = (e: SubmitEvent) => {
    e.preventDefault();
    run(() => (useRecovery ? api.unlockRecovery(recovery) : api.unlockPassword(password)));
  };

  onMount(() => {
    if (canTouchId) touchId();
  });
</script>

<div class="flex h-full items-center justify-center" data-tauri-drag-region>
  <div class="flex w-[340px] flex-col items-center gap-5">
    <div class="flex h-16 w-16 items-center justify-center rounded-2xl border border-line bg-panel">
      <Lock class="text-muted" size={28} />
    </div>
    <div class="text-center">
      <h1 class="text-lg font-semibold">Brainmatter ist gesperrt</h1>
      <p class="text-muted">Entsperre deinen Wissensspeicher.</p>
    </div>

    {#if canTouchId && !useRecovery}
      <button class="btn-primary w-full" disabled={busy} onclick={touchId}>
        <Fingerprint size={16} /> Mit Touch ID entsperren
      </button>
      <div class="text-xs text-faint">oder</div>
    {/if}

    <form class="flex w-full flex-col gap-2" onsubmit={submit}>
      {#if useRecovery}
        <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
        <input
          bind:this={input}
          class="field font-mono"
          bind:value={recovery}
          placeholder="XXXX-XXXX-…"
          autofocus />
      {:else}
        <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
        <input
          bind:this={input}
          class="field"
          type="password"
          bind:value={password}
          placeholder="Master-Passwort"
          autocomplete="current-password"
          autofocus={!canTouchId} />
      {/if}
      {#if error}<p class="text-center text-danger">{error}</p>{/if}
      <button class="btn-ghost" disabled={busy || (useRecovery ? !recovery : !password)}>
        {busy ? 'Entsperre …' : 'Entsperren'}
      </button>
    </form>

    <button class="text-xs text-faint hover:text-muted" onclick={() => ((useRecovery = !useRecovery), (error = null))}>
      {useRecovery ? 'Doch mit Passwort' : 'Passwort vergessen? Wiederherstellungsschlüssel verwenden'}
    </button>
  </div>
</div>
