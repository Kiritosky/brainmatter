<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { Check, CornerDownLeft, Fingerprint, Lock, Zap } from '@lucide/svelte';
  import { api, errorMessage, onCaptureOpen, onSession, type Status } from '#lib/api.ts';
  import { pop } from '#lib/ui/motion.ts';

  let { status: initial }: { status: Status } = $props();

  let status = $state(untrack(() => initial));
  let text = $state('');
  let saved = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  // A Touch ID prompt takes the focus; that must not close this window.
  let unlocking = $state(false);
  let field: HTMLTextAreaElement | undefined = $state();

  const locked = $derived(status.state !== 'unlocked');

  async function reset() {
    text = '';
    saved = false;
    error = null;
    status = await api.status();
    await tick();
    field?.focus();
  }

  async function close() {
    text = '';
    await api.hideCapture();
  }

  /** The first line is the title; the rest is the text of the entry. */
  async function submit() {
    const [first, ...rest] = text.split('\n');
    const title = first.trim();
    if (!title || saving) return;
    saving = true;
    error = null;
    try {
      await api.createNode({ title, body: rest.join('\n').trim() });
      saved = true;
      setTimeout(close, 450);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = false;
    }
  }

  async function unlock() {
    unlocking = true;
    error = null;
    try {
      await api.unlockTouchId();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      unlocking = false;
    }
    await tick();
    field?.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
    else if (e.key === 'Enter' && !e.shiftKey && !locked) {
      e.preventDefault();
      submit();
    }
  }

  onMount(() => {
    const cleanups = [
      onCaptureOpen(reset),
      onSession((s) => (status = s)),
      // Clicking elsewhere dismisses it, like Spotlight. Not in a plain browser.
      document.documentElement.classList.contains('tauri')
        ? getCurrentWindow().onFocusChanged(({ payload: focused }) => {
            if (!focused && !unlocking) close();
          })
        : Promise.resolve(() => {}),
    ];
    reset();
    return () => cleanups.forEach((c) => c.then((off) => off()));
  });
</script>

<svelte:window {onkeydown} />

<div class="flex h-full items-start justify-center p-2">
  <div class="surface flex h-full w-full flex-col overflow-hidden rounded-2xl" role="dialog" aria-label="Quick Capture" in:pop={{ duration: 160, y: -6 }}>
    {#if locked}
      <div class="flex flex-1 items-center gap-4 px-5">
        <div class="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl bg-accent/12 text-accent"><Lock size={20} /></div>
        <div class="min-w-0 flex-1">
          <div class="font-medium">Brainmatter ist gesperrt</div>
          <div class="truncate text-xs {error ? 'text-danger' : 'text-faint'}">{error ?? 'Zum Festhalten erst entsperren.'}</div>
        </div>
        {#if status.touch_id_enrolled}
          <button class="btn-primary shrink-0" disabled={unlocking} onclick={unlock}><Fingerprint size={15} /> Touch ID</button>
        {:else}
          <button class="btn-primary shrink-0" onclick={() => api.showMainWindow()}>Öffnen</button>
        {/if}
      </div>
    {:else if saved}
      <div class="flex flex-1 items-center justify-center gap-2.5 font-medium text-ok" in:pop={{ duration: 180, y: 0 }}>
        <span class="flex h-6 w-6 items-center justify-center rounded-full bg-ok/15"><Check size={14} /></span> Festgehalten
      </div>
    {:else}
      <div class="flex min-h-0 flex-1 gap-3 px-4 pt-3.5">
        <Zap size={16} class="mt-0.5 shrink-0 text-accent" />
        <textarea
          class="h-full min-w-0 flex-1 resize-none bg-transparent text-[15px] leading-snug outline-none placeholder:text-faint"
          placeholder="Was willst du festhalten?"
          aria-label="Neuer Eintrag"
          bind:this={field}
          bind:value={text}></textarea>
      </div>
      <footer class="flex items-center gap-4 border-t border-line-soft px-4 py-2 text-[11px] text-faint">
        {#if error}
          <span class="truncate text-danger">{error}</span>
        {:else}
          <span>Erste Zeile wird zum Titel</span>
        {/if}
        <span class="ml-auto flex items-center gap-1.5"><span class="kbd">⇧↵</span> Neue Zeile</span>
        <span class="flex items-center gap-1.5"><span class="kbd"><CornerDownLeft size={10} /></span> Sichern</span>
        <span class="flex items-center gap-1.5"><span class="kbd">esc</span> Schließen</span>
      </footer>
    {/if}
  </div>
</div>
