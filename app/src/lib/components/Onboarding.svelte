<script lang="ts">
  import { t } from '#lib/i18n.ts';
  import { ArrowLeft, Fingerprint, HardDrive, ShieldCheck, Copy, Check } from '@lucide/svelte';
  import { api, errorMessage, type Status } from '#lib/api.ts';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import { pop } from '#lib/ui/motion.ts';

  let { status, ondone }: { status: Status; ondone: () => void } = $props();

  let step = $state<'create' | 'recovery' | 'touchid' | 'adopt'>('create');
  const steps = $derived(
    step === 'adopt' ? [] : status.biometry_available ? ['create', 'recovery', 'touchid'] : ['create', 'recovery'],
  );

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
  const strengthLabel = [t('Sehr schwach', 'Very weak'), t('Schwach', 'Weak'), t('Okay', 'Okay'), t('Gut', 'Good'), t('Stark', 'Strong')];
  const strengthColor = ['var(--color-danger)', 'var(--color-danger)', 'var(--color-warn)', 'var(--color-ok)', 'var(--color-ok)'];

  async function create(e: SubmitEvent) {
    e.preventDefault();
    error = null;
    if (password !== confirm) {
      error = t('Die Passwörter stimmen nicht überein.', 'The passwords do not match.');
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

  /** From the copy of the backups on a drive, as on a new Mac. */
  async function adopt(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      if (await api.adoptBackup(password)) {
        password = '';
        ondone();
      }
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }

  function goTo(next: typeof step) {
    error = null;
    password = confirm = '';
    step = next;
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
        <h1 class="text-xl font-semibold tracking-tight">{t('Willkommen bei Brainmatter', 'Welcome to Brainmatter')}</h1>
        <p class="mt-1 mb-6 text-muted">{t('Dein Wissen, lokal und verschlüsselt auf diesem Mac.', 'What you know, local and encrypted on this Mac.')}</p>
        <form class="flex flex-col gap-3.5" onsubmit={create}>
          <label class="flex flex-col gap-1.5">
            <span class="text-xs font-medium text-muted">{t('Dein Name', 'Your name')}</span>
            <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
            <input class="field" bind:value={name} placeholder={t('z. B. Alex', 'e.g. Alex')} autofocus />
          </label>
          <label class="flex flex-col gap-1.5">
            <span class="text-xs font-medium text-muted">{t('Master-Passwort', 'Master password')}</span>
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
            <span class="text-xs font-medium text-muted">{t('Passwort wiederholen', 'Repeat the password')}</span>
            <input class="field" type="password" bind:value={confirm} autocomplete="new-password" />
          </label>
          <p class="text-xs leading-relaxed text-faint">
            {t('Ohne Passwort und Wiederherstellungsschlüssel sind die Daten nicht wiederherstellbar. Das ist Absicht.', 'Without the password and the recovery key, the data cannot be recovered. That is on purpose.')}
          </p>
          {#if error}<p class="text-danger" transition:pop={{ duration: 160, y: -3 }}>{error}</p>{/if}
          <button class="btn-primary mt-1" disabled={busy || !name.trim() || password.length < 8}>
            {busy ? t('Tresor wird erstellt …', 'Creating the vault …') : t('Tresor erstellen', 'Create vault')}
          </button>
        </form>
        <button class="mt-4 w-full text-center text-xs text-muted transition-colors hover:text-text" onclick={() => goTo('adopt')}>
          {t('Schon Brainmatter genutzt? Aus einem Backup übernehmen', 'Used Brainmatter before? Restore from a backup')}
        </button>
      {:else if step === 'adopt'}
        <div class="mb-4 flex h-12 w-12 items-center justify-center rounded-xl bg-accent/15 text-accent"><HardDrive size={22} /></div>
        <h1 class="text-xl font-semibold tracking-tight">{t('Aus einem Backup übernehmen', 'Restore from a backup')}</h1>
        <p class="mt-1 mb-5 leading-relaxed text-muted">
          {t(
            'Wähle den Ordner „Brainmatter-Backup“, etwa auf deinem externen Laufwerk. Das Master-Passwort von damals öffnet ihn, und das neueste Backup wird dein Tresor.',
            'Choose the “Brainmatter-Backup” folder, for example on your external drive. The master password you used then opens it, and the newest backup becomes your vault.',
          )}
        </p>
        <form class="flex flex-col gap-3.5" onsubmit={adopt}>
          <label class="flex flex-col gap-1.5">
            <span class="text-xs font-medium text-muted">{t('Master-Passwort', 'Master password')}</span>
            <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
            <input class="field" type="password" bind:value={password} autocomplete="current-password" autofocus />
          </label>
          {#if error}<p class="text-danger" transition:pop={{ duration: 160, y: -3 }}>{error}</p>{/if}
          <div class="mt-1 flex gap-2">
            <button type="button" class="btn-ghost" aria-label={t('Zurück', 'Back')} onclick={() => goTo('create')}><ArrowLeft size={14} /></button>
            <button class="btn-primary flex-1" disabled={busy || !password}>{busy ? t('Wird übernommen …', 'Taking over …') : t('Ordner wählen', 'Choose folder')}</button>
          </div>
        </form>
      {:else if step === 'recovery'}
        <div class="mb-4 flex h-12 w-12 items-center justify-center rounded-xl bg-ok/15 text-ok"><ShieldCheck size={24} /></div>
        <h1 class="text-xl font-semibold tracking-tight">{t('Dein Wiederherstellungsschlüssel', 'Your recovery key')}</h1>
        <p class="mt-1 mb-5 leading-relaxed text-muted">
          {t(
            'Falls du dein Passwort vergisst, ist das der einzige Weg zurück. Speichere ihn im Passwort-Manager oder drucke ihn aus.',
            'If you forget your password, this is the only way back. Keep it in a password manager or print it.',
          )}
          {t('Er wird', 'It is shown')} <strong class="text-text">{t('nur dieses eine Mal', 'only this once')}</strong>{t(' angezeigt.', '.')}
        </p>
        <div class="selectable mb-3 rounded-xl border border-line bg-bg/60 px-4 py-3.5 text-center font-mono text-[14.5px] leading-7 tracking-wider">
          {recoveryKey}
        </div>
        <button class="btn-ghost mb-5 w-full" onclick={copyKey}>
          {#if copied}<Check size={14} class="text-ok" /> {t('Kopiert', 'Copied')}{:else}<Copy size={14} /> {t('In die Zwischenablage kopieren', 'Copy to the clipboard')}{/if}
        </button>
        <div class="mb-5"><Checkbox bind:checked={saved}>{t('Ich habe den Schlüssel sicher aufbewahrt.', 'I have put the key somewhere safe.')}</Checkbox></div>
        <button class="btn-primary w-full" disabled={!saved}
          onclick={() => {
            recoveryKey = '';
            if (status.biometry_available) step = 'touchid';
            else ondone();
          }}>{t('Weiter', 'Continue')}</button>
      {:else}
        <div class="mb-4 flex h-12 w-12 items-center justify-center text-faint"><Fingerprint size={24} /></div>
        <h1 class="text-xl font-semibold tracking-tight">{t('Mit Touch ID entsperren?', 'Unlock with Touch ID?')}</h1>
        <p class="mt-1 mb-6 leading-relaxed text-muted">
          {t('Statt des Passworts genügt dann dein Finger. Nach App-Updates fragt Brainmatter einmal nach dem Passwort.', 'Your finger is then enough instead of the password. After app updates, Brainmatter asks for the password once.')}
        </p>
        {#if error}<p class="mb-3 text-danger">{error}</p>{/if}
        <div class="flex gap-2">
          <button class="btn-ghost flex-1" onclick={ondone}>{t('Später', 'Later')}</button>
          <button class="btn-primary flex-1" disabled={busy} onclick={enableTouchId}>{t('Touch ID aktivieren', 'Turn on Touch ID')}</button>
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
