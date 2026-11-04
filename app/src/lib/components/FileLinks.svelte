<script lang="ts">
  import { t } from '#lib/i18n.ts';
  import { Bot, ExternalLink, File as FileIcon, Folder, FolderSearch, ShieldCheck, X } from '@lucide/svelte';
  import { api, errorMessage, type FileLinkStatus, type Id } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';

  let { node }: { node: Id } = $props();

  let links = $state.raw<FileLinkStatus[]>([]);

  const load = async () => {
    links = await api.fileLinks(node).catch(() => []);
  };

  $effect(() => {
    void ws.revision;
    load();
  });

  async function act(fn: () => Promise<unknown>) {
    try {
      await fn();
    } catch (e) {
      ws.notify(errorMessage(e));
    }
    await load();
  }

  /** Asks for files or folders on the Mac. The entry's "add" row calls this. */
  export function link(folders: boolean) {
    return act(() => api.linkFiles(node, folders));
  }
</script>

{#if links.length}
  <div class="mt-7 mb-1 flex h-6 items-center">
    <div class="section-title !mb-0 flex-1">{t('Verknüpfte Dateien', 'Linked files')}</div>
  </div>
{/if}

{#each links as l (l.id)}
  <div class="group rounded-lg px-2 py-1.5 transition-colors hover:bg-elevated" in:pop={{ duration: 160, y: 3 }}>
    <div class="flex items-center gap-2">
      {#if l.is_dir}<Folder size={14} class="shrink-0 text-faint" />{:else}<FileIcon size={14} class="shrink-0 text-faint" />{/if}
      <div class="min-w-0 flex-1">
        <div class="truncate {l.exists ? '' : 'text-faint line-through'}">{l.label}</div>
        <div class="selectable truncate text-[11px] text-faint" title={l.path}>{l.path}</div>
      </div>
      {#if !l.exists}
        <span class="chip shrink-0 !border-warn/30 !bg-warn/10 !text-warn">{t('fehlt', 'missing')}</span>
      {/if}
      <div class="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
        {#if l.approved_for_ai}
          <button class="icon-btn !h-6 !w-6 !text-ai" aria-label={t(`„${l.label}“ für KIs sperren`, `Hide “${l.label}” from AI clients`)} title={t('KIs dürfen die Datei lesen. Klicken zum Sperren.', 'AI clients may read the file. Click to hide it.')}
            onclick={() => act(() => api.setFileLinkApproved(l.id, false))}><Bot size={13} /></button>
          <button class="icon-btn !h-6 !w-6" aria-label={t(`„${l.label}“ öffnen`, `Open “${l.label}”`)} title={t('Öffnen', 'Open')} disabled={!l.exists} onclick={() => act(() => api.openFile(l.id))}><ExternalLink size={13} /></button>
        {/if}
        <button class="icon-btn !h-6 !w-6" aria-label={t(`„${l.label}“ im Finder zeigen`, `Show “${l.label}” in Finder`)} title={t('Im Finder zeigen', 'Show in Finder')} disabled={!l.exists} onclick={() => act(() => api.revealFile(l.id))}><FolderSearch size={13} /></button>
        <button class="icon-btn !h-6 !w-6 hover:!text-danger" aria-label={t(`Verknüpfung „${l.label}“ entfernen`, `Remove link to “${l.label}”`)} title={t('Verknüpfung entfernen', 'Remove link')} onclick={() => act(() => api.deleteFileLink(l.id))}><X size={13} /></button>
      </div>
    </div>
    {#if !l.approved_for_ai}
      <div class="mt-1.5 flex items-center gap-2 rounded-md border border-ai/25 bg-ai/8 px-2 py-1.5 text-[11px] text-muted">
        <Bot size={12} class="shrink-0 text-ai" />
        <span class="min-w-0 flex-1">{t('KIs dürfen diese Datei nicht lesen.', 'AI clients may not read this file.')}</span>
        <button class="flex shrink-0 items-center gap-1 font-medium text-ai transition-opacity hover:opacity-80" onclick={() => act(() => api.setFileLinkApproved(l.id, true))}>
          <ShieldCheck size={12} /> {t('Freigeben', 'Allow')}
        </button>
      </div>
    {/if}
  </div>
{/each}
