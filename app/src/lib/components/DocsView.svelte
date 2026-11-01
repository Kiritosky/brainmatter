<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { ArrowLeft, ArrowRight, ArrowDown, ArrowUp, BookOpen, FilePlus, FileText, FolderSync, Pencil, Plus, Search, Trash2, TriangleAlert, X } from '@lucide/svelte';
  import { api, errorMessage, type DocPage, type DocSource, type Id, type NodeDetail, type SearchHit } from '#lib/api.ts';
  import { renderMarkdown } from '#lib/markdown.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';

  let { project }: { project: Id } = $props();

  const INDENT_PX = 14;

  let pages = $state.raw<DocPage[]>([]);
  let sources = $state.raw<DocSource[]>([]);
  let unreachable = $state.raw<string[]>([]);
  let currentId = $state<Id | null>(null);
  let current = $state.raw<NodeDetail | null>(null);
  let query = $state('');
  let hits = $state.raw<SearchHit[]>([]);
  let article: HTMLElement | undefined = $state();
  let scroller: HTMLElement | undefined = $state();
  let headings = $state.raw<{ id: string; title: string; level: number }[]>([]);
  let loaded = $state(false);
  let tagFilter = $state<string | null>(null);
  let newTag = $state('');

  const tagCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const p of pages) for (const tag of p.tags) counts.set(tag, (counts.get(tag) ?? 0) + 1);
    return [...counts].sort(([a], [b]) => a.localeCompare(b, 'de'));
  });
  const shown = $derived(tagFilter ? pages.filter((p) => p.tags.includes(tagFilter!)) : pages);

  const index =$derived(pages.findIndex((p) => p.node.id === currentId));
  const previous = $derived(index > 0 ? pages[index - 1] : null);
  const next = $derived(index >= 0 && index < pages.length - 1 ? pages[index + 1] : null);
  const page = $derived(index >= 0 ? pages[index] : null);
  const titles = $derived(new Set(pages.map((p) => p.node.title.toLowerCase())));
  /** Files usually start with their own title, which the page already shows. */
  function withoutTitle(body: string, title: string) {
    const first = /^\s*#[ \t]+(.+?)[ \t]*#*[ \t]*(\r?\n|$)/.exec(body);
    return first?.[1] === title ? body.slice(first[0].length) : body;
  }
  const html = $derived(
    current ? renderMarkdown(withoutTitle(current.node.body, current.node.title), (t) => titles.has(t.toLowerCase())) : '',
  );

  async function act(fn: () => Promise<unknown>) {
    try {
      await fn();
    } catch (e) {
      ws.notify(errorMessage(e));
    }
    await load();
  }

  async function load() {
    [pages, sources] = await Promise.all([api.docTree(project), api.docSources(project)]);
    loaded = true;
    // The last page with this tag may just have lost it.
    if (tagFilter && !pages.some((p) => p.tags.includes(tagFilter!))) tagFilter = null;
    if (!pages.some((p) => p.node.id === currentId)) currentId = pages[0]?.node.id ?? null;
    await loadCurrent();
  }

  async function loadCurrent() {
    current = currentId ? await api.nodeDetail(currentId).catch(() => null) : null;
  }

  async function open(id: Id) {
    currentId = id;
    query = '';
    await loadCurrent();
    scroller?.scrollTo({ top: 0 });
  }

  /** Files may have changed while nobody was looking, so this runs on opening. */
  async function syncSources() {
    try {
      const status = await api.syncDocSources(project);
      unreachable = status.unreachable;
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  $effect(() => {
    void project;
    untrack(() => {
      currentId = null;
      tagFilter = null;
      load().then(syncSources).then(load);
    });
  });

  $effect(() => {
    void ws.revision;
    untrack(() => loaded && load());
  });

  // The reader's table of contents comes from what was actually rendered.
  $effect(() => {
    void html;
    tick().then(() => {
      const found: typeof headings = [];
      article?.querySelectorAll<HTMLElement>('h1, h2, h3').forEach((el, i) => {
        el.id = `doc-heading-${i}`;
        found.push({ id: el.id, title: el.textContent ?? '', level: Number(el.tagName[1]) });
      });
      headings = found;
    });
  });

  $effect(() => {
    const q = query.trim();
    if (!q) {
      hits = [];
      return;
    }
    const tags = tagFilter ? [tagFilter] : [];
    const timer = setTimeout(async () => {
      hits = await api.search(q, { project, kinds: ['doc'], tags, limit: 30 }).catch(() => []);
    }, 80);
    return () => clearTimeout(timer);
  });

  async function newPage(parent: Id | null = null) {
    await act(async () => {
      const node = await api.createDocPage(project, 'Neue Seite', parent);
      currentId = node.id;
      await ws.select(node.id);
    });
  }

  function siblings(p: DocPage) {
    return pages.filter((o) => o.parent === p.parent);
  }

  function move(p: DocPage, by: number) {
    const position = siblings(p).findIndex((o) => o.node.id === p.node.id) + by;
    return act(() => api.placeDocPage(p.node.id, p.parent, position));
  }

  function addTag(e: SubmitEvent) {
    e.preventDefault();
    const tag = newTag.trim();
    newTag = '';
    if (tag && currentId) {
      const id = currentId;
      act(() => api.updateNode(id, { tags_add: [tag] }));
    }
  }

  function onArticleClick(e: MouseEvent) {
    const el = (e.target as HTMLElement).closest<HTMLElement>('.wikilink');
    if (!el) return;
    const target = (el.dataset.target ?? '').toLowerCase();
    const found = pages.find((p) => p.node.title.toLowerCase() === target);
    if (found) open(found.node.id);
    else {
      const outgoing = current?.outgoing.find((o) => o.other.title.toLowerCase() === target);
      if (outgoing) ws.select(outgoing.other.id, true);
    }
  }

  const fmt = (ms: number) => new Date(ms).toLocaleDateString('de-DE', { day: '2-digit', month: 'long', year: 'numeric' });
  // Snippets mark matches with **…**.
  const parts = (s: string) => s.split('**').map((text, i) => ({ text, mark: i % 2 === 1 }));
</script>

<div class="absolute inset-0 z-20 grid bg-bg" style="grid-template-columns: 248px minmax(0, 1fr)">
  <nav class="flex min-h-0 flex-col border-r border-line-soft" aria-label="Seiten">
    <div class="flex items-center gap-1.5 px-3 pt-1 pb-2">
      <div class="relative min-w-0 flex-1">
        <Search size={13} class="absolute top-2 left-2 text-faint" />
        <input class="field !h-7.5 w-full !pl-7 text-xs" placeholder="In der Doku suchen" aria-label="In der Doku suchen" bind:value={query} />
      </div>
      <button class="icon-btn shrink-0" aria-label="Neue Seite" title="Neue Seite" onclick={() => newPage()}><FilePlus size={15} /></button>
    </div>

    {#if tagCounts.length}
      <div class="flex max-h-20 flex-wrap gap-1 overflow-y-auto px-3 pb-2" role="group" aria-label="Nach Tag filtern">
        {#each tagCounts as [tag, count] (tag)}
          <button class="chip transition-colors {tagFilter === tag ? '!border-accent/40 !bg-raised !text-text' : 'hover:!border-faint hover:text-text'}"
            aria-pressed={tagFilter === tag} onclick={() => (tagFilter = tagFilter === tag ? null : tag)}>
            #{tag}<span class="text-faint tabular-nums">{count}</span>
          </button>
        {/each}
      </div>
    {/if}

    <div class="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
      {#if query.trim()}
        {#each hits as hit (hit.node.id)}
          <button class="w-full rounded-lg px-2 py-1.5 text-left transition-colors hover:bg-elevated" onclick={() => open(hit.node.id)}>
            <div class="truncate font-medium">{hit.node.title}</div>
            <div class="line-clamp-2 text-xs text-muted">
              {#each parts(hit.snippet) as p}{#if p.mark}<mark class="bg-transparent font-medium text-accent">{p.text}</mark>{:else}{p.text}{/if}{/each}
            </div>
          </button>
        {:else}
          <p class="px-2 py-6 text-center text-xs text-faint">Nichts gefunden.</p>
        {/each}
      {:else}
        {#each shown as p (p.node.id)}
          {@const list = siblings(p)}
          {@const position = list.findIndex((o) => o.node.id === p.node.id)}
          <!-- Under a filter the parents are missing, so nesting would only mislead. -->
          <div class="group relative" style="padding-left: {tagFilter ? 0 : p.depth * INDENT_PX}px">
            <button class="nav-row {currentId === p.node.id ? 'active' : ''}" onclick={() => open(p.node.id)}>
              <FileText size={13} class="shrink-0 {currentId === p.node.id ? 'text-accent' : 'text-faint'}" />
              <span class="truncate">{p.node.title}</span>
            </button>
            {#if !p.read_only && !tagFilter}
              <div class="absolute top-1 right-1 flex items-center rounded-md bg-elevated opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
                <button class="icon-btn !h-6 !w-6" aria-label="„{p.node.title}“ nach oben" title="Nach oben" disabled={position === 0} onclick={() => move(p, -1)}><ArrowUp size={12} /></button>
                <button class="icon-btn !h-6 !w-6" aria-label="„{p.node.title}“ nach unten" title="Nach unten" disabled={position === list.length - 1} onclick={() => move(p, 1)}><ArrowDown size={12} /></button>
                <button class="icon-btn !h-6 !w-6" aria-label="Unterseite von „{p.node.title}“" title="Unterseite" onclick={() => newPage(p.node.id)}><Plus size={12} /></button>
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>

    <div class="border-t border-line-soft p-2">
      <div class="mb-1 flex items-center pl-1.5">
        <span class="section-title !mb-0 flex-1">Aus Dateien</span>
        <button class="icon-btn !h-6 !w-6" aria-label="Doku-Ordner einbinden" title="Doku-Ordner einbinden" onclick={() => act(() => api.addDocSource(project, true))}><FolderSync size={13} /></button>
        <button class="icon-btn !h-6 !w-6" aria-label="Markdown-Datei einbinden" title="Markdown-Datei einbinden" onclick={() => act(() => api.addDocSource(project, false))}><FilePlus size={13} /></button>
      </div>
      {#each sources as s (s.id)}
        <div class="group flex items-center gap-1.5 rounded-md py-1 pr-0.5 pl-1.5 hover:bg-elevated" title={s.path}>
          {#if unreachable.includes(s.path)}
            <span title="Gerade nicht erreichbar. Die Seiten bleiben, bis der Ordner wieder da ist."><TriangleAlert size={12} class="shrink-0 text-warn" /></span>
          {/if}
          <span class="min-w-0 flex-1 truncate text-xs text-muted">{s.path.split('/').pop()}</span>
          <span class="text-[11px] text-faint tabular-nums">{s.pages}</span>
          <button class="text-faint opacity-0 transition-opacity group-hover:opacity-100 hover:text-danger focus-visible:opacity-100" aria-label="„{s.path.split('/').pop()}“ nicht mehr einbinden" title="Nicht mehr einbinden. Die Dateien bleiben."
            onclick={() => act(() => api.removeDocSource(s.id))}><X size={13} /></button>
        </div>
      {:else}
        <p class="px-1.5 text-[11px] leading-relaxed text-faint">Bindet z. B. den <code class="font-mono">docs</code>-Ordner eines Repos schreibgeschützt ein.</p>
      {/each}
    </div>
  </nav>

  <!-- A container, because the space depends on the panel beside it, not the window. -->
  <div class="@container min-h-0 overflow-y-auto" bind:this={scroller}>
    {#if current && page}
      <div class="mx-auto flex max-w-[1000px] gap-10 px-8 pt-4 pb-16">
        <main class="min-w-0 flex-1" in:pop={{ duration: 160, y: 4 }}>
          <header class="mb-6">
            <h1 class="text-[26px] leading-tight font-semibold tracking-tight wrap-anywhere">{current.node.title}</h1>
            <div class="mt-2 flex min-h-7.5 flex-wrap items-center gap-x-3 gap-y-1">
              <p class="min-w-0 flex-1 basis-48 text-xs text-faint">
                {#if page.read_only}
                  <span>Aus der Datei <code class="font-mono">{page.source_path}</code>, nur dort änderbar.</span>
                {/if}
                <span>Stand {fmt(current.node.updated_at)}</span>
              </p>
              {#if !current.read_only}
                <div class="flex shrink-0 items-center gap-1">
                  <button class="btn-ghost !h-7.5 text-xs" onclick={() => ws.select(page.node.id)}><Pencil size={13} /> Bearbeiten</button>
                  <button class="icon-btn hover:!text-danger" aria-label="In den Papierkorb" title="In den Papierkorb" onclick={() => act(() => api.deleteNode(page.node.id))}><Trash2 size={14} /></button>
                </div>
              {/if}
            </div>
            <div class="mt-2 flex flex-wrap items-center gap-1">
              {#each current.node.tags as tag (tag)}
                <span class="chip">
                  <button class="transition-colors hover:text-accent" title="Nach #{tag} filtern" onclick={() => (tagFilter = tag)}>#{tag}</button>
                  <button class="-mr-0.5 text-faint transition-colors hover:text-danger" aria-label="Tag {tag} entfernen" onclick={() => act(() => api.updateNode(page.node.id, { tags_remove: [tag] }))}><X size={11} /></button>
                </span>
              {/each}
              <form onsubmit={addTag}>
                <input class="h-6 w-20 rounded-md bg-transparent px-1 text-xs text-muted outline-none placeholder:text-faint focus:bg-elevated" placeholder="+ Tag" aria-label="Tag hinzufügen" bind:value={newTag} />
              </form>
            </div>
          </header>

          {#if current.node.body.trim()}
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized by DOMPurify -->
            <article class="prose-bm prose-doc selectable" role="presentation" bind:this={article} onclick={onArticleClick}>{@html html}</article>
          {:else}
            <button class="w-full rounded-xl border border-dashed border-line py-10 text-faint transition-colors hover:border-faint hover:text-muted" onclick={() => ws.select(page.node.id)}>
              Noch leer. Klicken zum Schreiben.
            </button>
          {/if}

          {#if previous || next}
            <div class="mt-12 grid grid-cols-2 gap-3 border-t border-line-soft pt-5">
              {#if previous}
                <button class="group rounded-xl border border-line-soft p-3 text-left transition-colors hover:border-line hover:bg-elevated" onclick={() => open(previous.node.id)}>
                  <span class="flex items-center gap-1.5 text-xs text-faint"><ArrowLeft size={12} /> Zurück</span>
                  <span class="mt-0.5 block truncate font-medium">{previous.node.title}</span>
                </button>
              {:else}<span></span>{/if}
              {#if next}
                <button class="rounded-xl border border-line-soft p-3 text-right transition-colors hover:border-line hover:bg-elevated" onclick={() => open(next.node.id)}>
                  <span class="flex items-center justify-end gap-1.5 text-xs text-faint">Weiter <ArrowRight size={12} /></span>
                  <span class="mt-0.5 block truncate font-medium">{next.node.title}</span>
                </button>
              {/if}
            </div>
          {/if}
        </main>

        {#if headings.length > 1}
          <aside class="sticky top-4 hidden h-fit w-44 shrink-0 @3xl:block" aria-label="Auf dieser Seite">
            <div class="section-title">Auf dieser Seite</div>
            {#each headings as h (h.id)}
              <button class="block w-full truncate py-1 text-left text-xs text-muted transition-colors hover:text-text"
                style="padding-left: {(h.level - 1) * 10}px" title={h.title}
                onclick={() => article?.querySelector(`#${h.id}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' })}>{h.title}</button>
            {/each}
          </aside>
        {/if}
      </div>
    {:else if loaded}
      <div class="flex h-full items-center justify-center">
        <div class="flex max-w-sm flex-col items-center text-center" in:pop={{ duration: 280, y: 10 }}>
          <div class="mb-4 flex h-14 w-14 items-center justify-center text-faint"><BookOpen size={26} /></div>
          <h2 class="text-base font-semibold tracking-tight">Noch keine Doku für „{ws.currentProject?.name}“</h2>
          <p class="mt-2 leading-relaxed text-muted">Schreibe Seiten direkt hier, oder binde den Doku-Ordner eines Repos ein. Verbundene KIs können beides lesen.</p>
          <div class="mt-5 flex gap-2">
            <button class="btn-primary" onclick={() => newPage()}>Erste Seite anlegen</button>
            <button class="btn-ghost" onclick={() => act(() => api.addDocSource(project, true))}><FolderSync size={14} /> Ordner einbinden</button>
          </div>
        </div>
      </div>
    {/if}
  </div>
</div>
