<script lang="ts">
  import { untrack } from 'svelte';
  import { slide } from 'svelte/transition';
  import { ArrowUpCircle, ChevronRight, Network, FolderPlus, Lock, Search, Plus, Activity, Settings, ListChecks, Trash2 } from '@lucide/svelte';
  import { api, errorMessage, type Id } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';

  let { userName }: { userName: string | null } = $props();

  const COLLAPSED_KEY = 'bm.collapsedProjects';
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
  const visible = $derived(ws.projects.filter((p) => !ancestors(p.project.id).some((a) => collapsed.has(a))));

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

    <div class="mt-5 mb-1 flex items-center pr-1 pl-2">
      <span class="section-title !mb-0 flex-1">Projekte</span>
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
        <div class="group relative" style="padding-left: {depth(project.id) * INDENT_PX}px">
          <!-- The space for the arrow stays empty without subprojects, so the dots line up. -->
          <button class="nav-row !pl-5.5 {ws.view === project.id ? 'active' : ''}" onclick={() => ws.setView(project.id)}>
            <span class="h-2 w-2 shrink-0 rounded-full" style="background: {ws.projectColor.get(project.id)}"></span>
            <span class="truncate">{project.name}</span>
            <span class="ml-auto pl-2 text-[11px] text-faint tabular-nums transition-opacity group-hover:opacity-0">{count(project.id)}</span>
          </button>
          {#if hasChildren}
            <button class="absolute top-1.5 flex h-5 w-5 items-center justify-center rounded text-faint transition-colors hover:bg-raised hover:text-text"
              style="left: {depth(project.id) * INDENT_PX + 2}px"
              aria-label="{isCollapsed ? 'Ausklappen' : 'Einklappen'}: {project.name}" aria-expanded={!isCollapsed}
              onclick={() => toggle(project.id)}>
              <ChevronRight size={13} class="transition-transform duration-150 ease-(--ease-out-soft) {isCollapsed ? '' : 'rotate-90'}" />
            </button>
          {/if}
          <button class="icon-btn absolute top-1 right-1 !h-6 !w-6 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
            aria-label="Unterprojekt in {project.name} anlegen" title="Unterprojekt" onclick={() => (reveal(project.id, true), (creatingUnder = project.id))}>
            <Plus size={12} />
          </button>
        </div>
        {#if creatingUnder === project.id}
          <form class="py-1 pr-1" style="padding-left: {(depth(project.id) + 1) * INDENT_PX + 4}px" onsubmit={createProject} in:pop={{ duration: 140, y: -3 }}>
            <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
            <input class="field !h-7.5 w-full" placeholder="Unterprojekt" bind:value={newName} autofocus onblur={() => (creatingUnder = undefined)} />
          </form>
        {/if}
      </div>
    {/each}
  </div>

  <div class="px-2 pb-1.5">
    {#if ws.update}
      <button class="nav-row !text-accent" in:pop={{ duration: 200, y: 4 }} onclick={() => (ws.settingsOpen = true)}>
        <ArrowUpCircle size={14} /> Update auf {ws.update.version}
      </button>
    {/if}
    <button class="nav-row {ws.panel === 'trash' ? 'active' : ''}" onclick={() => ((ws.panel = ws.panel === 'trash' ? null : 'trash'), ws.select(null))}>
      <Trash2 size={14} class={ws.panel === 'trash' ? 'text-accent' : ''} /> Papierkorb
    </button>
  </div>

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
