<script lang="ts">
  import { Network, FolderPlus, Lock, Search, Plus, Activity, Settings } from '@lucide/svelte';
  import { api, errorMessage, type Id } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';

  let { userName }: { userName: string | null } = $props();

  let creatingUnder = $state<Id | null | undefined>(undefined);
  let newName = $state('');

  const depth = (id: Id): number => {
    const p = ws.projects.find((x) => x.project.id === id)?.project;
    return p?.parent_id ? 1 + depth(p.parent_id) : 0;
  };

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

    {#each ws.projects as { project, node_count } (project.id)}
      <div class="group relative" style="padding-left: {depth(project.id) * 12}px">
        <button class="nav-row {ws.view === project.id ? 'active' : ''}" onclick={() => ws.setView(project.id)}>
          <span class="h-2 w-2 shrink-0 rounded-full" style="background: {ws.projectColor.get(project.id)}"></span>
          <span class="truncate">{project.name}</span>
          <span class="ml-auto pl-2 text-[11px] text-faint tabular-nums transition-opacity group-hover:opacity-0">{node_count}</span>
        </button>
        <button class="icon-btn absolute top-1 right-1 !h-6 !w-6 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
          aria-label="Unterprojekt anlegen" title="Unterprojekt" onclick={() => (creatingUnder = project.id)}>
          <Plus size={12} />
        </button>
      </div>
      {#if creatingUnder === project.id}
        <form class="py-1 pr-1" style="padding-left: {(depth(project.id) + 1) * 12 + 4}px" onsubmit={createProject} in:pop={{ duration: 140, y: -3 }}>
          <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
          <input class="field !h-7.5 w-full" placeholder="Unterprojekt" bind:value={newName} autofocus onblur={() => (creatingUnder = undefined)} />
        </form>
      {/if}
    {/each}
  </div>

  <footer class="flex items-center gap-1 border-t border-line-soft px-3 py-2">
    <div class="mr-1 flex h-6.5 w-6.5 shrink-0 items-center justify-center rounded-full bg-accent/15 text-[11px] font-semibold text-accent">
      {(userName ?? '?').slice(0, 1).toUpperCase()}
    </div>
    <span class="min-w-0 flex-1 truncate text-muted">{userName}</span>
    <button class="icon-btn {ws.panel === 'activity' ? '!bg-accent/15 !text-accent' : ''}" aria-label="Aktivität" title="Aktivität"
      onclick={() => ((ws.panel = ws.panel === 'activity' ? null : 'activity'), ws.select(null))}><Activity size={15} /></button>
    <button class="icon-btn" aria-label="Einstellungen" title="Einstellungen (⌘,)" onclick={() => (ws.settingsOpen = true)}><Settings size={15} /></button>
    <button class="icon-btn" aria-label="Sperren" title="Sperren (⌘⇧L)" onclick={() => api.lock()}><Lock size={15} /></button>
  </footer>
</nav>
