<script lang="ts">
  import { KeyRound, Fingerprint, ShieldCheck, Copy } from '@lucide/svelte';
  import { api, errorMessage, type Status } from '#lib/api.ts';

  let { status, ondone }: { status: Status; ondone: () => void } = $props();

  let step = $state<'create' | 'recovery' | 'touchid'>('create');
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
    const score = Math.min(4, Math.floor(password.length / 4) + classes - 2);
    return Math.max(0, score);
  });
  const strengthLabel = ['sehr schwach', 'schwach', 'okay', 'gut', 'stark'];

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

<div class="flex h-full items-center justify-center" data-tauri-drag-region>
  <div class="w-[420px] rounded-2xl border border-line bg-panel p-8 shadow-2xl">
    {#if step === 'create'}
      <div class="mb-6 flex items-center gap-3">
        <KeyRound class="text-accent" size={26} />
        <div>
          <h1 class="text-lg font-semibold">Willkommen bei Brainmatter</h1>
          <p class="text-muted">Dein Wissen wird lokal und verschlüsselt gespeichert.</p>
        </div>
      </div>
      <form class="flex flex-col gap-3" onsubmit={create}>
        <label class="flex flex-col gap-1">
          <span class="text-muted">Dein Name</span>
          <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
          <input class="field" bind:value={name} placeholder="z. B. Lasse" autofocus />
        </label>
        <label class="flex flex-col gap-1">
          <span class="text-muted">Master-Passwort</span>
          <input class="field" type="password" bind:value={password} autocomplete="new-password" />
        </label>
        {#if password}
          <div class="flex items-center gap-2">
            <div class="flex flex-1 gap-1">
              {#each [0, 1, 2, 3] as i}
                <div class="h-1 flex-1 rounded {i < strength ? (strength > 2 ? 'bg-ok' : 'bg-warn') : 'bg-line'}"></div>
              {/each}
            </div>
            <span class="text-xs text-muted">{strengthLabel[strength]}</span>
          </div>
        {/if}
        <label class="flex flex-col gap-1">
          <span class="text-muted">Passwort wiederholen</span>
          <input class="field" type="password" bind:value={confirm} autocomplete="new-password" />
        </label>
        <p class="text-xs text-faint">
          Ohne Passwort und Wiederherstellungsschlüssel sind die Daten nicht wiederherstellbar. Das ist Absicht.
        </p>
        {#if error}<p class="text-danger">{error}</p>{/if}
        <button class="btn-primary mt-2" disabled={busy || !name || password.length < 8}>
          {busy ? 'Tresor wird erstellt …' : 'Tresor erstellen'}
        </button>
      </form>
    {:else if step === 'recovery'}
      <div class="mb-4 flex items-center gap-3">
        <ShieldCheck class="text-ok" size={26} />
        <h1 class="text-lg font-semibold">Wiederherstellungsschlüssel</h1>
      </div>
      <p class="mb-4 text-muted">
        Falls du dein Passwort vergisst, ist das der einzige Weg zurück. Drucke ihn aus oder speichere ihn in deinem
        Passwort-Manager. Er wird <strong>nur dieses eine Mal</strong> angezeigt.
      </p>
      <div class="selectable mb-3 rounded-lg border border-line bg-elevated p-4 text-center font-mono text-[15px] leading-7 tracking-wide">
        {recoveryKey}
      </div>
      <button class="btn-ghost mb-4 w-full" onclick={copyKey}>
        <Copy size={14} /> {copied ? 'Kopiert' : 'In die Zwischenablage kopieren'}
      </button>
      <label class="mb-4 flex items-center gap-2">
        <input type="checkbox" bind:checked={saved} />
        <span>Ich habe den Schlüssel sicher aufbewahrt.</span>
      </label>
      <button
        class="btn-primary w-full"
        disabled={!saved}
        onclick={() => {
          recoveryKey = '';
          if (status.biometry_available) step = 'touchid';
          else ondone();
        }}>Weiter</button>
    {:else}
      <div class="mb-4 flex items-center gap-3">
        <Fingerprint class="text-accent" size={26} />
        <h1 class="text-lg font-semibold">Mit Touch ID entsperren?</h1>
      </div>
      <p class="mb-6 text-muted">
        Statt des Passworts genügt dann dein Finger. Nach App-Updates fragt Brainmatter einmal nach dem Passwort.
      </p>
      {#if error}<p class="mb-3 text-danger">{error}</p>{/if}
      <div class="flex gap-2">
        <button class="btn-ghost flex-1" onclick={ondone}>Später</button>
        <button class="btn-primary flex-1" disabled={busy} onclick={enableTouchId}>Touch ID aktivieren</button>
      </div>
    {/if}
  </div>
</div>
