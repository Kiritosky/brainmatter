<script lang="ts">
  import { t } from '#lib/i18n.ts';
  import { onMount, tick, untrack } from 'svelte';
  import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
  import { Fingerprint, LockOpen } from '@lucide/svelte';
  import { api, errorMessage, isApiError, onSession, onUnlockOpen, type Status } from '#lib/api.ts';
  import { pop } from '#lib/ui/motion.ts';

  // A small window in the middle of the screen, for an AI client that found the
  // vault locked. It stays until the user unlocks or turns it down.
  let { status: initial }: { status: Status } = $props();

  /** In points, as the window is made in `unlock_prompt.rs`. */
  const WIDTH = 380;
  const inTauri = document.documentElement.classList.contains('tauri');

  let status = $state(untrack(() => initial));
  let password = $state('');
  let error = $state<string | null>(null);
  let busy = $state(false);
  let waitingForFinger = $state(false);
  let shake = $state(false);
  let opened = $state(0);
  let attempts = 0;
  let panel: HTMLElement | undefined = $state();
  let field: HTMLInputElement | undefined = $state();

  const client = $derived(status.unlock_request);
  const canTouchId = $derived(status.touch_id_enrolled && status.biometry_available);

  async function reset() {
    password = '';
    error = null;
    attempts = 0;
    status = await api.status();
    opened++;
    await tick();
    field?.focus();
  }

  function fail(e: unknown) {
    error = errorMessage(e);
    shake = false;
    requestAnimationFrame(() => (shake = true));
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!password || busy) return;
    busy = true;
    error = null;
    try {
      await api.unlockPassword(password);
      password = '';
    } catch (err) {
      fail(err);
      if (isApiError(err) && err.code === 'wrong_password') {
        // Growing delay against guessing at the keyboard, as on the lock screen.
        attempts++;
        await new Promise((r) => setTimeout(r, Math.min(8000, 250 * 2 ** attempts)));
      }
      field?.select();
    } finally {
      busy = false;
    }
  }

  async function touchId() {
    waitingForFinger = true;
    error = null;
    try {
      await api.unlockTouchId();
    } catch (e) {
      if (!(isApiError(e) && (e.code === 'touch_id_cancelled' || e.code === 'touch_id_fallback'))) fail(e);
      field?.focus();
    } finally {
      waitingForFinger = false;
    }
  }

  const dismiss = () => api.dismissUnlockRequest();

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    e.preventDefault();
    dismiss();
  }

  // The window takes the height of what it shows, as the menu bar panel does.
  $effect(() => {
    if (!inTauri || !panel) return;
    const element = panel;
    const window = getCurrentWindow();
    const fit = () => window.setSize(new LogicalSize(WIDTH, element.offsetHeight + 16));
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    return () => observer.disconnect();
  });

  onMount(() => {
    const cleanups = [
      onUnlockOpen(reset),
      onSession((s) => {
        status = s;
        // Unlocked here or anywhere else, or turned down elsewhere: done.
        if (s.state !== 'locked' || !s.unlock_request) api.hideUnlockPrompt();
      }),
    ];
    reset();
    return () => cleanups.forEach((c) => c.then((off) => off()));
  });
</script>

<svelte:window {onkeydown} />

<div class="p-2">
  {#key opened}
    <div bind:this={panel} class="surface overflow-hidden rounded-2xl" role="dialog" aria-labelledby="unlock-title" in:pop={{ duration: 180, y: 6 }}>
      <header class="flex items-start gap-3 px-5 pt-5 pb-4" data-tauri-drag-region>
        <img src="/icon.png" alt="" class="pointer-events-none h-10 w-10 shrink-0 rounded-[10px]" draggable="false" />
        <div class="pointer-events-none min-w-0 flex-1">
          <div id="unlock-title" class="text-[14px] leading-snug font-semibold">
            {#if client}
              {t(`„${client}“ möchte auf Brainmatter zugreifen`, `“${client}” wants to access Brainmatter`)}
            {:else}
              {t('Brainmatter entsperren', 'Unlock Brainmatter')}
            {/if}
          </div>
          <div class="mt-0.5 text-[12px] text-faint">
            {waitingForFinger
              ? t('Leg deinen Finger auf den Sensor', 'Place your finger on the sensor')
              : t('Gib dein Master-Passwort ein, um Brainmatter zu entsperren.', 'Enter your master password to unlock Brainmatter.')}
          </div>
        </div>
      </header>

      <form class="px-5" class:shake onsubmit={submit} onanimationend={() => (shake = false)}>
        <input
          bind:this={field}
          bind:value={password}
          type="password"
          class="field w-full"
          placeholder={t('Master-Passwort', 'Master password')}
          aria-label={t('Master-Passwort', 'Master password')}
          autocomplete="current-password" />
        <div class="h-6 pt-1.5 text-xs text-danger" aria-live="polite">
          {#if error}<span in:pop={{ duration: 140, y: -2 }}>{error}</span>{/if}
        </div>

        <div class="flex items-center gap-2 pt-1 pb-4">
          {#if canTouchId}
            <button type="button" class="btn-ghost !px-2.5" disabled={waitingForFinger} onclick={touchId}
              aria-label={t('Mit Touch ID entsperren', 'Unlock with Touch ID')} title={t('Mit Touch ID entsperren', 'Unlock with Touch ID')}>
              <Fingerprint size={15} />
            </button>
          {/if}
          <button type="button" class="btn-ghost ml-auto" onclick={dismiss}>{t('Nicht jetzt', 'Not now')}</button>
          <button type="submit" class="btn-primary" disabled={!password || busy}>
            <LockOpen size={14} class={busy ? 'animate-pulse' : ''} />{t('Entsperren', 'Unlock')}
          </button>
        </div>
      </form>
    </div>
  {/key}
</div>
