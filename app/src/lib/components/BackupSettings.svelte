<script lang="ts">
  import { onMount } from 'svelte';
  import { flip } from 'svelte/animate';
  import { Archive, CircleCheck, FileDown, FolderInput, History, LoaderCircle, RefreshCw, TriangleAlert, Undo2 } from '@lucide/svelte';
  import { api, errorMessage, type BackupStatus, type ExportSummary, type ImportSkipReason, type ImportSummary, type Settings } from '#lib/api.ts';
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

  /** The folder chosen, before or after its import. */
  let preview = $state<ImportSummary | null>(null);
  let imported = $state<ImportSummary | null>(null);
  let importing = $state(false);
  let undone = $state(false);
  let importError = $state<string | null>(null);

  async function importStep(fn: () => Promise<void>) {
    busy = true;
    importError = null;
    try {
      await fn();
    } catch (err) {
      importError = errorMessage(err);
    } finally {
      busy = false;
      importing = false;
    }
  }

  const chooseFolder = () =>
    importStep(async () => {
      const summary = await api.importPreview();
      // Nothing back means the folder dialog was cancelled.
      if (summary) [preview, imported, undone] = [summary, null, false];
    });

  const runImport = () =>
    importStep(async () => {
      importing = true;
      imported = await api.importMarkdown();
      preview = null;
    });

  const undoImport = () =>
    importStep(async () => {
      await api.revert(imported!.change_set!);
      undone = true;
    });

  const count = (n: number, one: string, many: string) => (n === 1 ? `1 ${one}` : `${n.toLocaleString('de-DE')} ${many}`);
  function coming(p: ImportSummary) {
    const parts = [count(p.entries, 'neuer Eintrag', 'neue Einträge')];
    if (p.projects) parts.push(count(p.projects, 'neues Projekt', 'neue Projekte'));
    if (p.links) parts.push(count(p.links, 'Verbindung aus dem Export', 'Verbindungen aus dem Export'));
    const list = parts.length > 1 ? `${parts.slice(0, -1).join(', ')} und ${parts.at(-1)}` : parts[0];
    return `Aus „${p.folder}“ ${p.entries === 1 && parts.length === 1 ? 'kommt' : 'kommen'} ${list}.`;
  }
  const REASONS: Record<ImportSkipReason, string> = {
    too_large: 'zu groß',
    not_text: 'kein lesbarer Text',
    too_many: 'mehr als 50.000 Dateien',
    same_title: 'Titel doppelt im Ordner',
    long_title: 'Titel zu lang',
  };

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

<h3 class="section-title mt-7">Importieren</h3>
<div class="card">
  <div class="row">
    <FolderInput size={15} class="shrink-0 text-faint" />
    <div class="min-w-0 flex-1">
      <div>Markdown importieren</div>
      <div class="text-xs text-faint">Ein Ordner aus Obsidian, Logseq oder einem Export von Brainmatter. Ordner werden Projekte, Front Matter wird zu Tags und Art, Links zwischen den Dateien zu Verbindungen.</div>
    </div>
    <button class="btn-ghost !h-7 shrink-0 text-xs" aria-label="Ordner zum Importieren wählen" disabled={busy} onclick={chooseFolder}>Ordner wählen</button>
  </div>
  {#if importError}
    <div class="border-t border-line-soft px-4 py-3 text-xs text-danger" role="alert" in:pop={{ duration: 160, y: -3 }}>{importError}</div>
  {/if}
  {#if preview}
    <div class="border-t border-line-soft px-4 py-3 text-xs" in:pop={{ duration: 160, y: -3 }}>
      {#if preview.entries}
        <p class="leading-relaxed">{coming(preview)}</p>
      {:else}
        <p class="leading-relaxed">In „{preview.folder}“ ist nichts Neues.</p>
      {/if}
      {#if preview.existing}
        <p class="mt-1 text-faint">{count(preview.existing, 'Eintrag gibt', 'Einträge gibt')} es schon. {preview.existing === 1 ? 'Er bleibt' : 'Sie bleiben'}, wie {preview.existing === 1 ? 'er ist' : 'sie sind'}.</p>
      {/if}
      {#if preview.skipped_count}
        <details class="mt-1 text-warn">
          <summary class="cursor-default">{count(preview.skipped_count, 'Datei bleibt', 'Dateien bleiben')} draußen</summary>
          <ul class="selectable mt-1 space-y-0.5 text-faint">
            {#each preview.skipped as file (file.path)}
              <li class="break-all">{file.path} <span class="text-muted">· {REASONS[file.reason]}</span></li>
            {/each}
            {#if preview.skipped_count > preview.skipped.length}
              <li>und {(preview.skipped_count - preview.skipped.length).toLocaleString('de-DE')} weitere</li>
            {/if}
          </ul>
        </details>
      {/if}
      <div class="mt-3 flex justify-end gap-2">
        <button class="btn-ghost !h-7 text-xs" disabled={busy} onclick={() => (preview = null)}>Abbrechen</button>
        {#if preview.entries}
          <button class="btn-primary !h-7 text-xs" disabled={busy} onclick={runImport}>
            {#if importing}<LoaderCircle size={13} class="animate-spin" /> Importiere …{:else}Importieren{/if}
          </button>
        {/if}
      </div>
    </div>
  {/if}
  {#if imported}
    <div class="flex items-center gap-2 border-t border-line-soft px-4 py-3 text-xs" in:pop={{ duration: 160, y: -3 }}>
      {#if undone}
        <span class="flex-1 text-muted">Import von „{imported.folder}“ rückgängig gemacht.</span>
      {:else}
        <span class="flex flex-1 items-center gap-1.5 text-ok"><CircleCheck size={13} /> {count(imported.entries, 'Eintrag', 'Einträge')} aus „{imported.folder}“ importiert</span>
        {#if imported.change_set}
          <button class="btn-ghost !h-7 shrink-0 text-xs" disabled={busy} onclick={undoImport}><Undo2 size={13} /> Rückgängig</button>
        {/if}
      {/if}
    </div>
  {/if}
</div>
