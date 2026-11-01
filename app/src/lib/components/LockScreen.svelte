<script lang="ts">
  import { onMount } from 'svelte';
  import { ArrowRight, Fingerprint } from '@lucide/svelte';
  import { api, errorMessage, isApiError, type Status } from '#lib/api.ts';
  import { pop } from '#lib/ui/motion.ts';

  let { status }: { status: Status } = $props();

  let password = $state('');
  let recovery = $state('');
  let useRecovery = $state(false);
  let busy = $state(false);
  let waitingForFinger = $state(false);
  let error = $state<string | null>(null);
  let shake = $state(false);
  let attempts = 0;
  let input: HTMLInputElement | undefined = $state();

  const canTouchId = $derived(status.touch_id_enrolled && status.biometry_available);

  function fail(e: unknown) {
    error = errorMessage(e);
    shake = false;
    requestAnimationFrame(() => (shake = true));
  }

  async function touchId() {
    waitingForFinger = true;
    error = null;
    try {
      await api.unlockTouchId();
    } catch (e) {
      // Dismissing the prompt is not an error worth showing.
      if (!(isApiError(e) && e.code === 'touch_id_cancelled')) fail(e);
      input?.focus();
    } finally {
      waitingForFinger = false;
    }
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      await (useRecovery ? api.unlockRecovery(recovery) : api.unlockPassword(password));
    } catch (err) {
      fail(err);
      if (isApiError(err) && err.code === 'wrong_password') {
        // Growing delay against guessing at the keyboard.
        attempts++;
        await new Promise((r) => setTimeout(r, Math.min(8000, 250 * 2 ** attempts)));
      }
      input?.select();
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    if (canTouchId) touchId();
  });
</script>

<div class="backdrop flex h-full items-center justify-center" data-tauri-drag-region>
  <div class="flex w-[320px] flex-col items-center" in:pop={{ duration: 320, y: 10 }}>
    <img src="/icon.png" alt="" class="mb-5 h-20 w-20 drop-shadow-[0_12px_24px_rgba(0,0,0,0.45)]" draggable="false" />
    <h1 class="text-xl font-semibold tracking-tight">Brainmatter</h1>
    <p class="mt-1 mb-7 text-muted">{waitingForFinger ? 'Leg deinen Finger auf den Sensor' : 'Gesperrt'}</p>

    {#if canTouchId && !useRecovery}
      <button
        class="mb-6 flex h-14 w-14 items-center justify-center rounded-full border border-line bg-elevated text-text transition-colors duration-150 hover:bg-raised {waitingForFinger ? 'pulse' : ''}"
        aria-label="Mit Touch ID entsperren"
        onclick={touchId}>
        <Fingerprint size={26} />
      </button>
    {/if}

    <form class="w-full" class:shake onsubmit={submit} onanimationend={() => (shake = false)}>
      <div class="relative">
        {#if useRecovery}
          <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
          <input bind:this={input} class="field w-full pr-11 font-mono" bind:value={recovery} placeholder="XXXX-XXXX-…" autofocus />
        {:else}
          <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
          <input bind:this={input} class="field w-full pr-11" type="password" bind:value={password}
            placeholder="Master-Passwort" autocomplete="current-password" autofocus={!canTouchId} />
        {/if}
        <button class="icon-btn absolute top-1 right-1 !text-accent disabled:opacity-30"
          aria-label="Entsperren" disabled={busy || (useRecovery ? !recovery : !password)}>
          <ArrowRight size={16} class={busy ? 'animate-pulse' : ''} />
        </button>
      </div>
    </form>

    <div class="mt-3 h-5 text-center text-danger">
      {#if error}<span transition:pop={{ duration: 160, y: -3 }}>{error}</span>{/if}
    </div>

    <button class="mt-3 text-xs text-faint transition-colors hover:text-muted"
      onclick={() => ((useRecovery = !useRecovery), (error = null))}>
      {useRecovery ? 'Doch mit Passwort entsperren' : 'Passwort vergessen?'}
    </button>
  </div>
</div>
