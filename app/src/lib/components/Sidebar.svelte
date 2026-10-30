<script lang="ts">
  import { Network, FolderPlus, Lock, Search, Plus, Activity, Settings } from '@lucide/svelte';
  import { api, errorMessage, type Id } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';

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

<nav class="flex h-full flex-col bg-panel/80">
  <!-- Space for the macOS traffic lights; draggable like a title bar. -->
  <div class="h-10 shrink-0" data-tauri-drag-region></div>

  <div class="flex gap-1 px-2 pb-2">
    <button class="btn-ghost flex-1 !justify-start !py-1.5 text-muted" onclick={() => (ws.palette = 'open')}>
      <Search size={14} /> Suchen <span class="ml-auto text-[11px] text-faint">⌘K</span>
    </button>
    <button class="btn-ghost !px-2 !py-1.5" title="Neuer Eintrag (⌘N)" onclick={() => ws.createNode('Neuer Eintrag')}>
      <Plus size={14} />
    </button>
  </div>

  <div class="flex-1 overflow-y-auto px-2">
    <button
      class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 {ws.view === null ? 'bg-elevated text-text' : 'text-muted hover:bg-elevated/60'}"
      onclick={() => ws.setView(null)}>
      <Network size={14} /> Alles
    </button>

    <div class="mt-4 flex items-center px-2">
      <span class="section-title flex-1 !mb-0">Projekte</span>
      <button class="icon-btn !h-6 !w-6" title="Neues Projekt" onclick={() => (creatingUnder = null)}><FolderPlus size={13} /></button>
    </div>

    {#if creatingUnder === null}
      <form class="px-1 py-1" onsubmit={createProject}>
        <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
        <input class="field w-full !py-1" placeholder="Projektname" bind:value={newName} autofocus onblur={() => (creatingUnder = undefined)} />
      </form>
    {/if}

    {#each ws.projects as { project, node_count } (project.id)}
      <div class="group flex items-center" style="padding-left: {depth(project.id) * 14}px">
        <button
          class="flex min-w-0 flex-1 items-center gap-2 rounded-md px-2 py-1.5 {ws.view === project.id ? 'bg-elevated text-text' : 'text-muted hover:bg-elevated/60'}"
          onclick={() => ws.setView(project.id)}>
          <span class="h-2 w-2 shrink-0 rounded-full" style="background: {ws.projectColor.get(project.id)}"></span>
          <span class="truncate">{project.name}</span>
          <span class="ml-auto text-[11px] text-faint">{node_count}</span>
        </button>
        <button class="icon-btn !hidden !h-6 !w-6 group-hover:!inline-flex" title="Unterprojekt" onclick={() => (creatingUnder = project.id)}>
          <Plus size={12} />
        </button>
      </div>
      {#if creatingUnder === project.id}
        <form class="py-1" style="padding-left: {(depth(project.id) + 1) * 14 + 4}px" onsubmit={createProject}>
          <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
          <input class="field w-full !py-1" placeholder="Unterprojekt" bind:value={newName} autofocus onblur={() => (creatingUnder = undefined)} />
        </form>
      {/if}
    {/each}
  </div>

  <footer class="flex items-center gap-2 border-t border-line px-3 py-2">
    <div class="flex h-6 w-6 items-center justify-center rounded-full bg-accent/20 text-[11px] font-semibold text-accent">
      {(userName ?? '?').slice(0, 1).toUpperCase()}
    </div>
    <span class="flex-1 truncate text-muted">{userName}</span>
    <button class="icon-btn {ws.panel === 'activity' ? '!text-accent' : ''}" title="Aktivität"
      onclick={() => ((ws.panel = ws.panel === 'activity' ? null : 'activity'), ws.select(null))}><Activity size={14} /></button>
    <button class="icon-btn" title="Einstellungen (⌘,)" onclick={() => (ws.settingsOpen = true)}><Settings size={14} /></button>
    <button class="icon-btn" title="Sperren (⌘⇧L)" onclick={() => api.lock()}><Lock size={14} /></button>
  </footer>
</nav>
