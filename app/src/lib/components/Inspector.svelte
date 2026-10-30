<script lang="ts">
  import { Crosshair, Link2, Trash2, X, Pencil, Eye, History, Bot, Check, Undo2 } from '@lucide/svelte';
  import { api, errorMessage, type EdgeView, type NodeRevision } from '#lib/api.ts';
  import { renderMarkdown } from '#lib/markdown.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import MarkdownEditor from './MarkdownEditor.svelte';

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
  let loadedId = '';

  $effect(() => {
    const n = node;
    if (n.id !== loadedId) {
      clearTimeout(saveTimer);
      loadedId = n.id;
      dirty = false;
      editing = n.body.trim() === '';
      confirmDelete = false;
      showHistory = false;
    }
    if (!dirty) {
      draftTitle = n.title;
      draftBody = n.body;
    }
  });

  const knownTitles = $derived(new Set(detail.outgoing.map((e) => e.other.title.toLowerCase())));
  const html = $derived(renderMarkdown(node.body, (t) => knownTitles.has(t.toLowerCase())));

  const groups = $derived.by(() => {
    const out = new Map<string, EdgeView[]>();
    for (const e of detail.outgoing) out.set(`→ ${e.edge.relation}`, [...(out.get(`→ ${e.edge.relation}`) ?? []), e]);
    for (const e of detail.incoming) out.set(`← ${e.edge.relation}`, [...(out.get(`← ${e.edge.relation}`) ?? []), e]);
    return [...out.entries()];
  });

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
    const id = node.id;
    const patch: Parameters<typeof api.updateNode>[1] = {};
    if (draftTitle.trim() && draftTitle !== node.title) patch.title = draftTitle;
    if (draftBody !== node.body) patch.body = { mode: 'replace', text: draftBody };
    dirty = false;
    if (!patch.title && !patch.body) return;
    saving = true;
    try {
      await api.updateNode(id, patch);
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
  <header class="flex items-center gap-1 border-b border-line px-3 py-2">
    <span class="chip">{node.kind}</span>
    {#if !node.reviewed}
      <button class="chip !border-ai/40 !text-ai" title="Von einer KI erstellt. Klicken zum Bestätigen."
        onclick={() => act(() => api.updateNode(node.id, { reviewed: true }))}>
        <Bot size={11} /> ungeprüft <Check size={11} />
      </button>
    {/if}
    <div class="flex-1"></div>
    <button class="icon-btn" title="Im Graph zeigen" onclick={() => ws.focusTick++}><Crosshair size={15} /></button>
    <button class="icon-btn" title="Verbinden (⌘L)" onclick={() => (ws.palette = 'link')}><Link2 size={15} /></button>
    <button class="icon-btn" title="Verlauf" onclick={openHistory}><History size={15} /></button>
    {#if confirmDelete}
      <button class="chip !border-danger/50 !text-danger" onclick={() => act(() => api.deleteNode(node.id))}>Löschen?</button>
    {:else}
      <button class="icon-btn" title="In den Papierkorb" onclick={() => (confirmDelete = true)}><Trash2 size={15} /></button>
    {/if}
    <button class="icon-btn" title="Schließen (Esc)" onclick={() => ws.select(null)}><X size={15} /></button>
  </header>

  <div class="flex-1 overflow-y-auto px-4 py-3">
    <input
      class="mb-2 w-full bg-transparent text-[17px] font-semibold outline-none"
      bind:value={draftTitle}
      oninput={scheduleSave}
      onblur={save}
      onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()} />

    <div class="mb-3 flex flex-wrap items-center gap-1">
      {#each node.tags as tag (tag)}
        <span class="chip">#{tag}
          <button class="hover:text-danger" onclick={() => act(() => api.updateNode(node.id, { tags_remove: [tag] }))}><X size={10} /></button>
        </span>
      {/each}
      <form onsubmit={addTag}>
        <input class="w-20 bg-transparent text-xs text-muted outline-none placeholder:text-faint" placeholder="+ Tag" bind:value={newTag} />
      </form>
    </div>

    <div class="mb-4 flex flex-wrap items-center gap-1">
      {#each detail.projects as p (p.id)}
        <span class="chip" style="border-color: {ws.projectColor.get(p.id)}55">
          <span class="h-2 w-2 rounded-full" style="background: {ws.projectColor.get(p.id)}"></span>{p.name}
          <button class="hover:text-danger" onclick={() => act(() => api.removeFromProject(p.id, [node.id]))}><X size={10} /></button>
        </span>
      {/each}
      {#if availableProjects.length}
        <select
          class="rounded-md bg-transparent text-xs text-faint outline-none"
          onchange={(e) => {
            const v = (e.currentTarget as HTMLSelectElement).value;
            (e.currentTarget as HTMLSelectElement).value = '';
            if (v) act(() => api.addToProject(v, [node.id]));
          }}>
          <option value="">+ Projekt</option>
          {#each availableProjects as p (p.project.id)}<option value={p.project.id}>{p.project.name}</option>{/each}
        </select>
      {/if}
    </div>

    <div class="mb-1 flex items-center">
      <div class="section-title flex-1">Inhalt {#if saving}<span class="normal-case">· speichert …</span>{:else if dirty}<span class="normal-case">· ungespeichert</span>{/if}</div>
      <button class="icon-btn" title={editing ? 'Ansehen' : 'Bearbeiten'} onclick={() => (save(), (editing = !editing))}>
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
      <div class="prose-bm mb-2" role="presentation" onclick={onPreviewClick} ondblclick={() => (editing = true)}>{@html html}</div>
    {:else}
      <button class="text-faint" onclick={() => (editing = true)}>Leer. Klicken zum Schreiben.</button>
    {/if}

    {#if groups.length}
      <div class="section-title mt-5">Verbindungen</div>
      {#each groups as [label, edges] (label)}
        <div class="mb-2">
          <div class="mb-0.5 text-xs text-faint">{label}</div>
          {#each edges as e (e.edge.id)}
            <div class="group flex items-center gap-1 rounded-md px-1 py-0.5 hover:bg-elevated">
              <button class="flex-1 truncate text-left" onclick={() => ws.select(e.other.id, true)}>{e.other.title}</button>
              {#if e.edge.origin === 'ai'}<Bot size={11} class="text-ai" />{/if}
              {#if e.edge.origin !== 'wikilink'}
                <button class="hidden text-faint group-hover:block hover:text-danger" title="Verbindung lösen"
                  onclick={() => act(() => api.unlink(e.edge.src, e.edge.dst, e.edge.relation))}><X size={12} /></button>
              {/if}
            </div>
          {/each}
        </div>
      {/each}
    {/if}

    {#if showHistory}
      <div class="section-title mt-5">Verlauf</div>
      {#each history as h (h.change_set + h.rev)}
        <div class="flex items-center gap-2 py-1 text-xs">
          <span class="text-faint">{fmt(h.at)}</span>
          <span class="flex-1 truncate">{h.actor_name} · {h.before ? (h.after?.deleted_at ? 'gelöscht' : 'bearbeitet') : 'erstellt'}</span>
          <button class="icon-btn !h-5 !w-5" title="Diese Änderung rückgängig machen"
            onclick={() => act(async () => { await api.revert(h.change_set); history = await api.nodeHistory(node.id); })}>
            <Undo2 size={12} />
          </button>
        </div>
      {/each}
    {/if}
  </div>
  <footer class="border-t border-line px-4 py-2 text-[11px] text-faint">
    Rev {node.rev} · geändert {fmt(node.updated_at)} · #{node.id.slice(-8)}
  </footer>
</aside>
