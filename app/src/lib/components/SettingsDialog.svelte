<script lang="ts">
  import { plural, t } from '#lib/i18n.ts';
  import { onMount } from 'svelte';
  import { Archive, Bot, CircleHelp, X, Copy, Check, ShieldCheck, KeyRound, Lock, Network, Palette, ScanSearch } from '@lucide/svelte';
  import { api, errorMessage, type Settings, type Status } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { DEFAULT_PREFS, prefs, type Accent } from '#lib/prefs.svelte.ts';
  import Toggle from '#lib/ui/Toggle.svelte';
  import { theme } from '#lib/theme.svelte.ts';
  import Segmented from '#lib/ui/Segmented.svelte';
  import Select from '#lib/ui/Select.svelte';
  import { fadeQuick, pop } from '#lib/ui/motion.ts';
  import AiConnections from './AiConnections.svelte';
  import HelpSettings from './HelpSettings.svelte';
  import BackupSettings from './BackupSettings.svelte';
  import GraphSettings from './GraphSettings.svelte';
  import ModelSettings from './ModelSettings.svelte';
  import UpdateSettings from './UpdateSettings.svelte';

  type Tab = 'general' | 'graph' | 'security' | 'password' | 'backups' | 'search' | 'ai' | 'help';

  // Fixed colours: each swatch shows its own accent, whichever one is active.
  const ACCENTS: { value: Accent; label: string; color: string }[] = [
    { value: 'blue', label: t('Blau', 'Blue'), color: '#6386ff' },
    { value: 'teal', label: t('Türkis', 'Teal'), color: '#2fb8ac' },
    { value: 'green', label: t('Grün', 'Green'), color: '#4fbf72' },
    { value: 'amber', label: t('Bernstein', 'Amber'), color: '#e89b3a' },
    { value: 'rose', label: t('Rosa', 'Rose'), color: '#ee6d9a' },
  ];
  const isDefault = $derived((Object.keys(DEFAULT_PREFS) as (keyof typeof DEFAULT_PREFS)[]).every((k) => prefs.values[k] === DEFAULT_PREFS[k]));
  let tab = $state<Tab>('general');
  let status = $state<Status | null>(null);
  let settings = $state<Settings | null>(null);
  let message = $state<{ text: string; ok: boolean } | null>(null);
  let busy = $state(false);

  let current = $state('');
  let next = $state('');
  let confirm = $state('');
  let recoveryPassword = $state('');
  let newRecovery = $state('');
  let copied = $state(false);

  const THEMES: { value: Settings['theme']; label: string }[] = [
    { value: 'system', label: 'System' },
    { value: 'light', label: t('Hell', 'Light') },
    { value: 'dark', label: t('Dunkel', 'Dark') },
  ];

  const AUTO_LOCK = [5, 15, 30, 60, 120, 240, 480].map((m) => ({
    value: m,
    label: m < 60 ? plural(m, ['Minute', 'Minuten'], ['minute', 'minutes']) : plural(m / 60, ['Stunde', 'Stunden'], ['hour', 'hours']),
  }));

  const tabs: { id: Tab; label: string; icon: typeof Lock }[] = [
    { id: 'general', label: t('Allgemein', 'General'), icon: Palette },
    { id: 'graph', label: 'Graph', icon: Network },
    { id: 'security', label: t('Sperren', 'Locking'), icon: Lock },
    { id: 'password', label: t('Passwort & Schlüssel', 'Password & key'), icon: KeyRound },
    { id: 'backups', label: 'Backups', icon: Archive },
    { id: 'search', label: t('Suche nach Sinn', 'Search by meaning'), icon: ScanSearch },
    { id: 'ai', label: t('KI-Verbindungen', 'AI connections'), icon: Bot },
    { id: 'help', label: t('Hilfe', 'Help'), icon: CircleHelp },
  ];

  onMount(async () => {
    status = await api.status();
    settings = status.settings;
  });

  function flash(text: string, ok: boolean) {
    message = { text, ok };
    setTimeout(() => message?.text === text && (message = null), 3500);
  }

  async function run(fn: () => Promise<unknown>, success: string) {
    busy = true;
    try {
      await fn();
      flash(success, true);
      return true;
    } catch (e) {
      flash(errorMessage(e), false);
      return false;
    } finally {
      busy = false;
    }
  }

  async function save(patch: Partial<Settings>) {
    if (!settings) return;
    const next = { ...settings, ...patch };
    settings = next;
    await run(async () => (settings = await api.updateSettings(next)), t('Gespeichert', 'Saved'));
  }

  async function toggleTouchId(on: boolean) {
    await run(() => (on ? api.enrollTouchId() : api.disableTouchId()), on ? t('Touch ID aktiviert', 'Touch ID turned on') : t('Touch ID deaktiviert', 'Touch ID turned off'));
    status = await api.status();
  }

  async function changePassword(e: SubmitEvent) {
    e.preventDefault();
    if (next !== confirm) return flash(t('Die neuen Passwörter stimmen nicht überein.', 'The new passwords do not match.'), false);
    if (await run(() => api.changePassword(current, next), t('Passwort geändert', 'Password changed'))) current = next = confirm = '';
  }

  async function rotateRecovery(e: SubmitEvent) {
    e.preventDefault();
    await run(async () => (newRecovery = await api.rotateRecoveryKey(recoveryPassword)), t('Neuer Schlüssel erstellt', 'New key created'));
    recoveryPassword = '';
  }

  async function copy() {
    await navigator.clipboard.writeText(newRecovery);
    copied = true;
    setTimeout(() => (copied = false), 1600);
  }
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/45 backdrop-blur-[2px]" role="presentation"
  transition:fadeQuick onclick={() => (ws.settingsOpen = false)}>
  <div class="surface flex h-[560px] w-[760px] overflow-hidden rounded-xl" role="dialog" aria-label={t('Einstellungen', 'Settings')} tabindex="-1"
    transition:pop onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && (ws.settingsOpen = false)}>

    <nav class="flex w-52 shrink-0 flex-col gap-0.5 border-r border-line-soft bg-bg/40 p-3">
      <div class="px-2 pt-1 pb-3 text-[15px] font-semibold">{t('Einstellungen', 'Settings')}</div>
      {#each tabs as entry (entry.id)}
        <button
          class="flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors {tab === entry.id ? 'bg-text/8 text-text' : 'text-muted hover:bg-elevated hover:text-text'}"
          onclick={() => (tab = entry.id)}>
          <entry.icon size={15} class={tab === entry.id ? 'text-accent' : ''} />{entry.label}
        </button>
      {/each}
    </nav>

    <div class="relative flex min-w-0 flex-1 flex-col">
      <header class="flex h-12 shrink-0 items-center justify-end px-3">
        <button class="icon-btn" aria-label={t('Schließen', 'Close')} onclick={() => (ws.settingsOpen = false)}><X size={16} /></button>
      </header>

      <div class="flex-1 overflow-y-auto px-7 pb-7">
        {#if settings && status}
          {#if tab === 'general'}
            <h2 class="mb-5 text-lg font-semibold">{t('Allgemein', 'General')}</h2>
            <div class="card">
              <div class="row">
                <div class="flex-1">
                  <div>{t('Erscheinungsbild', 'Appearance')}</div>
                  <div class="text-xs text-faint">{t('Gilt für alle Fenster', 'Applies to every window')}</div>
                </div>
                <div class="w-64">
                  <Segmented value={settings.theme} options={THEMES} label={t('Erscheinungsbild', 'Appearance')}
                    onchange={(mode) => (theme.apply(mode), save({ theme: mode }))} />
                </div>
              </div>
              <div class="row">
                <div class="flex-1">{t('Akzentfarbe', 'Accent colour')}</div>
                <div class="flex gap-2" role="radiogroup" aria-label={t('Akzentfarbe', 'Accent colour')}>
                  {#each ACCENTS as a (a.value)}
                    <button class="swatch" style="--swatch: {a.color}" role="radio" aria-checked={prefs.values.accent === a.value}
                      aria-label={a.label} title={a.label} onclick={() => prefs.set('accent', a.value)}></button>
                  {/each}
                </div>
              </div>
              <div class="row">
                <div class="flex-1">
                  <div>{t('Weniger Bewegung', 'Reduce motion')}</div>
                  <div class="text-xs text-faint">{t('Übergänge und Animationen entfallen', 'No transitions or animations')}</div>
                </div>
                <Toggle checked={prefs.values.reduceMotion} label={t('Weniger Bewegung', 'Reduce motion')} onchange={(v) => prefs.set('reduceMotion', v)} />
              </div>
              <div class="row">
                <div class="flex-1">
                  <div>Quick Capture</div>
                  {#if settings.quick_capture && status.capture_shortcut_blocked}
                    <div class="text-xs text-warn" role="alert">{t('Eine andere App belegt', 'Another app uses')} <kbd class="kbd">⌥⇧Space</kbd>. {t('Gib es dort frei, dann schalte das hier aus und wieder ein.', 'Free it there, then turn this off and on again.')}</div>
                  {:else}
                    <div class="text-xs text-faint">{t('Mit', 'Capture something from anywhere with')} <kbd class="kbd">⌥⇧Space</kbd>{t(' von überall aus etwas festhalten', '')}</div>
                  {/if}
                </div>
                <Toggle checked={settings.quick_capture} label="Quick Capture" onchange={async (v) => { await save({ quick_capture: v }); status = await api.status(); }} />
              </div>
              <div class="row">
                <div class="flex-1">
                  <div>{t('KI-Aktivität anzeigen', 'Show AI activity')}</div>
                  <div class="text-xs text-faint">{t('Einblendung und pulsierende Einträge, wenn eine KI etwas schreibt', 'A notice and pulsing entries when an AI client writes something')}</div>
                </div>
                <Toggle checked={prefs.values.aiActivity} label={t('KI-Aktivität anzeigen', 'Show AI activity')} onchange={(v) => prefs.set('aiActivity', v)} />
              </div>
              <div class="row">
                <div class="flex-1">
                  <div>{t('Neuerungen nach Updates zeigen', 'Show what’s new after updates')}</div>
                  <div class="text-xs text-faint">{t('Die Liste bleibt in den Update-Einstellungen erreichbar', 'The list stays available in the update settings')}</div>
                </div>
                <Toggle checked={prefs.values.whatsNew} label={t('Neuerungen nach Updates zeigen', 'Show what’s new after updates')} onchange={(v) => prefs.set('whatsNew', v)} />
              </div>
            </div>
            <UpdateSettings {settings} onsave={save} />
            {#if !isDefault}
              <button class="btn-ghost mt-4" onclick={() => prefs.reset()}>{t('Darstellung zurücksetzen', 'Reset display settings')}</button>
            {/if}
          {:else if tab === 'graph'}
            <GraphSettings />
            {#if !isDefault}
              <button class="btn-ghost mt-4" onclick={() => prefs.reset()}>{t('Darstellung zurücksetzen', 'Reset display settings')}</button>
            {/if}
          {:else if tab === 'security'}
            <h2 class="mb-1 text-lg font-semibold">{t('Sperren', 'Locking')}</h2>
            <p class="mb-5 text-muted">{t('Gesperrt ist alles verschlüsselt, und auch KIs erhalten keinen Zugriff.', 'While locked, everything is encrypted, and AI clients get no access either.')}</p>
            <div class="card">
              <div class="row">
                <div class="flex-1">
                  <div>{t('Automatisch sperren', 'Lock automatically')}</div>
                  <div class="text-xs text-faint">{t('Nach dieser Zeit ohne Aktivität', 'After this long without activity')}</div>
                </div>
                <Select value={settings.auto_lock_minutes} options={AUTO_LOCK} label={t('Automatisch sperren nach', 'Lock automatically after')}
                  onchange={(m) => save({ auto_lock_minutes: m })} />
              </div>
              <div class="row">
                <div class="flex-1">
                  <div>{t('Beim Ruhezustand sperren', 'Lock on sleep')}</div>
                  <div class="text-xs text-faint">{t('Wenn der Mac schlafen geht', 'When the Mac goes to sleep')}</div>
                </div>
                <Toggle checked={settings.lock_on_sleep} label={t('Beim Ruhezustand sperren', 'Lock on sleep')} onchange={(v) => save({ lock_on_sleep: v })} />
              </div>
              <div class="row">
                <div class="flex-1">
                  <div>{t('Bei Bildschirmsperre sperren', 'Lock with the screen')}</div>
                  <div class="text-xs text-faint">{t('Auch wenn du kurz ⌃⌘Q drückst', 'Also when you briefly press ⌃⌘Q')}</div>
                </div>
                <Toggle checked={settings.lock_on_screen_lock} label={t('Bei Bildschirmsperre sperren', 'Lock with the screen')} onchange={(v) => save({ lock_on_screen_lock: v })} />
              </div>
            </div>

            <h3 class="section-title mt-7">{t('Entsperren', 'Unlocking')}</h3>
            <div class="card">
              <div class="row">
                <div class="flex-1">
                  <div>Touch ID</div>
                  <div class="text-xs text-faint">
                    {status.biometry_available ? t('Statt des Passworts deinen Finger verwenden', 'Use your finger instead of the password') : t('Auf diesem Mac nicht verfügbar', 'Not available on this Mac')}
                  </div>
                </div>
                <Toggle checked={status.touch_id_enrolled} disabled={busy || !status.biometry_available} label="Touch ID" onchange={toggleTouchId} />
              </div>
            </div>
          {:else if tab === 'help'}
            <HelpSettings />
          {:else if tab === 'ai'}
            <AiConnections />
          {:else if tab === 'search'}
            <ModelSettings />
          {:else if tab === 'backups'}
            <BackupSettings {settings} onsave={save} />
          {:else}
            <h2 class="mb-1 text-lg font-semibold">{t('Passwort & Schlüssel', 'Password & key')}</h2>
            <p class="mb-5 text-muted">{t('Beide Aktionen verlangen dein aktuelles Passwort.', 'Both need your current password.')}</p>

            <form class="card p-4" onsubmit={changePassword}>
              <div class="mb-3 font-medium">{t('Master-Passwort ändern', 'Change the master password')}</div>
              <div class="flex flex-col gap-2">
                <input class="field" type="password" placeholder={t('Aktuelles Passwort', 'Current password')} bind:value={current} autocomplete="current-password" />
                <div class="grid grid-cols-2 gap-2">
                  <input class="field" type="password" placeholder={t('Neues Passwort', 'New password')} bind:value={next} autocomplete="new-password" />
                  <input class="field" type="password" placeholder={t('Wiederholen', 'Repeat')} bind:value={confirm} autocomplete="new-password" />
                </div>
              </div>
              <div class="mt-3 flex justify-end">
                <button class="btn-primary" disabled={busy || !current || next.length < 8}>{t('Passwort ändern', 'Change password')}</button>
              </div>
            </form>

            <div class="card mt-4 p-4">
              <div class="mb-1 flex items-center gap-2 font-medium"><ShieldCheck size={15} class="text-ok" /> {t('Wiederherstellungsschlüssel', 'Recovery key')}</div>
              {#if newRecovery}
                <p class="mb-3 text-xs text-muted">{t('Nur jetzt sichtbar. Der alte Schlüssel gilt ab sofort nicht mehr.', 'Visible only now. The old key no longer works.')}</p>
                <div class="selectable mb-3 rounded-lg border border-line bg-bg/60 p-3 text-center font-mono text-[14px] tracking-wider" transition:pop>{newRecovery}</div>
                <div class="flex gap-2">
                  <button class="btn-ghost flex-1" onclick={copy}>
                    {#if copied}<Check size={14} class="text-ok" /> {t('Kopiert', 'Copied')}{:else}<Copy size={14} /> {t('Kopieren', 'Copy')}{/if}
                  </button>
                  <button class="btn-primary flex-1" onclick={() => (newRecovery = '')}>{t('Gesichert', 'Saved it')}</button>
                </div>
              {:else}
                <p class="mb-3 text-xs text-muted">{t('Erstellt einen neuen Schlüssel und macht den alten ungültig, etwa wenn er in falsche Hände geraten sein könnte.', 'Creates a new key and voids the old one, for example if it may have fallen into the wrong hands.')}</p>
                <form class="flex gap-2" onsubmit={rotateRecovery}>
                  <input class="field flex-1" type="password" placeholder={t('Aktuelles Passwort', 'Current password')} bind:value={recoveryPassword} />
                  <button class="btn-ghost" disabled={busy || !recoveryPassword}>{t('Neu erstellen', 'Create new key')}</button>
                </form>
              {/if}
            </div>
          {/if}
        {/if}
      </div>

      {#if message}
        <div class="absolute right-6 bottom-5 flex items-center gap-2 rounded-lg border px-3 py-2 text-sm shadow-lg {message.ok ? 'border-ok/30 bg-ok/10 text-ok' : 'border-danger/30 bg-danger/10 text-danger'}"
          transition:pop={{ y: 8 }}>
          {#if message.ok}<Check size={14} />{/if}{message.text}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .swatch {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--swatch);
    outline: 2px solid transparent;
    outline-offset: 2px;
    transition: outline-color 140ms, transform 140ms var(--ease-out-soft);
  }
  .swatch:hover {
    transform: scale(1.12);
  }
  .swatch[aria-checked='true'] {
    outline-color: var(--swatch);
  }
</style>
