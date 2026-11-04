<script lang="ts">
  import { locale, num, plural, t } from '#lib/i18n.ts';
  import { onMount } from 'svelte';
  import { flip } from 'svelte/animate';
  import { Archive, CircleCheck, FileDown, FolderInput, HardDrive, History, LoaderCircle, RefreshCw, TriangleAlert, Undo2 } from '@lucide/svelte';
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

  let copyBusy = $state(false);
  let copyError = $state<string | null>(null);
  async function chooseCopy() {
    copyBusy = true;
    copyError = null;
    try {
      // Nothing back means the folder dialog was cancelled.
      status = (await api.chooseBackupCopy()) ?? status;
    } catch (err) {
      copyError = errorMessage(err);
    } finally {
      copyBusy = false;
    }
  }
  async function removeCopy() {
    copyError = null;
    status = await api.removeBackupCopy();
  }
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

  function coming(p: ImportSummary) {
    const parts = [plural(p.entries, ['neuer Eintrag', 'neue Einträge'], ['new entry', 'new entries'])];
    if (p.projects) parts.push(plural(p.projects, ['neues Projekt', 'neue Projekte'], ['new project', 'new projects']));
    if (p.links) parts.push(plural(p.links, ['Verbindung aus dem Export', 'Verbindungen aus dem Export'], ['link from the export', 'links from the export']));
    const list = parts.length > 1 ? `${parts.slice(0, -1).join(', ')} ${t('und', 'and')} ${parts.at(-1)}` : parts[0];
    return t(
      `Aus „${p.folder}“ ${p.entries === 1 && parts.length === 1 ? 'kommt' : 'kommen'} ${list}.`,
      `“${p.folder}” brings ${list}.`,
    );
  }
  const REASONS: Record<ImportSkipReason, string> = {
    too_large: t('zu groß', 'too large'),
    not_text: t('kein lesbarer Text', 'no readable text'),
    too_many: t(`mehr als ${num(50_000)} Dateien`, `more than ${num(50_000)} files`),
    same_title: t('Titel doppelt im Ordner', 'title twice in the folder'),
    long_title: t('Titel zu lang', 'title too long'),
  };

  const when = (ms: number) =>
    new Date(ms).toLocaleString(locale, { weekday: 'short', day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' });
</script>

<h2 class="mb-1 text-lg font-semibold">Backups</h2>
<p class="mb-5 text-muted">
  {t(
    'Verschlüsselte Sicherungen deiner Einträge. Sie liegen neben dem Tresor und schützen vor Bedienfehlern und beschädigten Daten. Gegen den Verlust des Macs hilft eine Kopie an einem zweiten Ort.',
    'Encrypted copies of your entries. They sit next to the vault and protect against mistakes and damaged data. Against losing the Mac, a copy in a second place helps.',
  )}
</p>

<div class="card">
  <div class="row">
    <div class="flex-1">
      <div>{t('Automatisch sichern', 'Back up automatically')}</div>
      <div class="text-xs text-faint">{t('Täglich, wenn sich etwas geändert hat. Bleiben: 7 Tage und 4 Wochen.', 'Daily, when something has changed. Kept: 7 days and 4 weeks.')}</div>
    </div>
    <Toggle checked={settings.auto_backup} label={t('Automatisch sichern', 'Back up automatically')} onchange={(v) => onsave({ auto_backup: v })} />
  </div>
  <div class="row border-t border-line-soft">
    <HardDrive size={15} class="shrink-0 text-faint" />
    <div class="min-w-0 flex-1">
      <div>{t('Zweiter Ort', 'Second place')}</div>
      {#if status?.copy}
        {@const copy = status.copy}
        <div class="selectable truncate text-xs text-faint" title={copy.folder}>{copy.folder}</div>
        {#if copy.error}
          <div class="mt-0.5 text-xs text-danger">{copy.error}</div>
        {:else if !copy.reachable}
          <div class="mt-0.5 text-xs text-warn">{t('Nicht verbunden. Die Kopie wird nachgeholt, sobald der Ort wieder da ist.', 'Not connected. The copy catches up as soon as the place is back.')}</div>
        {:else if copy.newest}
          <div class="mt-0.5 text-xs text-faint tabular-nums">{t('Stand:', 'As of')} {when(copy.newest)}</div>
        {/if}
      {:else}
        <div class="text-xs text-faint">{t('Jedes Backup auch auf ein externes Laufwerk oder in einen anderen Ordner, mit allem, was ein neuer Mac zum Wiederherstellen braucht.', 'Every backup also on an external drive or in another folder, with all a new Mac needs to restore it.')}</div>
      {/if}
      {#if copyError}
        <div class="mt-0.5 text-xs text-danger" role="alert" in:pop={{ duration: 160, y: -3 }}>{copyError}</div>
      {/if}
    </div>
    {#if status?.copy}
      <button class="btn-ghost !h-7 shrink-0 text-xs" disabled={copyBusy} onclick={removeCopy}>{t('Entfernen', 'Remove')}</button>
    {/if}
    <button class="btn-ghost !h-7 shrink-0 text-xs" aria-label={t('Zweiten Ort für Backups wählen', 'Choose a second place for backups')} disabled={copyBusy || status?.running} onclick={chooseCopy}>
      {#if copyBusy}<LoaderCircle size={13} class="animate-spin" /> {t('Kopiere …', 'Copying …')}{:else}{status?.copy ? t('Ändern', 'Change') : t('Ort wählen', 'Choose place')}{/if}
    </button>
  </div>
</div>

<div class="mt-7 mb-1.5 flex items-center">
  <h3 class="section-title !mb-0 flex-1">{t('Vorhandene Backups', 'Backups')}</h3>
  <button class="btn-ghost !h-7 text-xs" disabled={busy || status?.running} onclick={() => run(api.backupNow)}>
    <RefreshCw size={13} class={busy || status?.running ? 'animate-spin' : ''} /> {t('Jetzt sichern', 'Back up now')}
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
            <History size={13} /> {t('Wiederherstellen', 'Restore')}
          </button>
        </div>
        {#if restoring === b.name}
          <form class="border-t border-line-soft px-4 py-3" onsubmit={restore} in:pop={{ duration: 160, y: -3 }}>
            <p class="mb-2.5 text-xs leading-relaxed text-muted">
              {t(
                'Alles wird auf diesen Stand zurückgesetzt, und Brainmatter sperrt sich danach. Der jetzige Stand wird vorher als eigenes Backup gesichert, du kannst also zurück.',
                'Everything goes back to this point, and Brainmatter locks afterwards. The current state is saved as a backup of its own first, so you can go back.',
              )}
            </p>
            <div class="flex gap-2">
              <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
              <input class="field flex-1" type="password" placeholder={t('Master-Passwort', 'Master password')} aria-label={t('Master-Passwort', 'Master password')} bind:value={password} autofocus />
              <button class="btn-danger" disabled={busy || !password}>{t('Zurücksetzen', 'Restore')}</button>
            </div>
          </form>
        {/if}
      </div>
    {:else}
      <div class="px-4 py-7 text-center text-faint">{t('Noch keine Backups. Das erste entsteht automatisch, sobald du etwas änderst.', 'No backups yet. The first one is made by itself once you change something.')}</div>
    {/each}
  </div>
{/if}

<h3 class="section-title mt-7">{t('Exportieren', 'Export')}</h3>
<div class="card">
  <div class="row">
    <FileDown size={15} class="shrink-0 text-faint" />
    <div class="min-w-0 flex-1">
      <div>{t('Als Markdown exportieren', 'Export as Markdown')}</div>
      <div class="text-xs text-faint">{t('Eine Datei pro Eintrag in Ordnern nach Projekt, dazu alle Anhänge. Der Papierkorb bleibt draußen.', 'One file per entry, in folders by project, plus all attachments. The trash stays out.')}</div>
    </div>
    <button class="btn-ghost !h-7 shrink-0 text-xs" onclick={() => ((exporting = !exporting), (exportPassword = ''), (exportError = null), (exported = null))}>{t('Exportieren', 'Export')}</button>
  </div>
  {#if exporting}
    <form class="border-t border-line-soft px-4 py-3" onsubmit={runExport} in:pop={{ duration: 160, y: -3 }}>
      <p class="mb-2.5 text-xs leading-relaxed text-warn">
        <TriangleAlert size={13} class="mr-1 inline align-[-2px]" />{t('Der Export ist nicht verschlüsselt. Jeder mit Zugriff auf den Ordner kann alles lesen.', 'The export is not encrypted. Anyone with access to the folder can read all of it.')}
      </p>
      {#if exportError}
        <p class="mb-2.5 text-xs text-danger" role="alert">{exportError}</p>
      {/if}
      <div class="flex gap-2">
        <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
        <input class="field flex-1" type="password" placeholder={t('Master-Passwort', 'Master password')} aria-label={t('Master-Passwort für den Export', 'Master password for the export')} bind:value={exportPassword} autofocus />
        <button class="btn-primary shrink-0" disabled={busy || !exportPassword}>{t('Ordner wählen', 'Choose folder')}</button>
      </div>
    </form>
  {/if}
  {#if exported}
    <div class="border-t border-line-soft px-4 py-3 text-xs" in:pop={{ duration: 160, y: -3 }}>
      <div class="flex items-center gap-1.5 text-ok"><CircleCheck size={13} /> {t(`${num(exported.entries)} Einträge und ${num(exported.attachments)} Anhänge exportiert`, `Exported ${num(exported.entries)} entries and ${num(exported.attachments)} attachments`)}</div>
      <div class="selectable mt-1 break-all text-faint">{exported.folder}</div>
      {#if exported.failed_attachments.length}
        <div class="mt-1 text-warn">{t('Nicht lesbar:', 'Could not read:')} {exported.failed_attachments.join(', ')}</div>
      {/if}
    </div>
  {/if}
</div>

<h3 class="section-title mt-7">{t('Importieren', 'Import')}</h3>
<div class="card">
  <div class="row">
    <FolderInput size={15} class="shrink-0 text-faint" />
    <div class="min-w-0 flex-1">
      <div>{t('Markdown importieren', 'Import Markdown')}</div>
      <div class="text-xs text-faint">
        {t(
          'Ein Ordner aus Obsidian, Logseq oder einem Export von Brainmatter. Ordner werden Projekte, Front Matter wird zu Tags und Art, Links zwischen den Dateien zu Verbindungen.',
          'A folder from Obsidian, Logseq or a Brainmatter export. Folders become projects, front matter becomes tags and kind, links between the files become links.',
        )}
      </div>
    </div>
    <button class="btn-ghost !h-7 shrink-0 text-xs" aria-label={t('Ordner zum Importieren wählen', 'Choose a folder to import')} disabled={busy} onclick={chooseFolder}>{t('Ordner wählen', 'Choose folder')}</button>
  </div>
  {#if importError}
    <div class="border-t border-line-soft px-4 py-3 text-xs text-danger" role="alert" in:pop={{ duration: 160, y: -3 }}>{importError}</div>
  {/if}
  {#if preview}
    <div class="border-t border-line-soft px-4 py-3 text-xs" in:pop={{ duration: 160, y: -3 }}>
      {#if preview.entries}
        <p class="leading-relaxed">{coming(preview)}</p>
      {:else}
        <p class="leading-relaxed">{t(`In „${preview.folder}“ ist nichts Neues.`, `Nothing new in “${preview.folder}”.`)}</p>
      {/if}
      {#if preview.existing}
        <p class="mt-1 text-faint">
          {preview.existing === 1
            ? t('1 Eintrag gibt es schon. Er bleibt, wie er ist.', '1 entry is already here. It stays as it is.')
            : t(`${num(preview.existing)} Einträge gibt es schon. Sie bleiben, wie sie sind.`, `${num(preview.existing)} entries are already here. They stay as they are.`)}
        </p>
      {/if}
      {#if preview.skipped_count}
        <details class="mt-1 text-warn">
          <summary class="cursor-default">{plural(preview.skipped_count, ['Datei bleibt', 'Dateien bleiben'], ['file stays', 'files stay'])} {t('draußen', 'out')}</summary>
          <ul class="selectable mt-1 space-y-0.5 text-faint">
            {#each preview.skipped as file (file.path)}
              <li class="break-all">{file.path} <span class="text-muted">· {REASONS[file.reason]}</span></li>
            {/each}
            {#if preview.skipped_count > preview.skipped.length}
              <li>{t(`und ${num(preview.skipped_count - preview.skipped.length)} weitere`, `and ${num(preview.skipped_count - preview.skipped.length)} more`)}</li>
            {/if}
          </ul>
        </details>
      {/if}
      <div class="mt-3 flex justify-end gap-2">
        <button class="btn-ghost !h-7 text-xs" disabled={busy} onclick={() => (preview = null)}>{t('Abbrechen', 'Cancel')}</button>
        {#if preview.entries}
          <button class="btn-primary !h-7 text-xs" disabled={busy} onclick={runImport}>
            {#if importing}<LoaderCircle size={13} class="animate-spin" /> {t('Importiere …', 'Importing …')}{:else}{t('Importieren', 'Import')}{/if}
          </button>
        {/if}
      </div>
    </div>
  {/if}
  {#if imported}
    <div class="flex items-center gap-2 border-t border-line-soft px-4 py-3 text-xs" in:pop={{ duration: 160, y: -3 }}>
      {#if undone}
        <span class="flex-1 text-muted">{t(`Import von „${imported.folder}“ rückgängig gemacht.`, `Undid the import of “${imported.folder}”.`)}</span>
      {:else}
        <span class="flex flex-1 items-center gap-1.5 text-ok"><CircleCheck size={13} />
          {t(
            `${plural(imported.entries, ['Eintrag', 'Einträge'], ['entry', 'entries'])} aus „${imported.folder}“ importiert`,
            `Imported ${plural(imported.entries, ['Eintrag', 'Einträge'], ['entry', 'entries'])} from “${imported.folder}”`,
          )}</span>
        {#if imported.change_set}
          <button class="btn-ghost !h-7 shrink-0 text-xs" disabled={busy} onclick={undoImport}><Undo2 size={13} /> {t('Rückgängig', 'Undo')}</button>
        {/if}
      {/if}
    </div>
  {/if}
</div>
