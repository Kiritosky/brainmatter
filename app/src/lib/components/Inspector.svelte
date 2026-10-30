<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { ArrowLeft, ArrowRight, Crosshair, Link2, Trash2, X, Pencil, Eye, History, Bot, Check, Plus, Undo2 } from '@lucide/svelte';
  import { api, errorMessage, type EdgeView, type NodeRevision } from '#lib/api.ts';
  import { renderMarkdown } from '#lib/markdown.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import MarkdownEditor from './MarkdownEditor.svelte';
  import { pop } from '#lib/ui/motion.ts';

  let pickingProject = $state(false);

  const detail = $derived(ws.detail!);
  const node = $derived(detail.node);

  // Local drafts: server updates only replace them while nothing is unsaved.
  let draftTitle = $state('');
  let draftBody = $state('');
  let dirty = $state(false);
  let saving = $state(false);
  let editing = $state(false);
  let confirmDelete = $state(false);
  let newTag = $state('');
  let showHistory = $state(false);
  let history = $state<NodeRevision[]>([]);
  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  // The workspace creates one Inspector per entry ({#key}). Saving must not
  // read `node`: a pending save can run after ws.detail moved to another entry,
  // and would write this draft there.
  const nodeId = untrack(() => node.id);
  let saved = { title: '', body: '' };

  untrack(() => {
    editing = node.body.trim() === '';
  });

  $effect(() => {
    const n = node;
    saved = { title: n.title, body: n.body };
    if (!dirty) {
      draftTitle = n.title;
      draftBody = n.body;
    }
  });

  // Leaving the entry within the autosave delay must not lose the last edit.
  onDestroy(save);

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

  const availableProjects = $derived(
    ws.projects.filter((p) => !detail.projects.some((dp) => dp.id === p.project.id)),
  );

  function scheduleSave() {
    dirty = true;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(save, 900);
  }

  async function save() {
    clearTimeout(saveTimer);
    if (!dirty) return;
    const patch: Parameters<typeof api.updateNode>[1] = {};
    if (draftTitle.trim() && draftTitle !== saved.title) patch.title = draftTitle;
    if (draftBody !== saved.body) patch.body = { mode: 'replace', text: draftBody };
    dirty = false;
    if (!patch.title && !patch.body) return;
    saved = { title: patch.title ?? saved.title, body: patch.body?.text ?? saved.body };
    saving = true;
    try {
      await api.updateNode(nodeId, patch);
    } catch (e) {
      ws.notify(errorMessage(e));
    } finally {
      saving = false;
    }
  }

  async function act(fn: () => Promise<unknown>) {
    try {
      await fn();
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  function addTag(e: SubmitEvent) {
    e.preventDefault();
    const tag = newTag.trim();
    newTag = '';
    if (tag) act(() => api.updateNode(node.id, { tags_add: [tag] }));
  }

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

<aside class="flex h-full flex-col border-l border-line bg-panel">
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
    <button class="icon-btn" aria-label="Im Graph zeigen" title="Im Graph zeigen" onclick={() => ws.focusTick++}><Crosshair size={15} /></button>
    <button class="icon-btn" aria-label="Verbinden" title="Verbinden (⌘L)" onclick={() => (ws.palette = 'link')}><Link2 size={15} /></button>
    <button class="icon-btn {showHistory ? '!bg-accent/15 !text-accent' : ''}" aria-label="Verlauf" title="Verlauf" onclick={openHistory}><History size={15} /></button>
    {#if confirmDelete}
      <button class="btn-danger !h-7" in:pop={{ duration: 120, y: 0 }} onclick={() => ((dirty = false), act(() => api.deleteNode(nodeId)))} onblur={() => (confirmDelete = false)}>Löschen?</button>
    {:else}
      <button class="icon-btn hover:!text-danger" aria-label="In den Papierkorb" title="In den Papierkorb" onclick={() => (confirmDelete = true)}><Trash2 size={15} /></button>
    {/if}
    <button class="icon-btn" aria-label="Schließen" title="Schließen (Esc)" onclick={() => ws.select(null)}><X size={15} /></button>
  </header>

  <div class="flex-1 overflow-y-auto px-5 pt-4 pb-6">
    <input
      class="mb-2.5 w-full bg-transparent text-lg leading-snug font-semibold tracking-tight outline-none"
      aria-label="Titel"
      bind:value={draftTitle}
      oninput={scheduleSave}
      onblur={save}
      onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()} />

    <div class="mb-1.5 flex flex-wrap items-center gap-1">
      {#each detail.projects as p (p.id)}
        <span class="chip group/chip">
          <span class="h-2 w-2 rounded-full" style="background: {ws.projectColor.get(p.id)}"></span>{p.name}
          <button class="-mr-0.5 text-faint transition-colors hover:text-danger" aria-label="Aus Projekt entfernen" onclick={() => act(() => api.removeFromProject(p.id, [node.id]))}><X size={11} /></button>
        </span>
      {/each}
      {#if availableProjects.length}
        <div class="relative">
          <button class="chip gap-1 !border-dashed !bg-transparent transition-colors hover:!border-faint hover:text-text" onclick={() => (pickingProject = !pickingProject)}>
            <Plus size={11} /> Projekt
          </button>
          {#if pickingProject}
            <div class="fixed inset-0 z-40" role="presentation" onclick={() => (pickingProject = false)}></div>
            <div class="surface absolute top-7 left-0 z-50 max-h-56 min-w-44 overflow-y-auto rounded-xl p-1" transition:pop={{ duration: 140, y: -4 }}>
              {#each availableProjects as p (p.project.id)}
                <button class="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left transition-colors hover:bg-accent/12"
                  onclick={() => ((pickingProject = false), act(() => api.addToProject(p.project.id, [node.id])))}>
                  <span class="h-2 w-2 shrink-0 rounded-full" style="background: {ws.projectColor.get(p.project.id)}"></span>
                  <span class="truncate">{p.project.name}</span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <div class="mb-5 flex flex-wrap items-center gap-1">
      {#each node.tags as tag (tag)}
        <span class="chip">#{tag}
          <button class="-mr-0.5 text-faint transition-colors hover:text-danger" aria-label="Tag entfernen" onclick={() => act(() => api.updateNode(node.id, { tags_remove: [tag] }))}><X size={11} /></button>
        </span>
      {/each}
      <form onsubmit={addTag}>
        <input class="h-6 w-20 rounded-md bg-transparent px-1 text-xs text-muted outline-none placeholder:text-faint focus:bg-elevated" placeholder="+ Tag" aria-label="Tag hinzufügen" bind:value={newTag} />
      </form>
    </div>

    <div class="mb-1 flex h-7 items-center">
      <div class="section-title !mb-0">Inhalt</div>
      {#if saving || dirty}
        <span class="ml-2 flex items-center gap-1.5 text-[11px] text-faint" in:pop={{ duration: 120, y: 0 }}>
          <span class="h-1.5 w-1.5 animate-pulse rounded-full bg-warn"></span>{saving ? 'Speichert …' : 'Ungespeichert'}
        </span>
      {/if}
      <div class="flex-1"></div>
      <button class="icon-btn" aria-label={editing ? 'Ansehen' : 'Bearbeiten'} title={editing ? 'Ansehen' : 'Bearbeiten'} onclick={() => (save(), (editing = !editing))}>
        {#if editing}<Eye size={14} />{:else}<Pencil size={14} />{/if}
      </button>
    </div>
    {#if editing}
      <MarkdownEditor
        value={draftBody}
        autofocus
        titles={completeTitles}
        onsave={save}
        onchange={(text) => ((draftBody = text), scheduleSave())} />
    {:else if node.body.trim()}
      <!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized by DOMPurify -->
      <div class="prose-bm" role="presentation" onclick={onPreviewClick} ondblclick={() => (editing = true)}>{@html html}</div>
    {:else}
      <button class="w-full rounded-xl border border-dashed border-line py-6 text-faint transition-colors hover:border-faint hover:text-muted" onclick={() => (editing = true)}>
        Noch leer. Klicken zum Schreiben.
      </button>
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
