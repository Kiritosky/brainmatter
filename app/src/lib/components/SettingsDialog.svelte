<script lang="ts">
  import { onMount } from 'svelte';
  import { X, Copy } from '@lucide/svelte';
  import { api, errorMessage, type Settings, type Status } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';

  let status = $state<Status | null>(null);
  let settings = $state<Settings | null>(null);
  let message = $state<{ text: string; ok: boolean } | null>(null);
  let busy = $state(false);

  let current = $state('');
  let next = $state('');
  let confirm = $state('');
  let recoveryPassword = $state('');
  let newRecovery = $state('');

  const AUTO_LOCK = [5, 15, 30, 60, 120, 240, 480];

  onMount(async () => {
    status = await api.status();
    settings = status.settings;
  });

  async function run(fn: () => Promise<unknown>, success: string) {
    busy = true;
    message = null;
    try {
      await fn();
      message = { text: success, ok: true };
    } catch (e) {
      message = { text: errorMessage(e), ok: false };
    } finally {
      busy = false;
    }
  }

  async function save(patch: Partial<Settings>) {
    if (!settings) return;
    await run(async () => (settings = await api.updateSettings({ ...settings!, ...patch })), 'Gespeichert.');
  }

  async function toggleTouchId() {
    if (!status) return;
    await run(
      () => (status!.touch_id_enrolled ? api.disableTouchId() : api.enrollTouchId()),
      status.touch_id_enrolled ? 'Touch ID deaktiviert.' : 'Touch ID aktiviert.',
    );
    status = await api.status();
  }

  async function changePassword(e: SubmitEvent) {
    e.preventDefault();
    if (next !== confirm) {
      message = { text: 'Die neuen Passwörter stimmen nicht überein.', ok: false };
      return;
    }
    await run(() => api.changePassword(current, next), 'Passwort geändert.');
    if (message?.ok) current = next = confirm = '';
  }

  async function rotateRecovery(e: SubmitEvent) {
    e.preventDefault();
    await run(async () => (newRecovery = await api.rotateRecoveryKey(recoveryPassword)), 'Neuer Schlüssel erstellt. Der alte gilt nicht mehr.');
    recoveryPassword = '';
  }
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/40" role="presentation"
  onclick={() => (ws.settingsOpen = false)}>
  <div class="max-h-[85vh] w-[520px] overflow-y-auto rounded-xl border border-line bg-panel shadow-2xl" role="dialog" tabindex="-1"
    onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && (ws.settingsOpen = false)}>
    <header class="flex items-center border-b border-line px-5 py-3">
      <h2 class="flex-1 text-[15px] font-semibold">Einstellungen</h2>
      <button class="icon-btn" onclick={() => (ws.settingsOpen = false)}><X size={15} /></button>
    </header>

    {#if settings && status}
      <section class="border-b border-line px-5 py-4">
        <div class="section-title">Sperren</div>
        <label class="flex items-center justify-between py-1.5">
          <span>Automatisch sperren nach</span>
          <select class="field !py-1" value={settings.auto_lock_minutes}
            onchange={(e) => save({ auto_lock_minutes: Number((e.currentTarget as HTMLSelectElement).value) })}>
            {#each AUTO_LOCK as m}<option value={m}>{m < 60 ? `${m} Minuten` : `${m / 60} Std.`}</option>{/each}
          </select>
        </label>
        <label class="flex items-center justify-between py-1.5">
          <span>Beim Ruhezustand sperren</span>
          <input type="checkbox" checked={settings.lock_on_sleep} onchange={(e) => save({ lock_on_sleep: (e.currentTarget as HTMLInputElement).checked })} />
        </label>
        <label class="flex items-center justify-between py-1.5">
          <span>Bei Bildschirmsperre sperren</span>
          <input type="checkbox" checked={settings.lock_on_screen_lock} onchange={(e) => save({ lock_on_screen_lock: (e.currentTarget as HTMLInputElement).checked })} />
        </label>
        {#if status.biometry_available}
          <label class="flex items-center justify-between py-1.5">
            <span>Mit Touch ID entsperren</span>
            <input type="checkbox" checked={status.touch_id_enrolled} disabled={busy} onchange={toggleTouchId} />
          </label>
        {/if}
        <p class="mt-1 text-xs text-faint">Gesperrt sind alle Daten verschlüsselt, und auch KIs erhalten keinen Zugriff.</p>
      </section>

      <section class="border-b border-line px-5 py-4">
        <div class="section-title">Passwort ändern</div>
        <form class="flex flex-col gap-2" onsubmit={changePassword}>
          <input class="field" type="password" placeholder="Aktuelles Passwort" bind:value={current} autocomplete="current-password" />
          <input class="field" type="password" placeholder="Neues Passwort (mind. 8 Zeichen)" bind:value={next} autocomplete="new-password" />
          <input class="field" type="password" placeholder="Neues Passwort wiederholen" bind:value={confirm} autocomplete="new-password" />
          <button class="btn-ghost self-end" disabled={busy || !current || next.length < 8}>Passwort ändern</button>
        </form>
      </section>

      <section class="px-5 py-4">
        <div class="section-title">Wiederherstellungsschlüssel</div>
        {#if newRecovery}
          <div class="selectable mb-2 rounded-lg border border-line bg-elevated p-3 text-center font-mono text-[14px] tracking-wide">{newRecovery}</div>
          <div class="flex gap-2">
            <button class="btn-ghost flex-1" onclick={() => navigator.clipboard.writeText(newRecovery)}><Copy size={14} /> Kopieren</button>
            <button class="btn-primary flex-1" onclick={() => (newRecovery = '')}>Gesichert</button>
          </div>
        {:else}
          <p class="mb-2 text-xs text-muted">Erstellt einen neuen Schlüssel und macht den alten ungültig, z. B. wenn er in falsche Hände geraten sein könnte.</p>
          <form class="flex gap-2" onsubmit={rotateRecovery}>
            <input class="field flex-1" type="password" placeholder="Aktuelles Passwort" bind:value={recoveryPassword} />
            <button class="btn-ghost" disabled={busy || !recoveryPassword}>Neu erstellen</button>
          </form>
        {/if}
      </section>

      {#if message}
        <div class="border-t border-line px-5 py-2.5 text-sm {message.ok ? 'text-ok' : 'text-danger'}">{message.text}</div>
      {/if}
    {/if}
  </div>
</div>
