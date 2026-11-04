<script lang="ts">
  import { locale, t } from '#lib/i18n.ts';
  import { onDestroy, untrack } from 'svelte';
  import { ArrowLeft, ArrowRight, Star, Crosshair, File as FileIcon, FileImage, FileText, FilePlus, FolderPlus, GitMerge, Link2, Paperclip, Trash2, Upload, X, Pencil, Eye, History, Bot, Check, Plus, Undo2 } from '@lucide/svelte';
  import { api, errorMessage, TEMPLATE_KIND, type Attachment, type EdgeView, type NodeRevision, type Similar } from '#lib/api.ts';
  import { MAX_ATTACHMENT_BYTES, attachmentUrl, formatSize, pastedAttachmentMarkdown, pastedFile, previewKind } from '#lib/attachments.ts';
  import { renderMarkdown } from '#lib/markdown.ts';
  import { EntryDraft } from '#lib/autosave.svelte.ts';
  import ProjectPicker from '#lib/ui/ProjectPicker.svelte';
  import TagInput from '#lib/ui/TagInput.svelte';
  import { ws } from '#lib/workspace.svelte.ts';
  import AttachmentPreview from './AttachmentPreview.svelte';
  import FileLinks from './FileLinks.svelte';
  import MarkdownEditor from './MarkdownEditor.svelte';
  import TemplateUse from './TemplateUse.svelte';
  import { pop } from '#lib/ui/motion.ts';

  let pickingProject = $state(false);

  const detail = $derived(ws.detail!);
  const node = $derived(detail.node);

  let editing = $state(false);
  let confirmDelete = $state(false);
  let showHistory = $state(false);
  let history = $state<NodeRevision[]>([]);

  // The workspace creates one Inspector per entry ({#key}). Saving must not
  // read `node`: a pending save can run after ws.detail moved to another entry,
  // and would write this draft there.
  const nodeId = untrack(() => node.id);
  const draft = new EntryDraft(nodeId, (message) => ws.notify(message));
  const favorite = $derived(ws.favorites.some((f) => f.id === nodeId));

  untrack(() => {
    editing = node.body.trim() === '' && !detail.read_only;
  });

  $effect(() => {
    const n = node;
    untrack(() => draft.accept(n));
  });

  // Leaving the entry within the autosave delay must not lose the last edit.
  onDestroy(() => draft.save());

  const knownTitles = $derived(new Set(detail.outgoing.map((e) => e.other.title.toLowerCase())));
  const html = $derived(renderMarkdown(node.body, (title) => knownTitles.has(title.toLowerCase())));

  const groups = $derived.by(() => {
    const out = new Map<string, { out: boolean; relation: string; edges: EdgeView[] }>();
    const add = (isOut: boolean, e: EdgeView) => {
      const key = `${isOut ? 'out' : 'in'}:${e.edge.relation}`;
      if (!out.has(key)) out.set(key, { out: isOut, relation: e.edge.relation, edges: [] });
      out.get(key)!.edges.push(e);
    };
    detail.outgoing.forEach((e) => add(true, e));
    detail.incoming.forEach((e) => add(false, e));
    return [...out.entries()];
  });

  const RELATIONS: Record<string, [string, string]> = {
    mentions: [t('erwähnt', 'mentions'), t('erwähnt von', 'mentioned by')],
    related_to: [t('verwandt mit', 'related to'), t('verwandt mit', 'related to')],
    part_of: [t('Teil von', 'part of'), t('enthält', 'contains')],
    depends_on: [t('hängt ab von', 'depends on'), t('Grundlage für', 'basis for')],
    example_of: [t('Beispiel für', 'example of'), t('hat Beispiel', 'has example')],
    supports: [t('stützt', 'supports'), t('gestützt von', 'supported by')],
    contradicts: [t('widerspricht', 'contradicts'), t('Widerspruch von', 'contradicted by')],
    derived_from: [t('abgeleitet von', 'derived from'), t('Quelle für', 'source of')],
  };
  const relationLabel = (relation: string, out: boolean) =>
    RELATIONS[relation]?.[out ? 0 : 1] ?? relation.replaceAll('_', ' ');

  async function act(fn: () => Promise<unknown>) {
    try {
      await fn();
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  /** Merging is one click away from the list, so it asks once. */
  let confirmMerge = $state<string | null>(null);

  async function merge(s: Similar) {
    await draft.save();
    await act(async () => {
      await api.mergeNodes(nodeId, [s.node.id]);
      ws.notify(t(`„${s.node.title}“ wurde hierher verschoben. Rückgängig im Verlauf.`, `Moved “${s.node.title}” into this entry. Undo it in the history.`));
    });
    confirmMerge = null;
  }

  async function dismiss(s: Similar) {
    ws.suggestions = ws.suggestions.filter((o) => o.node.id !== s.node.id);
    await act(() => api.dismissSuggestion(nodeId, s.node.id));
  }

  let previewing = $state<Attachment | null>(null);
  let dragging = $state(false);
  let uploading = $state(0);
  let filePicker: HTMLInputElement | undefined = $state();
  let fileLinks: FileLinks | undefined = $state();
  let preview: HTMLDivElement | undefined = $state();
  const inlineUrls = new Map<string, Promise<string>>();

  async function attach(file: File) {
    if (file.size > MAX_ATTACHMENT_BYTES) {
      ws.notify(t(`„${file.name}“ ist zu groß. Anhänge dürfen höchstens 200 MB haben.`, `“${file.name}” is too large. Attachments can be 200 MB at most.`));
      return null;
    }
    uploading++;
    try {
      return await api.addAttachment(nodeId, file);
    } catch (e) {
      ws.notify(errorMessage(e));
      return null;
    } finally {
      uploading--;
    }
  }

  async function ondrop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    await Promise.all([...(e.dataTransfer?.files ?? [])].map(attach));
  }

  function hasFiles(e: DragEvent) {
    return e.dataTransfer?.types.includes('Files') ?? false;
  }

  async function pasteFile(file: File) {
    const a = await attach(pastedFile(file));
    return a && pastedAttachmentMarkdown(a);
  }

  // Images in the text. Only attachments of this entry: a note written by an AI
  // must not be able to pull other files into view.
  $effect(() => {
    void html;
    const files = detail.attachments;
    preview?.querySelectorAll<HTMLImageElement>('img[data-attachment]').forEach(async (img) => {
      const a = files.find((f) => f.id === img.dataset.attachment && previewKind(f.mime) === 'image');
      if (!a) return;
      if (!inlineUrls.has(a.id)) inlineUrls.set(a.id, attachmentUrl(a));
      img.src = await inlineUrls.get(a.id)!;
    });
  });

  onDestroy(() => inlineUrls.forEach((url) => url.then(URL.revokeObjectURL).catch(() => {})));

  async function openHistory() {
    showHistory = !showHistory;
    if (showHistory) history = await api.nodeHistory(node.id);
  }

  function onPreviewClick(e: MouseEvent) {
    const el = (e.target as HTMLElement).closest<HTMLElement>('.wikilink');
    if (!el) return;
    const target = (el.dataset.target ?? '').toLowerCase();
    const hit = detail.outgoing.find((o) => o.other.title.toLowerCase() === target);
    if (hit) ws.select(hit.other.id, true);
    else ws.createNode(el.dataset.target ?? '');
  }

  async function completeTitles(prefix: string) {
    const hits = await api.search(prefix, { limit: 8 });
    return hits.map((h) => h.node.title).filter((title) => title !== node.title);
  }

  const fmt = (ms: number) =>
    new Date(ms).toLocaleString(locale, { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
</script>

<aside class="relative flex h-full flex-col border-l border-line bg-panel"
  aria-label={t('Eintrag', 'Entry')}
  ondragenter={(e) => hasFiles(e) && (dragging = true)}
  ondragover={(e) => hasFiles(e) && e.preventDefault()}
  ondragleave={(e) => e.relatedTarget === null && (dragging = false)}
  {ondrop}>
  {#if dragging}
    <div class="pointer-events-none absolute inset-2 z-30 flex flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-accent/60 bg-panel/90 text-accent" transition:pop={{ duration: 140, y: 0 }}>
      <Upload size={22} />
      <span class="font-medium">{t('Zum Anhängen loslassen', 'Drop to attach')}</span>
      <span class="text-xs text-faint">{t('Wird verschlüsselt im Tresor gespeichert', 'Stored encrypted in the vault')}</span>
    </div>
  {/if}
  <header class="flex h-11 shrink-0 items-center gap-1 border-b border-line-soft px-3" data-tauri-drag-region>
    <span class="chip">{node.kind}</span>
    {#if !node.reviewed}
      <button class="chip gap-1.5 !border-ai/30 !bg-ai/10 !text-ai transition-colors hover:!bg-ai/20"
        title={t('Von einer KI geschrieben. Klicken, um es als geprüft zu markieren.', 'Written by an AI client. Click to mark it as reviewed.')}
        onclick={() => act(() => api.updateNode(node.id, { reviewed: true }))}>
        <Bot size={12} /> {t('ungeprüft', 'unreviewed')} <Check size={12} />
      </button>
    {/if}
    <div class="h-full flex-1" data-tauri-drag-region></div>
    <button class="icon-btn" aria-label={favorite ? t('Aus den Favoriten entfernen', 'Remove from favourites') : t('Zu den Favoriten', 'Add to favourites')} aria-pressed={favorite}
      title={favorite ? t('Aus den Favoriten entfernen', 'Remove from favourites') : t('Zu den Favoriten', 'Add to favourites')} onclick={() => ws.toggleFavorite(nodeId)}>
      <Star size={15} class={favorite ? 'fill-warn/70 text-warn' : ''} />
    </button>
    <button class="icon-btn" aria-label={t('Im Graph zeigen', 'Show in the graph')} title={t('Im Graph zeigen', 'Show in the graph')} onclick={() => ws.focusTick++}><Crosshair size={15} /></button>
    <button class="icon-btn" aria-label={t('Verbinden', 'Link')} title={t('Verbinden (⌘L)', 'Link (⌘L)')} onclick={() => (ws.palette = 'link')}><Link2 size={15} /></button>
    <button class="icon-btn {showHistory ? '!bg-raised !text-text' : ''}" aria-label={t('Verlauf', 'History')} title={t('Verlauf', 'History')} onclick={openHistory}><History size={15} /></button>
    {#if detail.read_only}
      <!-- Goes away with its file. -->
    {:else if confirmDelete}
      <button class="btn-danger !h-7" in:pop={{ duration: 120, y: 0 }} onclick={() => (draft.discard(), act(() => api.deleteNode(nodeId)))} onblur={() => (confirmDelete = false)}>{t('Löschen?', 'Delete?')}</button>
    {:else}
      <button class="icon-btn hover:!text-danger" aria-label={t('In den Papierkorb', 'Move to trash')} title={t('In den Papierkorb', 'Move to trash')} onclick={() => (confirmDelete = true)}><Trash2 size={15} /></button>
    {/if}
    <button class="icon-btn" aria-label={t('Schließen', 'Close')} title={t('Schließen (Esc)', 'Close (Esc)')} onclick={() => ws.select(null)}><X size={15} /></button>
  </header>

  <div class="flex-1 overflow-y-auto px-5 pt-4 pb-6">
    <input
      class="mb-2.5 w-full bg-transparent text-lg leading-snug font-semibold tracking-tight outline-none"
      aria-label={t('Titel', 'Title')}
      readonly={detail.read_only}
      bind:value={draft.title}
      oninput={() => draft.changed()}
      onblur={() => draft.save()}
      onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()} />

    <div class="mb-1.5 flex flex-wrap items-center gap-1">
      {#each detail.projects as p (p.id)}
        <span class="chip group/chip">
          <span class="h-2 w-2 rounded-full" style="background: {ws.projectColor.get(p.id)}"></span>{p.name}
          <button class="-mr-0.5 text-faint transition-colors hover:text-danger" aria-label={t('Aus Projekt entfernen', 'Remove from project')} onclick={() => act(() => api.removeFromProject(p.id, [node.id]))}><X size={11} /></button>
        </span>
      {/each}
      <div class="relative">
        <button class="chip gap-1 !border-dashed !bg-transparent transition-colors hover:!border-faint hover:text-text" onclick={() => (pickingProject = !pickingProject)}>
          <Plus size={11} /> {t('Projekt', 'Project')}
        </button>
        {#if pickingProject}
          <div class="fixed inset-0 z-40" role="presentation" onclick={() => (pickingProject = false)}></div>
          <div class="surface absolute top-7 left-0 z-50 w-64 rounded-xl p-1" transition:pop={{ duration: 140, y: -4 }}>
            <ProjectPicker
              exclude={(id) => detail.projects.some((p) => p.id === id)}
              onclose={() => (pickingProject = false)}
              onpick={(id) => ((pickingProject = false), id && act(() => api.addToProject(id, [node.id])))}
              oncreate={(name) => ((pickingProject = false), act(async () => api.addToProject((await api.createProject(name)).id, [node.id])))} />
          </div>
        {/if}
      </div>
    </div>

    <div class="mb-5 flex flex-wrap items-center gap-1">
      {#each node.tags as tag (tag)}
        <span class="chip">#{tag}
          <button class="-mr-0.5 text-faint transition-colors hover:text-danger" aria-label={t('Tag entfernen', 'Remove tag')} onclick={() => act(() => api.updateNode(node.id, { tags_remove: [tag] }))}><X size={11} /></button>
        </span>
      {/each}
      <TagInput exclude={node.tags} onadd={(tag) => act(() => api.updateNode(node.id, { tags_add: [tag] }))} />
    </div>

    <div class="mb-1 flex h-7 items-center">
      <div class="section-title !mb-0">{t('Inhalt', 'Content')}</div>
      {#if draft.inFlight || draft.dirty}
        <span class="ml-2 flex items-center gap-1.5 text-[11px] text-faint" in:pop={{ duration: 120, y: 0 }}>
          <span class="h-1.5 w-1.5 animate-pulse rounded-full bg-warn"></span>{draft.inFlight ? t('Speichert …', 'Saving …') : t('Ungespeichert', 'Unsaved')}
        </span>
      {/if}
      <div class="flex-1"></div>
      {#if detail.read_only}
        <span class="text-[11px] text-faint">{t('Aus einer Datei, nur dort änderbar', 'From a file; change it there')}</span>
      {:else}
        <button class="icon-btn" aria-label={editing ? t('Ansehen', 'View') : t('Bearbeiten', 'Edit')} title={editing ? t('Ansehen', 'View') : t('Bearbeiten', 'Edit')} onclick={() => (draft.save(), (editing = !editing))}>
          {#if editing}<Eye size={14} />{:else}<Pencil size={14} />{/if}
        </button>
      {/if}
    </div>
    {#if editing}
      <MarkdownEditor
        value={draft.body}
        placeholders={node.kind === TEMPLATE_KIND}
        autofocus
        titles={completeTitles}
        onpastefile={pasteFile}
        onsave={() => draft.save()}
        onchange={(text) => ((draft.body = text), draft.changed())} />
    {:else if node.body.trim()}
      <!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized by DOMPurify -->
      <div class="prose-bm" role="presentation" bind:this={preview} onclick={onPreviewClick} ondblclick={() => (editing = true)}>{@html html}</div>
    {:else if !detail.read_only}
      <button class="w-full rounded-xl border border-dashed border-line py-6 text-faint transition-colors hover:border-faint hover:text-muted" onclick={() => (editing = true)}>
        Noch leer. Klicken zum Schreiben.
      </button>
    {/if}

    {#if node.kind === TEMPLATE_KIND}
      <TemplateUse body={draft.body} title={draft.title} projects={detail.projects.map((p) => p.id)} />
    {/if}

    <input class="hidden" type="file" multiple bind:this={filePicker} aria-hidden="true" tabindex="-1"
      onchange={async (e) => {
        const input = e.currentTarget;
        await Promise.all([...(input.files ?? [])].map(attach));
        input.value = '';
      }} />

    {#if detail.attachments.length || uploading}
      <div class="mt-7 mb-1 flex h-6 items-center">
        <div class="section-title !mb-0 flex-1">{t('Anhänge', 'Attachments')}</div>
        {#if uploading}
          <span class="mr-1 flex items-center gap-1.5 text-[11px] text-faint" in:pop={{ duration: 120, y: 0 }}>
            <span class="h-1.5 w-1.5 animate-pulse rounded-full bg-accent"></span>{t('Verschlüsselt …', 'Encrypting …')}
          </span>
        {/if}
      </div>
      {#each detail.attachments as a (a.id)}
        {@const kind = previewKind(a.mime)}
        <div class="group flex h-8 items-center gap-2 rounded-lg px-2 transition-colors hover:bg-elevated" in:pop={{ duration: 160, y: 3 }}>
          {#if kind === 'image'}<FileImage size={14} class="shrink-0 text-faint" />{:else if kind === 'none'}<FileIcon size={14} class="shrink-0 text-faint" />{:else}<FileText size={14} class="shrink-0 text-faint" />{/if}
          <button class="min-w-0 flex-1 truncate text-left" onclick={() => (previewing = a)}>{a.filename}</button>
          <span class="shrink-0 text-[11px] text-faint tabular-nums group-hover:hidden">{formatSize(a.size)}</span>
          <button class="hidden shrink-0 text-faint transition-colors group-hover:block hover:text-danger focus-visible:block" aria-label={t('Anhang entfernen', 'Remove attachment')} title={t('Anhang entfernen', 'Remove attachment')}
            onclick={() => act(() => api.deleteAttachment(a.id))}><X size={13} /></button>
        </div>
      {/each}
    {/if}

    <FileLinks node={nodeId} bind:this={fileLinks} />

    <!-- Only what is there takes up room; the rest waits here. -->
    {#if node.kind !== TEMPLATE_KIND}
      <div class="mt-5 flex flex-wrap gap-1">
        <button class="chip gap-1 !bg-transparent transition-colors hover:!border-faint hover:text-text" aria-label={t('Datei anhängen', 'Attach a file')} title={t('Wird verschlüsselt gespeichert. Geht auch per Ziehen.', 'Stored encrypted. Dragging works too.')} onclick={() => filePicker?.click()}><Paperclip size={11} /> {t('Anhang', 'Attachment')}</button>
        <button class="chip gap-1 !bg-transparent transition-colors hover:!border-faint hover:text-text" title={t('Verweist auf eine Datei auf deinem Mac, ohne sie zu kopieren.', 'Points to a file on your Mac without copying it.')} onclick={() => fileLinks?.link(false)}><FilePlus size={11} /> {t('Datei verknüpfen', 'Link a file')}</button>
        <button class="chip gap-1 !bg-transparent transition-colors hover:!border-faint hover:text-text" title={t('Verweist auf einen Ordner auf deinem Mac, ohne ihn zu kopieren.', 'Points to a folder on your Mac without copying it.')} onclick={() => fileLinks?.link(true)}><FolderPlus size={11} /> {t('Ordner verknüpfen', 'Link a folder')}</button>
      </div>
    {/if}

    {#if groups.length}
      <div class="section-title mt-7">{t('Verbindungen', 'Links')}</div>
      <div class="flex flex-col gap-3">
        {#each groups as [key, group] (key)}
          <div>
            <div class="mb-1 flex items-center gap-1 px-1 text-[11px] text-faint">
              {#if group.out}<ArrowRight size={11} />{:else}<ArrowLeft size={11} />{/if}
              {relationLabel(group.relation, group.out)}
            </div>
            {#each group.edges as e (e.edge.id)}
              <div class="group flex h-8 items-center gap-2 rounded-lg px-2 transition-colors hover:bg-elevated">
                <button class="min-w-0 flex-1 truncate text-left" onclick={() => ws.select(e.other.id, true)}>{e.other.title}</button>
                {#if e.edge.origin === 'ai'}<span title={t('Von einer KI verbunden', 'Linked by an AI client')}><Bot size={12} class="text-ai" /></span>{/if}
                {#if e.edge.origin !== 'wikilink'}
                  <button class="text-faint opacity-0 transition-opacity group-hover:opacity-100 hover:text-danger focus-visible:opacity-100" aria-label={t('Verbindung lösen', 'Remove link')} title={t('Verbindung lösen', 'Remove link')}
                    onclick={() => act(() => api.unlink(e.edge.src, e.edge.dst, e.edge.relation))}><X size={13} /></button>
                {/if}
              </div>
            {/each}
          </div>
        {/each}
      </div>
    {/if}

    {#if ws.suggestions.length}
      <div class="section-title mt-7 flex items-center gap-1.5"><Link2 size={11} /> {t('Vorschläge', 'Suggestions')}</div>
      {#each ws.suggestions as s (s.node.id)}
        <div class="group flex h-8 items-center gap-2 rounded-lg px-2 transition-colors hover:bg-elevated" in:pop={{ duration: 160, y: 3 }}>
          <button class="min-w-0 flex-1 truncate text-left text-muted transition-colors hover:text-text" onclick={() => ws.select(s.node.id, true)}>{s.node.title}</button>
          {#if s.likely_duplicate}
            <span class="chip shrink-0 !h-5 !border-warn/30 !bg-warn/10 !text-warn group-hover:hidden">{t('Duplikat?', 'Duplicate?')}</span>
          {/if}
          <div class="hidden shrink-0 items-center gap-0.5 group-hover:flex group-focus-within:flex">
            {#if confirmMerge === s.node.id}
              <button class="btn-danger !h-6 !px-2 text-xs" onclick={() => merge(s)} onblur={() => (confirmMerge = null)}>{t('Zusammenführen?', 'Merge?')}</button>
            {:else}
              {#if s.likely_duplicate}
                <button class="icon-btn !h-6 !w-6" aria-label={t('Hierher zusammenführen', 'Merge into this entry')} title={t('Hierher zusammenführen', 'Merge into this entry')} onclick={() => (confirmMerge = s.node.id)}><GitMerge size={13} /></button>
              {/if}
              <button class="icon-btn !h-6 !w-6 hover:!text-accent" aria-label={t('Verbinden', 'Link')} title={t('Verbinden', 'Link')} onclick={() => act(() => api.acceptSuggestion(nodeId, s.node.id))}><Link2 size={13} /></button>
              <button class="icon-btn !h-6 !w-6" aria-label={t('Vorschlag ausblenden', 'Dismiss suggestion')} title={t('Nicht mehr vorschlagen', 'Stop suggesting this')} onclick={() => dismiss(s)}><X size={13} /></button>
            {/if}
          </div>
        </div>
      {/each}
    {/if}

    {#if showHistory}
      <div in:pop={{ duration: 160, y: 4 }}>
        <div class="section-title mt-7">{t('Verlauf', 'History')}</div>
        {#each history as h (h.change_set + h.rev)}
          <div class="group flex h-8 items-center gap-2.5 rounded-lg px-2 transition-colors hover:bg-elevated">
            <span class="flex-1 truncate">{h.actor_name} <span class="text-muted">{h.before ? (h.after?.deleted_at ? t('gelöscht', 'deleted') : t('bearbeitet', 'edited')) : t('erstellt', 'created')}</span></span>
            <span class="text-[11px] text-faint tabular-nums">{fmt(h.at)}</span>
            <button class="icon-btn !h-6 !w-6 opacity-0 group-hover:opacity-100 focus-visible:opacity-100" aria-label={t('Rückgängig', 'Undo')} title={t('Diese Änderung rückgängig machen', 'Undo this change')}
              onclick={() => act(async () => { await api.revert(h.change_set); history = await api.nodeHistory(node.id); })}>
              <Undo2 size={12} />
            </button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
  <footer class="flex items-center gap-1.5 border-t border-line-soft px-5 py-2 text-[11px] text-faint tabular-nums">
    <span>{t('Geändert', 'Changed')} {fmt(node.updated_at)}</span><span aria-hidden="true">·</span><span>Rev. {node.rev}</span>
    <span class="selectable ml-auto font-mono">#{node.id.slice(-8)}</span>
  </footer>
</aside>

{#if previewing}
  <!-- Keyed: opening another file during the fade-out would otherwise reuse this
       instance, whose content was loaded for the previous file. -->
  {#key previewing.id}
    <AttachmentPreview attachment={previewing} onclose={() => (previewing = null)} />
  {/key}
{/if}
