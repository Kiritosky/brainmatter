<script lang="ts">
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
  const html = $derived(renderMarkdown(node.body, (t) => knownTitles.has(t.toLowerCase())));

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
    mentions: ['erwähnt', 'erwähnt von'],
    related_to: ['verwandt mit', 'verwandt mit'],
    part_of: ['Teil von', 'enthält'],
    depends_on: ['hängt ab von', 'Grundlage für'],
    example_of: ['Beispiel für', 'hat Beispiel'],
    supports: ['stützt', 'gestützt von'],
    contradicts: ['widerspricht', 'Widerspruch von'],
    derived_from: ['abgeleitet von', 'Quelle für'],
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
      ws.notify(`„${s.node.title}“ wurde hierher verschoben. Rückgängig im Verlauf.`);
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
      ws.notify(`„${file.name}“ ist zu groß. Anhänge dürfen höchstens 200 MB haben.`);
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
    return hits.map((h) => h.node.title).filter((t) => t !== node.title);
  }

  const fmt = (ms: number) =>
    new Date(ms).toLocaleString('de-DE', { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
</script>

<aside class="relative flex h-full flex-col border-l border-line bg-panel"
  aria-label="Eintrag"
  ondragenter={(e) => hasFiles(e) && (dragging = true)}
  ondragover={(e) => hasFiles(e) && e.preventDefault()}
  ondragleave={(e) => e.relatedTarget === null && (dragging = false)}
  {ondrop}>
  {#if dragging}
    <div class="pointer-events-none absolute inset-2 z-30 flex flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-accent/60 bg-panel/90 text-accent" transition:pop={{ duration: 140, y: 0 }}>
      <Upload size={22} />
      <span class="font-medium">Zum Anhängen loslassen</span>
      <span class="text-xs text-faint">Wird verschlüsselt im Tresor gespeichert</span>
    </div>
  {/if}
  <header class="flex h-11 shrink-0 items-center gap-1 border-b border-line-soft px-3" data-tauri-drag-region>
    <span class="chip">{node.kind}</span>
    {#if !node.reviewed}
      <button class="chip gap-1.5 !border-ai/30 !bg-ai/10 !text-ai transition-colors hover:!bg-ai/20"
        title="Von einer KI geschrieben. Klicken, um es als geprüft zu markieren."
        onclick={() => act(() => api.updateNode(node.id, { reviewed: true }))}>
        <Bot size={12} /> ungeprüft <Check size={12} />
      </button>
    {/if}
    <div class="h-full flex-1" data-tauri-drag-region></div>
    <button class="icon-btn" aria-label={favorite ? 'Aus den Favoriten entfernen' : 'Zu den Favoriten'} aria-pressed={favorite}
      title={favorite ? 'Aus den Favoriten entfernen' : 'Zu den Favoriten'} onclick={() => ws.toggleFavorite(nodeId)}>
      <Star size={15} class={favorite ? 'fill-warn/70 text-warn' : ''} />
    </button>
    <button class="icon-btn" aria-label="Im Graph zeigen" title="Im Graph zeigen" onclick={() => ws.focusTick++}><Crosshair size={15} /></button>
    <button class="icon-btn" aria-label="Verbinden" title="Verbinden (⌘L)" onclick={() => (ws.palette = 'link')}><Link2 size={15} /></button>
    <button class="icon-btn {showHistory ? '!bg-raised !text-text' : ''}" aria-label="Verlauf" title="Verlauf" onclick={openHistory}><History size={15} /></button>
    {#if detail.read_only}
      <!-- Goes away with its file. -->
    {:else if confirmDelete}
      <button class="btn-danger !h-7" in:pop={{ duration: 120, y: 0 }} onclick={() => (draft.discard(), act(() => api.deleteNode(nodeId)))} onblur={() => (confirmDelete = false)}>Löschen?</button>
    {:else}
      <button class="icon-btn hover:!text-danger" aria-label="In den Papierkorb" title="In den Papierkorb" onclick={() => (confirmDelete = true)}><Trash2 size={15} /></button>
    {/if}
    <button class="icon-btn" aria-label="Schließen" title="Schließen (Esc)" onclick={() => ws.select(null)}><X size={15} /></button>
  </header>

  <div class="flex-1 overflow-y-auto px-5 pt-4 pb-6">
    <input
      class="mb-2.5 w-full bg-transparent text-lg leading-snug font-semibold tracking-tight outline-none"
      aria-label="Titel"
      readonly={detail.read_only}
      bind:value={draft.title}
      oninput={() => draft.changed()}
      onblur={() => draft.save()}
      onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()} />

    <div class="mb-1.5 flex flex-wrap items-center gap-1">
      {#each detail.projects as p (p.id)}
        <span class="chip group/chip">
          <span class="h-2 w-2 rounded-full" style="background: {ws.projectColor.get(p.id)}"></span>{p.name}
          <button class="-mr-0.5 text-faint transition-colors hover:text-danger" aria-label="Aus Projekt entfernen" onclick={() => act(() => api.removeFromProject(p.id, [node.id]))}><X size={11} /></button>
        </span>
      {/each}
      <div class="relative">
        <button class="chip gap-1 !border-dashed !bg-transparent transition-colors hover:!border-faint hover:text-text" onclick={() => (pickingProject = !pickingProject)}>
          <Plus size={11} /> Projekt
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
          <button class="-mr-0.5 text-faint transition-colors hover:text-danger" aria-label="Tag entfernen" onclick={() => act(() => api.updateNode(node.id, { tags_remove: [tag] }))}><X size={11} /></button>
        </span>
      {/each}
      <TagInput exclude={node.tags} onadd={(tag) => act(() => api.updateNode(node.id, { tags_add: [tag] }))} />
    </div>

    <div class="mb-1 flex h-7 items-center">
      <div class="section-title !mb-0">Inhalt</div>
      {#if draft.inFlight || draft.dirty}
        <span class="ml-2 flex items-center gap-1.5 text-[11px] text-faint" in:pop={{ duration: 120, y: 0 }}>
          <span class="h-1.5 w-1.5 animate-pulse rounded-full bg-warn"></span>{draft.inFlight ? 'Speichert …' : 'Ungespeichert'}
        </span>
      {/if}
      <div class="flex-1"></div>
      {#if detail.read_only}
        <span class="text-[11px] text-faint">Aus einer Datei, nur dort änderbar</span>
      {:else}
        <button class="icon-btn" aria-label={editing ? 'Ansehen' : 'Bearbeiten'} title={editing ? 'Ansehen' : 'Bearbeiten'} onclick={() => (draft.save(), (editing = !editing))}>
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
        <div class="section-title !mb-0 flex-1">Anhänge</div>
        {#if uploading}
          <span class="mr-1 flex items-center gap-1.5 text-[11px] text-faint" in:pop={{ duration: 120, y: 0 }}>
            <span class="h-1.5 w-1.5 animate-pulse rounded-full bg-accent"></span>Verschlüsselt …
          </span>
        {/if}
      </div>
      {#each detail.attachments as a (a.id)}
        {@const kind = previewKind(a.mime)}
        <div class="group flex h-8 items-center gap-2 rounded-lg px-2 transition-colors hover:bg-elevated" in:pop={{ duration: 160, y: 3 }}>
          {#if kind === 'image'}<FileImage size={14} class="shrink-0 text-faint" />{:else if kind === 'none'}<FileIcon size={14} class="shrink-0 text-faint" />{:else}<FileText size={14} class="shrink-0 text-faint" />{/if}
          <button class="min-w-0 flex-1 truncate text-left" onclick={() => (previewing = a)}>{a.filename}</button>
          <span class="shrink-0 text-[11px] text-faint tabular-nums group-hover:hidden">{formatSize(a.size)}</span>
          <button class="hidden shrink-0 text-faint transition-colors group-hover:block hover:text-danger focus-visible:block" aria-label="Anhang entfernen" title="Anhang entfernen"
            onclick={() => act(() => api.deleteAttachment(a.id))}><X size={13} /></button>
        </div>
      {/each}
    {/if}

    <FileLinks node={nodeId} bind:this={fileLinks} />

    <!-- Only what is there takes up room; the rest waits here. -->
    {#if node.kind !== TEMPLATE_KIND}
      <div class="mt-5 flex flex-wrap gap-1">
        <button class="chip gap-1 !bg-transparent transition-colors hover:!border-faint hover:text-text" aria-label="Datei anhängen" title="Wird verschlüsselt gespeichert. Geht auch per Ziehen." onclick={() => filePicker?.click()}><Paperclip size={11} /> Anhang</button>
        <button class="chip gap-1 !bg-transparent transition-colors hover:!border-faint hover:text-text" title="Verweist auf eine Datei auf deinem Mac, ohne sie zu kopieren." onclick={() => fileLinks?.link(false)}><FilePlus size={11} /> Datei verknüpfen</button>
        <button class="chip gap-1 !bg-transparent transition-colors hover:!border-faint hover:text-text" title="Verweist auf einen Ordner auf deinem Mac, ohne ihn zu kopieren." onclick={() => fileLinks?.link(true)}><FolderPlus size={11} /> Ordner verknüpfen</button>
      </div>
    {/if}

    {#if groups.length}
      <div class="section-title mt-7">Verbindungen</div>
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
                {#if e.edge.origin === 'ai'}<span title="Von einer KI verbunden"><Bot size={12} class="text-ai" /></span>{/if}
                {#if e.edge.origin !== 'wikilink'}
                  <button class="text-faint opacity-0 transition-opacity group-hover:opacity-100 hover:text-danger focus-visible:opacity-100" aria-label="Verbindung lösen" title="Verbindung lösen"
                    onclick={() => act(() => api.unlink(e.edge.src, e.edge.dst, e.edge.relation))}><X size={13} /></button>
                {/if}
              </div>
            {/each}
          </div>
        {/each}
      </div>
    {/if}

    {#if ws.suggestions.length}
      <div class="section-title mt-7 flex items-center gap-1.5"><Link2 size={11} /> Vorschläge</div>
      {#each ws.suggestions as s (s.node.id)}
        <div class="group flex h-8 items-center gap-2 rounded-lg px-2 transition-colors hover:bg-elevated" in:pop={{ duration: 160, y: 3 }}>
          <button class="min-w-0 flex-1 truncate text-left text-muted transition-colors hover:text-text" onclick={() => ws.select(s.node.id, true)}>{s.node.title}</button>
          {#if s.likely_duplicate}
            <span class="chip shrink-0 !h-5 !border-warn/30 !bg-warn/10 !text-warn group-hover:hidden">Duplikat?</span>
          {/if}
          <div class="hidden shrink-0 items-center gap-0.5 group-hover:flex group-focus-within:flex">
            {#if confirmMerge === s.node.id}
              <button class="btn-danger !h-6 !px-2 text-xs" onclick={() => merge(s)} onblur={() => (confirmMerge = null)}>Zusammenführen?</button>
            {:else}
              {#if s.likely_duplicate}
                <button class="icon-btn !h-6 !w-6" aria-label="Hierher zusammenführen" title="Hierher zusammenführen" onclick={() => (confirmMerge = s.node.id)}><GitMerge size={13} /></button>
              {/if}
              <button class="icon-btn !h-6 !w-6 hover:!text-accent" aria-label="Verbinden" title="Verbinden" onclick={() => act(() => api.acceptSuggestion(nodeId, s.node.id))}><Link2 size={13} /></button>
              <button class="icon-btn !h-6 !w-6" aria-label="Vorschlag ausblenden" title="Nicht mehr vorschlagen" onclick={() => dismiss(s)}><X size={13} /></button>
            {/if}
          </div>
        </div>
      {/each}
    {/if}

    {#if showHistory}
      <div in:pop={{ duration: 160, y: 4 }}>
        <div class="section-title mt-7">Verlauf</div>
        {#each history as h (h.change_set + h.rev)}
          <div class="group flex h-8 items-center gap-2.5 rounded-lg px-2 transition-colors hover:bg-elevated">
            <span class="flex-1 truncate">{h.actor_name} <span class="text-muted">{h.before ? (h.after?.deleted_at ? 'gelöscht' : 'bearbeitet') : 'erstellt'}</span></span>
            <span class="text-[11px] text-faint tabular-nums">{fmt(h.at)}</span>
            <button class="icon-btn !h-6 !w-6 opacity-0 group-hover:opacity-100 focus-visible:opacity-100" aria-label="Rückgängig" title="Diese Änderung rückgängig machen"
              onclick={() => act(async () => { await api.revert(h.change_set); history = await api.nodeHistory(node.id); })}>
              <Undo2 size={12} />
            </button>
          </div>
        {/each}
      </div>
    {/if}
  </div>
  <footer class="flex items-center gap-1.5 border-t border-line-soft px-5 py-2 text-[11px] text-faint tabular-nums">
    <span>Geändert {fmt(node.updated_at)}</span><span aria-hidden="true">·</span><span>Rev. {node.rev}</span>
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
