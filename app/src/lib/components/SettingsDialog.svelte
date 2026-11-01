<script lang="ts">
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
    { value: 'blue', label: 'Blau', color: '#6386ff' },
    { value: 'teal', label: 'Türkis', color: '#2fb8ac' },
    { value: 'green', label: 'Grün', color: '#4fbf72' },
    { value: 'amber', label: 'Bernstein', color: '#e89b3a' },
    { value: 'rose', label: 'Rosa', color: '#ee6d9a' },
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
    { value: 'light', label: 'Hell' },
    { value: 'dark', label: 'Dunkel' },
  ];

  const AUTO_LOCK = [5, 15, 30, 60, 120, 240, 480].map((m) => ({
    value: m,
    label: m < 60 ? `${m} Minuten` : m === 60 ? '1 Stunde' : `${m / 60} Stunden`,
  }));

  const tabs: { id: Tab; label: string; icon: typeof Lock }[] = [
    { id: 'general', label: 'Allgemein', icon: Palette },
    { id: 'graph', label: 'Graph', icon: Network },
    { id: 'security', label: 'Sperren', icon: Lock },
    { id: 'password', label: 'Passwort & Schlüssel', icon: KeyRound },
    { id: 'backups', label: 'Backups', icon: Archive },
    { id: 'search', label: 'Suche nach Sinn', icon: ScanSearch },
    { id: 'ai', label: 'KI-Verbindungen', icon: Bot },
    { id: 'help', label: 'Hilfe', icon: CircleHelp },
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
    await run(async () => (settings = await api.updateSettings(next)), 'Gespeichert');
  }

  async function toggleTouchId(on: boolean) {
    await run(() => (on ? api.enrollTouchId() : api.disableTouchId()), on ? 'Touch ID aktiviert' : 'Touch ID deaktiviert');
    status = await api.status();
  }

  async function changePassword(e: SubmitEvent) {
    e.preventDefault();
    if (next !== confirm) return flash('Die neuen Passwörter stimmen nicht überein.', false);
    if (await run(() => api.changePassword(current, next), 'Passwort geändert')) current = next = confirm = '';
  }

  async function rotateRecovery(e: SubmitEvent) {
    e.preventDefault();
    await run(async () => (newRecovery = await api.rotateRecoveryKey(recoveryPassword)), 'Neuer Schlüssel erstellt');
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
  <div class="surface flex h-[560px] w-[760px] overflow-hidden rounded-xl" role="dialog" aria-label="Einstellungen" tabindex="-1"
    transition:pop onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && (ws.settingsOpen = false)}>

    <nav class="flex w-52 shrink-0 flex-col gap-0.5 border-r border-line-soft bg-bg/40 p-3">
      <div class="px-2 pt-1 pb-3 text-[15px] font-semibold">Einstellungen</div>
      {#each tabs as t (t.id)}
        <button
          class="flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left transition-colors {tab === t.id ? 'bg-text/8 text-text' : 'text-muted hover:bg-elevated hover:text-text'}"
          onclick={() => (tab = t.id)}>
          <t.icon size={15} class={tab === t.id ? 'text-accent' : ''} />{t.label}
        </button>
      {/each}
    </nav>

    <div class="relative flex min-w-0 flex-1 flex-col">
      <header class="flex h-12 shrink-0 items-center justify-end px-3">
        <button class="icon-btn" aria-label="Schließen" onclick={() => (ws.settingsOpen = false)}><X size={16} /></button>
      </header>

      <div class="flex-1 overflow-y-auto px-7 pb-7">
        {#if settings && status}
          {#if tab === 'general'}
            <h2 class="mb-5 text-lg font-semibold">Allgemein</h2>
            <div class="card">
              <div class="row">
                <div class="flex-1">
                  <div>Erscheinungsbild</div>
                  <div class="text-xs text-faint">Gilt für alle Fenster</div>
                </div>
                <div class="w-64">
                  <Segmented value={settings.theme} options={THEMES} label="Erscheinungsbild"
                    onchange={(t) => (theme.apply(t), save({ theme: t }))} />
                </div>
              </div>
              <div class="row">
                <div class="flex-1">Akzentfarbe</div>
                <div class="flex gap-2" role="radiogroup" aria-label="Akzentfarbe">
                  {#each ACCENTS as a (a.value)}
                    <button class="swatch" style="--swatch: {a.color}" role="radio" aria-checked={prefs.values.accent === a.value}
                      aria-label={a.label} title={a.label} onclick={() => prefs.set('accent', a.value)}></button>
                  {/each}
                </div>
              </div>
              <div class="row">
                <div class="flex-1">
                  <div>Weniger Bewegung</div>
                  <div class="text-xs text-faint">Übergänge und Animationen entfallen</div>
                </div>
                <Toggle checked={prefs.values.reduceMotion} label="Weniger Bewegung" onchange={(v) => prefs.set('reduceMotion', v)} />
              </div>
              <div class="row">
                <div class="flex-1">
                  <div>Quick Capture</div>
                  {#if settings.quick_capture && status.capture_shortcut_blocked}
                    <div class="text-xs text-warn" role="alert">Eine andere App belegt <kbd class="kbd">⌥⇧Space</kbd>. Gib es dort frei, dann schalte das hier aus und wieder ein.</div>
                  {:else}
                    <div class="text-xs text-faint">Mit <kbd class="kbd">⌥⇧Space</kbd> von überall aus etwas festhalten</div>
                  {/if}
                </div>
                <Toggle checked={settings.quick_capture} label="Quick Capture" onchange={async (v) => { await save({ quick_capture: v }); status = await api.status(); }} />
              </div>
              <div class="row">
                <div class="flex-1">
                  <div>KI-Aktivität anzeigen</div>
                  <div class="text-xs text-faint">Einblendung und pulsierende Einträge, wenn eine KI etwas schreibt</div>
                </div>
                <Toggle checked={prefs.values.aiActivity} label="KI-Aktivität anzeigen" onchange={(v) => prefs.set('aiActivity', v)} />
              </div>
              <div class="row">
                <div class="flex-1">
                  <div>Neuerungen nach Updates zeigen</div>
                  <div class="text-xs text-faint">Die Liste bleibt in den Update-Einstellungen erreichbar</div>
                </div>
                <Toggle checked={prefs.values.whatsNew} label="Neuerungen nach Updates zeigen" onchange={(v) => prefs.set('whatsNew', v)} />
              </div>
            </div>
            <UpdateSettings {settings} onsave={save} />
            {#if !isDefault}
              <button class="btn-ghost mt-4" onclick={() => prefs.reset()}>Darstellung zurücksetzen</button>
            {/if}
          {:else if tab === 'graph'}
            <GraphSettings />
            {#if !isDefault}
              <button class="btn-ghost mt-4" onclick={() => prefs.reset()}>Darstellung zurücksetzen</button>
            {/if}
          {:else if tab === 'security'}
            <h2 class="mb-1 text-lg font-semibold">Sperren</h2>
            <p class="mb-5 text-muted">Gesperrt ist alles verschlüsselt, und auch KIs erhalten keinen Zugriff.</p>
            <div class="card">
              <div class="row">
                <div class="flex-1">
                  <div>Automatisch sperren</div>
                  <div class="text-xs text-faint">Nach dieser Zeit ohne Aktivität</div>
                </div>
                <Select value={settings.auto_lock_minutes} options={AUTO_LOCK} label="Automatisch sperren nach"
                  onchange={(m) => save({ auto_lock_minutes: m })} />
              </div>
              <div class="row">
                <div class="flex-1">
                  <div>Beim Ruhezustand sperren</div>
                  <div class="text-xs text-faint">Wenn der Mac schlafen geht</div>
                </div>
                <Toggle checked={settings.lock_on_sleep} label="Beim Ruhezustand sperren" onchange={(v) => save({ lock_on_sleep: v })} />
              </div>
              <div class="row">
                <div class="flex-1">
                  <div>Bei Bildschirmsperre sperren</div>
                  <div class="text-xs text-faint">Auch wenn du kurz ⌃⌘Q drückst</div>
                </div>
                <Toggle checked={settings.lock_on_screen_lock} label="Bei Bildschirmsperre sperren" onchange={(v) => save({ lock_on_screen_lock: v })} />
              </div>
            </div>

            <h3 class="section-title mt-7">Entsperren</h3>
            <div class="card">
              <div class="row">
                <div class="flex-1">
                  <div>Touch ID</div>
                  <div class="text-xs text-faint">
                    {status.biometry_available ? 'Statt des Passworts deinen Finger verwenden' : 'Auf diesem Mac nicht verfügbar'}
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
            <h2 class="mb-1 text-lg font-semibold">Passwort & Schlüssel</h2>
            <p class="mb-5 text-muted">Beide Aktionen verlangen dein aktuelles Passwort.</p>

            <form class="card p-4" onsubmit={changePassword}>
              <div class="mb-3 font-medium">Master-Passwort ändern</div>
              <div class="flex flex-col gap-2">
                <input class="field" type="password" placeholder="Aktuelles Passwort" bind:value={current} autocomplete="current-password" />
                <div class="grid grid-cols-2 gap-2">
                  <input class="field" type="password" placeholder="Neues Passwort" bind:value={next} autocomplete="new-password" />
                  <input class="field" type="password" placeholder="Wiederholen" bind:value={confirm} autocomplete="new-password" />
                </div>
              </div>
              <div class="mt-3 flex justify-end">
                <button class="btn-primary" disabled={busy || !current || next.length < 8}>Passwort ändern</button>
              </div>
            </form>

            <div class="card mt-4 p-4">
              <div class="mb-1 flex items-center gap-2 font-medium"><ShieldCheck size={15} class="text-ok" /> Wiederherstellungsschlüssel</div>
              {#if newRecovery}
                <p class="mb-3 text-xs text-muted">Nur jetzt sichtbar. Der alte Schlüssel gilt ab sofort nicht mehr.</p>
                <div class="selectable mb-3 rounded-lg border border-line bg-bg/60 p-3 text-center font-mono text-[14px] tracking-wider" transition:pop>{newRecovery}</div>
                <div class="flex gap-2">
                  <button class="btn-ghost flex-1" onclick={copy}>
                    {#if copied}<Check size={14} class="text-ok" /> Kopiert{:else}<Copy size={14} /> Kopieren{/if}
                  </button>
                  <button class="btn-primary flex-1" onclick={() => (newRecovery = '')}>Gesichert</button>
                </div>
              {:else}
                <p class="mb-3 text-xs text-muted">Erstellt einen neuen Schlüssel und macht den alten ungültig, etwa wenn er in falsche Hände geraten sein könnte.</p>
                <form class="flex gap-2" onsubmit={rotateRecovery}>
                  <input class="field flex-1" type="password" placeholder="Aktuelles Passwort" bind:value={recoveryPassword} />
                  <button class="btn-ghost" disabled={busy || !recoveryPassword}>Neu erstellen</button>
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
