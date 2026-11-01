<script lang="ts">
  import { untrack } from 'svelte';
  import { slide } from 'svelte/transition';
  import DeleteProjectDialog from './DeleteProjectDialog.svelte';
  import ProjectMenu from './ProjectMenu.svelte';
  import { ArrowUpCircle, BookMarked, ChevronRight, Ellipsis, Eye, EyeOff, Star, Network, FolderPlus, Lock, Search, Plus, Activity, Settings, ListChecks, Trash2 } from '@lucide/svelte';
  import { api, errorMessage, type Id, type Project } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';

  let { userName }: { userName: string | null } = $props();

  const COLLAPSED_KEY = 'bm.collapsedProjects';
  const MAX_TAGS = 24;
  const INDENT_PX = 12;

  let creatingUnder = $state<Id | null | undefined>(undefined);
  let newName = $state('');
  let collapsed = $state.raw(new Set<Id>(JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? '[]')));

  const byId = $derived(new Map(ws.projects.map((p) => [p.project.id, p])));
  const childrenOf = $derived.by(() => {
    const map = new Map<Id, Id[]>();
    for (const { project } of ws.projects) {
      if (project.parent_id) map.set(project.parent_id, [...(map.get(project.parent_id) ?? []), project.id]);
    }
    return map;
  });

  const ancestors = (id: Id): Id[] => {
    const parent = byId.get(id)?.project.parent_id;
    return parent ? [parent, ...ancestors(parent)] : [];
  };
  const depth = (id: Id) => ancestors(id).length;
  let showHidden = $state(false);
  /** Hidden, but needed to place something visible below it. */
  const holdsVisible = $derived.by(() => {
    const out = new Set<Id>();
    // Children come after their parents, so backwards reaches every ancestor.
    for (const { project } of [...ws.projects].reverse()) {
      if (project.parent_id && (!ws.hiddenProjects.has(project.id) || out.has(project.id))) out.add(project.parent_id);
    }
    return out;
  });
  const listed = (id: Id) => showHidden || !ws.hiddenProjects.has(id) || holdsVisible.has(id) || ws.view === id;
  const visible = $derived(ws.projects.filter((p) => listed(p.project.id) && !ancestors(p.project.id).some((a) => collapsed.has(a))));

  // Nothing left to unfold once everything is visible again.
  $effect(() => {
    if (!ws.hiddenProjectCount) showHidden = false;
  });

  /** A collapsed project answers for what it hides. An entry in two of its
   * subprojects counts twice here; for an overview that is close enough. */
  const count = (id: Id): number =>
    (byId.get(id)?.node_count ?? 0) + (collapsed.has(id) ? total(id) : 0);
  const total = (id: Id): number =>
    (childrenOf.get(id) ?? []).reduce((sum, child) => sum + (byId.get(child)?.node_count ?? 0) + total(child), 0);

  function setCollapsed(next: Set<Id>) {
    collapsed = next;
    localStorage.setItem(COLLAPSED_KEY, JSON.stringify([...next]));
  }

  function toggle(id: Id) {
    const next = new Set(collapsed);
    if (!next.delete(id)) next.add(id);
    setCollapsed(next);
  }

  function reveal(id: Id, withSelf = false) {
    const next = new Set(collapsed);
    const hidden = [...ancestors(id), ...(withSelf ? [id] : [])].filter((a) => next.delete(a));
    if (hidden.length) setCollapsed(next);
  }

  // Arriving at a project from elsewhere (search, breadcrumb) must show it. Only
  // on arrival: collapsing around the open project afterwards stays possible.
  $effect(() => {
    const view = ws.view;
    if (view) untrack(() => reveal(view));
  });

  async function createProject(e: SubmitEvent) {
    e.preventDefault();
    const name = newName.trim();
    const parent = creatingUnder ?? null;
    creatingUnder = undefined;
    newName = '';
    if (!name) return;
    try {
      const p = await api.createProject(name, parent);
      await ws.loadProjects();
      await ws.setView(p.id);
    } catch (err) {
      ws.notify(errorMessage(err));
    }
  }

  // ------------------------------------------------- renaming, menu, moving ---

  let renaming = $state<Id | null>(null);
  let renameValue = $state('');
  let menu = $state<{ project: Project; x: number; y: number } | null>(null);
  let deleting = $state<Project | null>(null);

  type Zone = 'before' | 'inside' | 'after';
  let dragging = $state<Id | null>(null);
  let dropAt = $state<{ id: Id | null; zone: Zone } | null>(null);
  /** Past the end of any list; the core puts it last. */
  const AT_END = 1_000_000;

  function startRename(project: Project) {
    menu = null;
    renameValue = project.name;
    renaming = project.id;
  }

  async function run(fn: () => Promise<unknown>) {
    try {
      await fn();
      await ws.loadProjects();
    } catch (err) {
      ws.notify(errorMessage(err));
    }
  }

  async function commitRename() {
    const id = renaming;
    const name = renameValue.trim();
    renaming = null;
    const current = id ? byId.get(id)?.project.name : null;
    if (id && name && name !== current) await run(() => api.renameProject(id, name));
  }

  function onRenameKey(e: KeyboardEvent) {
    if (e.key === 'Enter') commitRename();
    else if (e.key === 'Escape') {
      e.stopPropagation();
      renaming = null;
    }
  }

  function openMenu(project: Project, e: MouseEvent) {
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { project, x: box.left, y: box.bottom + 4 };
  }

  function canDropOn(id: Id) {
    return dragging !== null && !ws.subtreeOf(dragging).has(id);
  }

  function onDragOver(e: DragEvent, id: Id) {
    if (!canDropOn(id)) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const share = (e.clientY - box.top) / box.height;
    dropAt = { id, zone: share < 0.28 ? 'before' : share > 0.72 ? 'after' : 'inside' };
  }

  function siblingsOf(parent: Id | null) {
    return ws.projects
      .filter((p) => (p.project.parent_id ?? null) === parent && p.project.id !== dragging)
      .map((p) => p.project.id);
  }

  async function drop() {
    const id = dragging;
    const at = dropAt;
    dragging = null;
    dropAt = null;
    if (!id || !at) return;
    if (at.id === null) return run(() => api.moveProject(id, null, AT_END));
    if (at.zone === 'inside') {
      reveal(at.id, true);
      const parent = at.id;
      return run(() => api.moveProject(id, parent, AT_END));
    }
    const parent = byId.get(at.id)?.project.parent_id ?? null;
    const index = siblingsOf(parent).indexOf(at.id);
    return run(() => api.moveProject(id, parent, index + (at.zone === 'after' ? 1 : 0)));
  }
