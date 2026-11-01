<script lang="ts">
  import { Fingerprint, ShieldCheck, Copy, Check } from '@lucide/svelte';
  import { api, errorMessage, type Status } from '#lib/api.ts';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import { pop } from '#lib/ui/motion.ts';

  let { status, ondone }: { status: Status; ondone: () => void } = $props();

  let step = $state<'create' | 'recovery' | 'touchid'>('create');
  const steps = $derived(status.biometry_available ? ['create', 'recovery', 'touchid'] : ['create', 'recovery']);

  let name = $state('');
  let password = $state('');
  let confirm = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);
  let recoveryKey = $state('');
  let saved = $state(false);
  let copied = $state(false);

  // Rough strength hint: length and character classes.
  let strength = $derived.by(() => {
    const classes = [/[a-z]/, /[A-Z]/, /\d/, /[^\w\s]/].filter((r) => r.test(password)).length;
    return Math.max(0, Math.min(4, Math.floor(password.length / 4) + classes - 2));
  });
  const strengthLabel = ['Sehr schwach', 'Schwach', 'Okay', 'Gut', 'Stark'];
  const strengthColor = ['var(--color-danger)', 'var(--color-danger)', 'var(--color-warn)', 'var(--color-ok)', 'var(--color-ok)'];

  async function create(e: SubmitEvent) {
    e.preventDefault();
    error = null;
    if (password !== confirm) {
      error = 'Die Passwörter stimmen nicht überein.';
      return;
    }
    busy = true;
    try {
      recoveryKey = await api.createVault(password, name);
      password = confirm = '';
      step = 'recovery';
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }

  async function copyKey() {
    await navigator.clipboard.writeText(recoveryKey);
    copied = true;
    setTimeout(() => (copied = false), 1600);
  }

  async function enableTouchId() {
    busy = true;
    error = null;
    try {
      await api.enrollTouchId();
      ondone();
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="backdrop flex h-full flex-col items-center justify-center" data-tauri-drag-region>
  {#key step}
    <div class="surface w-[430px] rounded-xl p-8" in:pop={{ duration: 300, y: 12 }}>
      {#if step === 'create'}
        <img src="/icon.png" alt="" class="mb-4 h-14 w-14" draggable="false" />
        <h1 class="text-xl font-semibold tracking-tight">Willkommen bei Brainmatter</h1>
        <p class="mt-1 mb-6 text-muted">Dein Wissen, lokal und verschlüsselt auf diesem Mac.</p>
        <form class="flex flex-col gap-3.5" onsubmit={create}>
          <label class="flex flex-col gap-1.5">
            <span class="text-xs font-medium text-muted">Dein Name</span>
            <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
            <input class="field" bind:value={name} placeholder="z. B. Lasse" autofocus />
          </label>
          <label class="flex flex-col gap-1.5">
            <span class="text-xs font-medium text-muted">Master-Passwort</span>
            <input class="field" type="password" bind:value={password} autocomplete="new-password" />
          </label>
          <div class="flex h-4 items-center gap-2.5">
            <div class="h-1 flex-1 overflow-hidden rounded-full bg-line">
              <div class="h-full rounded-full transition-[width,background-color] duration-300"
                style="width: {password ? ((strength + 1) / 5) * 100 : 0}%; background: {strengthColor[strength]}"></div>
            </div>
            <span class="w-20 text-right text-[11px] text-faint">{password ? strengthLabel[strength] : ''}</span>
          </div>
          <label class="flex flex-col gap-1.5">
            <span class="text-xs font-medium text-muted">Passwort wiederholen</span>
            <input class="field" type="password" bind:value={confirm} autocomplete="new-password" />
          </label>
          <p class="text-xs leading-relaxed text-faint">
            Ohne Passwort und Wiederherstellungsschlüssel sind die Daten nicht wiederherstellbar. Das ist Absicht.
          </p>
          {#if error}<p class="text-danger" transition:pop={{ duration: 160, y: -3 }}>{error}</p>{/if}
          <button class="btn-primary mt-1" disabled={busy || !name.trim() || password.length < 8}>
            {busy ? 'Tresor wird erstellt …' : 'Tresor erstellen'}
          </button>
        </form>
      {:else if step === 'recovery'}
        <div class="mb-4 flex h-12 w-12 items-center justify-center rounded-xl bg-ok/15 text-ok"><ShieldCheck size={24} /></div>
        <h1 class="text-xl font-semibold tracking-tight">Dein Wiederherstellungsschlüssel</h1>
        <p class="mt-1 mb-5 leading-relaxed text-muted">
          Falls du dein Passwort vergisst, ist das der einzige Weg zurück. Speichere ihn im Passwort-Manager oder drucke ihn aus.
          Er wird <strong class="text-text">nur dieses eine Mal</strong> angezeigt.
        </p>
        <div class="selectable mb-3 rounded-xl border border-line bg-bg/60 px-4 py-3.5 text-center font-mono text-[14.5px] leading-7 tracking-wider">
          {recoveryKey}
        </div>
        <button class="btn-ghost mb-5 w-full" onclick={copyKey}>
          {#if copied}<Check size={14} class="text-ok" /> Kopiert{:else}<Copy size={14} /> In die Zwischenablage kopieren{/if}
        </button>
        <div class="mb-5"><Checkbox bind:checked={saved}>Ich habe den Schlüssel sicher aufbewahrt.</Checkbox></div>
        <button class="btn-primary w-full" disabled={!saved}
          onclick={() => {
            recoveryKey = '';
            if (status.biometry_available) step = 'touchid';
            else ondone();
          }}>Weiter</button>
      {:else}
        <div class="mb-4 flex h-12 w-12 items-center justify-center text-faint"><Fingerprint size={24} /></div>
        <h1 class="text-xl font-semibold tracking-tight">Mit Touch ID entsperren?</h1>
        <p class="mt-1 mb-6 leading-relaxed text-muted">
          Statt des Passworts genügt dann dein Finger. Nach App-Updates fragt Brainmatter einmal nach dem Passwort.
        </p>
        {#if error}<p class="mb-3 text-danger">{error}</p>{/if}
        <div class="flex gap-2">
          <button class="btn-ghost flex-1" onclick={ondone}>Später</button>
          <button class="btn-primary flex-1" disabled={busy} onclick={enableTouchId}>Touch ID aktivieren</button>
        </div>
      {/if}
    </div>
  {/key}

  <div class="mt-5 flex gap-1.5">
    {#each steps as s (s)}
      <span class="h-1.5 rounded-full transition-all duration-300 {s === step ? 'w-5 bg-accent' : 'w-1.5 bg-line'}"></span>
    {/each}
  </div>
</div>
