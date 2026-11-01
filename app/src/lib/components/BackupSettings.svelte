<script lang="ts">
  import { onMount } from 'svelte';
  import { flip } from 'svelte/animate';
  import { Archive, CircleCheck, FileDown, History, RefreshCw, TriangleAlert } from '@lucide/svelte';
  import { api, errorMessage, type BackupStatus, type ExportSummary, type Settings } from '#lib/api.ts';
  import { formatSize } from '#lib/attachments.ts';
  import Toggle from '#lib/ui/Toggle.svelte';
  import { pop } from '#lib/ui/motion.ts';

  let { settings, onsave }: { settings: Settings; onsave: (patch: Partial<Settings>) => void } = $props();

  let status = $state<BackupStatus | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** The backup whose restore is being confirmed. */
  let restoring = $state<string | null>(null);
  let password = $state('');

  const load = async () => (status = await api.backupStatus());
  onMount(load);

  async function run(fn: () => Promise<unknown>) {
    busy = true;
    error = null;
    try {
      await fn();
      await load();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  async function restore(e: SubmitEvent) {
    e.preventDefault();
    const name = restoring!;
    // On success the session locks and this page goes away with the workspace.
    await run(() => api.restoreBackup(name, password));
    password = '';
  }

  let exporting = $state(false);
  let exportPassword = $state('');
  let exportError = $state<string | null>(null);
  let exported = $state<ExportSummary | null>(null);

  async function runExport(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    exportError = null;
    try {
      const summary = await api.exportMarkdown(exportPassword);
      // Nothing back means the folder dialog was cancelled; the form stays for another try.
      if (summary) {
        exported = summary;
        exporting = false;
        exportPassword = '';
      }
    } catch (err) {
      exportError = errorMessage(err);
    } finally {
      busy = false;
    }
  }

  const when = (ms: number) =>
    new Date(ms).toLocaleString('de-DE', { weekday: 'short', day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' });
</script>

<h2 class="mb-1 text-lg font-semibold">Backups</h2>
<p class="mb-5 text-muted">Verschlüsselte Sicherungen deiner Einträge. Sie liegen neben dem Tresor und schützen vor Bedienfehlern und beschädigten Daten, nicht vor dem Verlust des Macs.</p>

<div class="card">
  <div class="row">
    <div class="flex-1">
      <div>Automatisch sichern</div>
      <div class="text-xs text-faint">Täglich, wenn sich etwas geändert hat. Bleiben: 7 Tage und 4 Wochen.</div>
    </div>
    <Toggle checked={settings.auto_backup} label="Automatisch sichern" onchange={(v) => onsave({ auto_backup: v })} />
  </div>
</div>

<div class="mt-7 mb-1.5 flex items-center">
  <h3 class="section-title !mb-0 flex-1">Vorhandene Backups</h3>
  <button class="btn-ghost !h-7 text-xs" disabled={busy || status?.running} onclick={() => run(api.backupNow)}>
    <RefreshCw size={13} class={busy || status?.running ? 'animate-spin' : ''} /> Jetzt sichern
  </button>
</div>

{#if error || status?.error}
  <div class="mb-2 flex items-start gap-2 rounded-lg border border-danger/30 bg-danger/10 px-3 py-2 text-xs text-danger" in:pop={{ duration: 160, y: 3 }}>
    <TriangleAlert size={14} class="mt-px shrink-0" /> <span>{error ?? status?.error}</span>
  </div>
{/if}

{#if status}
  <div class="card">
    {#each status.backups as b (b.name)}
      <div animate:flip={{ duration: 180 }}>
        <div class="row">
          <Archive size={15} class="shrink-0 text-faint" />
          <div class="min-w-0 flex-1">
            <div class="tabular-nums">{when(b.at)}</div>
            <div class="text-xs text-faint tabular-nums">{formatSize(b.size)}</div>
          </div>
          <button class="btn-ghost !h-7 text-xs" onclick={() => ((restoring = restoring === b.name ? null : b.name), (password = ''), (error = null))}>
            <History size={13} /> Wiederherstellen
          </button>
        </div>
        {#if restoring === b.name}
          <form class="border-t border-line-soft px-4 py-3" onsubmit={restore} in:pop={{ duration: 160, y: -3 }}>
            <p class="mb-2.5 text-xs leading-relaxed text-muted">
              Alles wird auf diesen Stand zurückgesetzt, und Brainmatter sperrt sich danach. Der jetzige Stand wird vorher als eigenes Backup gesichert, du kannst also zurück.
            </p>
            <div class="flex gap-2">
              <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
              <input class="field flex-1" type="password" placeholder="Master-Passwort" aria-label="Master-Passwort" bind:value={password} autofocus />
              <button class="btn-danger" disabled={busy || !password}>Zurücksetzen</button>
            </div>
          </form>
        {/if}
      </div>
    {:else}
      <div class="px-4 py-7 text-center text-faint">Noch keine Backups. Das erste entsteht automatisch, sobald du etwas änderst.</div>
    {/each}
  </div>
{/if}

<h3 class="section-title mt-7">Exportieren</h3>
<div class="card">
  <div class="row">
    <FileDown size={15} class="shrink-0 text-faint" />
    <div class="min-w-0 flex-1">
      <div>Als Markdown exportieren</div>
      <div class="text-xs text-faint">Eine Datei pro Eintrag in Ordnern nach Projekt, dazu alle Anhänge. Der Papierkorb bleibt draußen.</div>
    </div>
    <button class="btn-ghost !h-7 shrink-0 text-xs" onclick={() => ((exporting = !exporting), (exportPassword = ''), (exportError = null), (exported = null))}>Exportieren</button>
  </div>
  {#if exporting}
    <form class="border-t border-line-soft px-4 py-3" onsubmit={runExport} in:pop={{ duration: 160, y: -3 }}>
      <p class="mb-2.5 text-xs leading-relaxed text-warn">
        <TriangleAlert size={13} class="mr-1 inline align-[-2px]" />Der Export ist nicht verschlüsselt. Jeder mit Zugriff auf den Ordner kann alles lesen.
      </p>
      {#if exportError}
        <p class="mb-2.5 text-xs text-danger" role="alert">{exportError}</p>
      {/if}
      <div class="flex gap-2">
        <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
        <input class="field flex-1" type="password" placeholder="Master-Passwort" aria-label="Master-Passwort für den Export" bind:value={exportPassword} autofocus />
        <button class="btn-primary shrink-0" disabled={busy || !exportPassword}>Ordner wählen</button>
      </div>
    </form>
  {/if}
  {#if exported}
    <div class="border-t border-line-soft px-4 py-3 text-xs" in:pop={{ duration: 160, y: -3 }}>
      <div class="flex items-center gap-1.5 text-ok"><CircleCheck size={13} /> {exported.entries} Einträge und {exported.attachments} Anhänge exportiert</div>
      <div class="selectable mt-1 break-all text-faint">{exported.folder}</div>
      {#if exported.failed_attachments.length}
        <div class="mt-1 text-warn">Nicht lesbar: {exported.failed_attachments.join(', ')}</div>
      {/if}
    </div>
  {/if}
</div>
