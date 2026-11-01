<script lang="ts">
  import { Bot, ExternalLink, File as FileIcon, FilePlus, Folder, FolderPlus, FolderSearch, ShieldCheck, X } from '@lucide/svelte';
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
</script>

<div class="mt-7 mb-1 flex h-6 items-center">
  <div class="section-title !mb-0 flex-1">Verknüpfte Dateien</div>
  <button class="icon-btn !h-6 !w-6" aria-label="Datei verknüpfen" title="Datei verknüpfen" onclick={() => act(() => api.linkFiles(node, false))}><FilePlus size={13} /></button>
  <button class="icon-btn !h-6 !w-6" aria-label="Ordner verknüpfen" title="Ordner verknüpfen" onclick={() => act(() => api.linkFiles(node, true))}><FolderPlus size={13} /></button>
</div>

{#each links as l (l.id)}
  <div class="group rounded-lg px-2 py-1.5 transition-colors hover:bg-elevated" in:pop={{ duration: 160, y: 3 }}>
    <div class="flex items-center gap-2">
      {#if l.is_dir}<Folder size={14} class="shrink-0 text-faint" />{:else}<FileIcon size={14} class="shrink-0 text-faint" />{/if}
      <div class="min-w-0 flex-1">
        <div class="truncate {l.exists ? '' : 'text-faint line-through'}">{l.label}</div>
        <div class="selectable truncate text-[11px] text-faint" title={l.path}>{l.path}</div>
      </div>
      {#if !l.exists}
        <span class="chip shrink-0 !border-warn/30 !bg-warn/10 !text-warn">fehlt</span>
      {/if}
      <div class="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
        {#if l.approved_for_ai}
          <button class="icon-btn !h-6 !w-6 !text-ai" aria-label="„{l.label}“ für KIs sperren" title="KIs dürfen die Datei lesen. Klicken zum Sperren."
            onclick={() => act(() => api.setFileLinkApproved(l.id, false))}><Bot size={13} /></button>
          <button class="icon-btn !h-6 !w-6" aria-label="„{l.label}“ öffnen" title="Öffnen" disabled={!l.exists} onclick={() => act(() => api.openFile(l.id))}><ExternalLink size={13} /></button>
        {/if}
        <button class="icon-btn !h-6 !w-6" aria-label="„{l.label}“ im Finder zeigen" title="Im Finder zeigen" disabled={!l.exists} onclick={() => act(() => api.revealFile(l.id))}><FolderSearch size={13} /></button>
        <button class="icon-btn !h-6 !w-6 hover:!text-danger" aria-label="Verknüpfung „{l.label}“ entfernen" title="Verknüpfung entfernen" onclick={() => act(() => api.deleteFileLink(l.id))}><X size={13} /></button>
      </div>
    </div>
    {#if !l.approved_for_ai}
      <div class="mt-1.5 flex items-center gap-2 rounded-md border border-ai/25 bg-ai/8 px-2 py-1.5 text-[11px] text-muted">
        <Bot size={12} class="shrink-0 text-ai" />
        <span class="min-w-0 flex-1">KIs dürfen diese Datei nicht lesen.</span>
        <button class="flex shrink-0 items-center gap-1 font-medium text-ai transition-opacity hover:opacity-80" onclick={() => act(() => api.setFileLinkApproved(l.id, true))}>
          <ShieldCheck size={12} /> Freigeben
        </button>
      </div>
    {/if}
  </div>
{:else}
  <p class="px-1 text-xs leading-relaxed text-faint">Verweist auf Dateien oder Ordner auf deinem Mac, ohne sie zu kopieren. Auch nach dem Verschieben bleibt der Link gültig.</p>
{/each}