</script>

<nav class="sidebar-glass flex h-full flex-col">
  <!-- Space for the macOS traffic lights; draggable like a title bar. -->
  <div class="h-11 shrink-0" data-tauri-drag-region></div>

  <div class="flex gap-1.5 px-3 pb-3">
    <button
      class="flex h-8 min-w-0 flex-1 items-center gap-2 rounded-lg border border-line-soft bg-bg/50 px-2.5 text-muted transition-colors duration-150 hover:border-line hover:text-text"
      onclick={() => (ws.palette = 'open')}>
      <Search size={14} class="shrink-0" /> <span class="truncate">Suchen</span>
      <span class="kbd ml-auto">⌘K</span>
    </button>
    <button class="btn-primary !h-8 !w-8 shrink-0 !px-0" aria-label="Neuer Eintrag" title="Neuer Eintrag (⌘N)" onclick={() => ws.createNode('Neuer Eintrag')}>
      <Plus size={15} />
    </button>
  </div>

  <div class="flex-1 overflow-y-auto px-2">
    <button
      class="nav-row {ws.view === null ? 'active' : ''}"
      onclick={() => ws.setView(null)}>
      <Network size={14} class={ws.view === null ? 'text-accent' : ''} /> Alles
    </button>

    {#if ws.favorites.length}
      <div class="mt-5 mb-1 pl-2"><span class="section-title !mb-0">Favoriten</span></div>
      {#each ws.favorites as favorite (favorite.id)}
        <button class="nav-row {ws.selected === favorite.id ? 'active' : ''}" onclick={() => ws.select(favorite.id, true)}>
          <Star size={13} class="shrink-0 fill-warn/70 text-warn" /> <span class="truncate">{favorite.title}</span>
        </button>
      {/each}
    {/if}

    <!-- Dropping on the heading moves a project to the top level. -->
    <div class="mt-5 mb-1 flex items-center rounded-lg pr-1 pl-2 transition-colors {dropAt?.id === null && dragging ? 'bg-accent/15' : ''}"
      role="presentation"
      ondragover={(e) => {
        if (dragging && byId.get(dragging)?.project.parent_id) {
          e.preventDefault();
          dropAt = { id: null, zone: 'inside' };
        }
      }}
      ondragleave={() => dropAt?.id === null && (dropAt = null)}
      ondrop={(e) => (e.preventDefault(), drop())}>
      <span class="section-title !mb-0 flex-1">{dragging && byId.get(dragging)?.project.parent_id ? 'Hierher: oberste Ebene' : 'Projekte'}</span>
      <button class="icon-btn !h-6 !w-6" aria-label="Neues Projekt" title="Neues Projekt" onclick={() => (creatingUnder = null)}><FolderPlus size={13} /></button>
    </div>

    {#if creatingUnder === null}
      <form class="px-1 pb-1" onsubmit={createProject} in:pop={{ duration: 140, y: -3 }}>
        <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
        <input class="field !h-7.5 w-full" placeholder="Projektname" bind:value={newName} autofocus onblur={() => (creatingUnder = undefined)} />
      </form>
    {/if}

    {#each visible as { project } (project.id)}
      {@const hasChildren = childrenOf.has(project.id)}
      {@const isCollapsed = collapsed.has(project.id)}
      <div transition:slide={{ duration: 150 }}>
        <div class="group relative {dragging === project.id ? 'opacity-40' : ''}" style="padding-left: {depth(project.id) * INDENT_PX}px"
          role="listitem"
          draggable={renaming !== project.id}
          ondragstart={(e) => {
            dragging = project.id;
            if (e.dataTransfer) {
              e.dataTransfer.effectAllowed = 'move';
              e.dataTransfer.setData('text/plain', project.name);
            }
          }}
          ondragend={() => ((dragging = null), (dropAt = null))}
          ondragover={(e) => onDragOver(e, project.id)}
          ondragleave={(e) => {
            if (dropAt?.id === project.id && !(e.currentTarget as HTMLElement).contains(e.relatedTarget as Node)) dropAt = null;
          }}
          ondrop={(e) => (e.preventDefault(), e.stopPropagation(), drop())}>
          {#if dropAt?.id === project.id && dropAt.zone !== 'inside'}
            <div class="pointer-events-none absolute right-1 z-10 h-0.5 rounded-full bg-accent {dropAt.zone === 'before' ? '-top-px' : '-bottom-px'}"
              style="left: {depth(project.id) * INDENT_PX + 8}px"></div>
          {/if}
          {#if renaming === project.id}
            <div class="flex h-8 items-center pl-1">
              <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
              <input class="field !h-7 w-full" aria-label="Neuer Name" bind:value={renameValue} autofocus onblur={commitRename} onkeydown={onRenameKey}
                onfocus={(e) => e.currentTarget.select()} />
            </div>
          {:else}
            <!-- The space for the arrow stays empty without subprojects, so the dots line up. -->
            <button class="nav-row !pl-5.5 group-hover:!pr-20 {ws.hiddenProjects.has(project.id) ? 'opacity-50' : ''} {ws.view === project.id ? 'active' : ''} {dropAt?.id === project.id && dropAt.zone === 'inside' ? '!bg-accent/20 ring-1 ring-accent/60' : ''}"
              onclick={() => ws.setView(project.id)} ondblclick={() => startRename(project)}>
              <span class="h-2 w-2 shrink-0 rounded-full" style="background: {ws.projectColor.get(project.id)}"></span>
              <span class="truncate">{project.name}</span>
              <span class="ml-auto pl-2 text-[11px] text-faint tabular-nums group-hover:hidden {menu?.project.id === project.id ? 'hidden' : ''}">{count(project.id)}</span>
            </button>
          {/if}
          {#if hasChildren && renaming !== project.id}
            <button class="absolute top-1.5 flex h-5 w-5 items-center justify-center rounded text-faint transition-colors hover:bg-raised hover:text-text"
              style="left: {depth(project.id) * INDENT_PX + 2}px"
              aria-label="{isCollapsed ? 'Ausklappen' : 'Einklappen'}: {project.name}" aria-expanded={!isCollapsed}
              onclick={() => toggle(project.id)}>
              <ChevronRight size={13} class="transition-transform duration-150 ease-(--ease-out-soft) {isCollapsed ? '' : 'rotate-90'}" />
            </button>
          {/if}
          {#if renaming !== project.id}
            {@const isHidden = ws.hiddenProjects.has(project.id)}
            <button class="icon-btn absolute top-1 right-13 !h-6 !w-6 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              aria-label="{isHidden ? 'Einblenden' : 'Ausblenden'}: {project.name}" aria-pressed={isHidden} title={isHidden ? 'Einblenden' : 'Ausblenden'}
              onclick={() => ws.toggleProjectHidden(project.id)}>
              {#if isHidden}<Eye size={12} />{:else}<EyeOff size={12} />{/if}
            </button>
            <button class="icon-btn absolute top-1 right-7 !h-6 !w-6 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              aria-label="Unterprojekt in {project.name} anlegen" title="Unterprojekt" onclick={() => (reveal(project.id, true), (creatingUnder = project.id))}>
              <Plus size={12} />
            </button>
            <button class="icon-btn absolute top-1 right-1 !h-6 !w-6 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 {menu?.project.id === project.id ? '!opacity-100' : ''}"
              aria-label="Mehr zu {project.name}" title="Mehr" aria-haspopup="menu" onclick={(e) => openMenu(project, e)}>
              <Ellipsis size={13} />
            </button>
          {/if}
        </div>
        {#if creatingUnder === project.id}
          <form class="py-1 pr-1" style="padding-left: {(depth(project.id) + 1) * INDENT_PX + 4}px" onsubmit={createProject} in:pop={{ duration: 140, y: -3 }}>
            <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
            <input class="field !h-7.5 w-full" placeholder="Unterprojekt" bind:value={newName} autofocus onblur={() => (creatingUnder = undefined)} />
          </form>
        {/if}
      </div>
    {/each}

    {#if ws.hiddenProjectCount}
      <div class="mt-1 flex items-center gap-1 pr-1 pl-2 text-xs text-faint" in:pop={{ duration: 140, y: -3 }}>
        <button class="flex h-6.5 min-w-0 flex-1 items-center gap-1.5 text-left transition-colors hover:text-text" aria-expanded={showHidden} onclick={() => (showHidden = !showHidden)}>
          <EyeOff size={12} class="shrink-0" /> <span class="truncate">{ws.hiddenProjectCount} ausgeblendet</span>
          <ChevronRight size={12} class="shrink-0 transition-transform duration-150 ease-(--ease-out-soft) {showHidden ? 'rotate-90' : ''}" />
        </button>
        <button class="shrink-0 transition-colors hover:text-text" onclick={() => ws.showAllProjects()}>Alle zeigen</button>
      </div>
    {/if}

    {#if ws.tagCounts.length}
      <div class="mt-5 mb-1 pl-2"><span class="section-title !mb-0">Tags</span></div>
      <div class="flex flex-wrap gap-1 px-1.5 pb-2">
        {#each ws.tagCounts.slice(0, MAX_TAGS) as { tag, count: uses } (tag)}
          {@const active = ws.filter.tags.includes(tag)}
          <button class="flex h-6 max-w-full items-center gap-1 rounded-md px-1.5 text-xs transition-colors {active ? 'bg-accent/15 text-text' : 'text-muted hover:bg-elevated hover:text-text'}"
            aria-pressed={active} title="Im Graph filtern" onclick={() => ws.toggleFilter('tags', tag)}>
            <span class="truncate">#{tag}</span><span class="text-[10px] text-faint tabular-nums">{uses}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <div class="px-2 pb-1.5">
    {#if ws.update}
      <button class="nav-row !text-accent" in:pop={{ duration: 200, y: 4 }} onclick={() => (ws.settingsOpen = true)}>
        <ArrowUpCircle size={14} /> Update auf {ws.update.version}
      </button>
    {/if}
    <button class="nav-row {ws.panel === 'library' ? 'active' : ''}" onclick={() => ((ws.panel = ws.panel === 'library' ? null : 'library'), ws.select(null))}>
      <BookMarked size={14} class={ws.panel === 'library' ? 'text-accent' : ''} /> Bibliothek
    </button>
    <button class="nav-row {ws.panel === 'trash' ? 'active' : ''}" onclick={() => ((ws.panel = ws.panel === 'trash' ? null : 'trash'), ws.select(null))}>
      <Trash2 size={14} class={ws.panel === 'trash' ? 'text-accent' : ''} /> Papierkorb
    </button>
  </div>

  {#if menu}
    <ProjectMenu project={menu.project} x={menu.x} y={menu.y} {depth}
      onclose={() => (menu = null)}
      onrename={() => menu && startRename(menu.project)}
      onsubproject={() => {
        const { id } = menu!.project;
        menu = null;
        reveal(id, true);
        creatingUnder = id;
      }}
      onmove={(parent) => {
        const { id } = menu!.project;
        menu = null;
        if (parent) reveal(parent, true);
        run(() => api.moveProject(id, parent, AT_END));
      }}
      ondelete={() => {
        deleting = menu!.project;
        menu = null;
      }} />
  {/if}
  {#if deleting}
    <DeleteProjectDialog project={deleting} onclose={() => (deleting = null)} />
  {/if}

  <footer class="flex items-center gap-1 border-t border-line-soft px-3 py-2">
    <div class="mr-1 flex h-6.5 w-6.5 shrink-0 items-center justify-center rounded-full bg-accent/15 text-[11px] font-semibold text-accent">
      {(userName ?? '?').slice(0, 1).toUpperCase()}
    </div>
    <span class="min-w-0 flex-1 truncate text-muted">{userName}</span>
    <button class="icon-btn relative {ws.panel === 'review' ? '!bg-accent/15 !text-accent' : ''}" aria-label="Prüfen" title="Prüfen"
      onclick={() => ((ws.panel = ws.panel === 'review' ? null : 'review'), ws.select(null))}>
      <ListChecks size={15} />
      {#if ws.unreviewed.length}
        <span class="absolute top-0.5 right-0.5 h-1.5 w-1.5 rounded-full bg-ai" in:pop={{ duration: 160, y: 0 }}></span>
      {/if}
    </button>
    <button class="icon-btn {ws.panel === 'activity' ? '!bg-accent/15 !text-accent' : ''}" aria-label="Aktivität" title="Aktivität"
      onclick={() => ((ws.panel = ws.panel === 'activity' ? null : 'activity'), ws.select(null))}><Activity size={15} /></button>
    <button class="icon-btn" aria-label="Einstellungen" title="Einstellungen (⌘,)" onclick={() => (ws.settingsOpen = true)}><Settings size={15} /></button>
    <button class="icon-btn" aria-label="Sperren" title="Sperren (⌘⇧L)" onclick={() => api.lock()}><Lock size={15} /></button>
  </footer>
</nav>
