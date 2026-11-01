<script lang="ts">
  import { ArrowLeft, CornerLeftUp, Eye, EyeOff, Focus, FolderInput, FolderPlus, Pencil, Trash2 } from '@lucide/svelte';
  import type { Id, Project } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';

  let {
    project,
    x,
    y,
    depth,
    onclose,
    onrename,
    onsubproject,
    onmove,
    ondelete,
  }: {
    project: Project;
    x: number;
    y: number;
    depth: (id: Id) => number;
    onclose: () => void;
    onrename: () => void;
    onsubproject: () => void;
    onmove: (parent: Id | null) => void;
    ondelete: () => void;
  } = $props();

  const WIDTH = 216;
  const MARGIN = 8;
  const INDENT_PX = 10;

  let moving = $state(false);
  let panel: HTMLDivElement | undefined = $state();
  let height = $state(0);

  // Everything but itself, what is below it, and where it already is.
  const targets = $derived.by(() => {
    const own = ws.subtreeOf(project.id);
    return ws.projects.filter((p) => !own.has(p.project.id) && p.project.id !== project.parent_id);
  });

  const left = $derived(Math.min(x, window.innerWidth - WIDTH - MARGIN));
  const top = $derived(Math.max(MARGIN, Math.min(y, window.innerHeight - height - MARGIN)));

  function onkeydowncapture(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      if (moving) moving = false;
      else onclose();
    }
  }
</script>

<svelte:window {onkeydowncapture} />

<div class="fixed inset-0 z-40" role="presentation" onclick={onclose}></div>
<div class="surface fixed z-50 rounded-xl p-1" style="left: {left}px; top: {top}px; width: {WIDTH}px" role="menu" aria-label="Projekt „{project.name}“" tabindex="-1"
  bind:this={panel} bind:clientHeight={height} transition:pop={{ duration: 130, y: -3 }}>
  {#if moving}
    <button class="menu-row text-muted" role="menuitem" onclick={() => (moving = false)}><ArrowLeft size={13} /> Verschieben nach …</button>
    <div class="my-1 h-px bg-line-soft"></div>
    <div class="max-h-64 overflow-y-auto">
      {#if project.parent_id}
        <button class="menu-row" role="menuitem" onclick={() => onmove(null)}><CornerLeftUp size={13} class="text-faint" /> Oberste Ebene</button>
      {/if}
      {#each targets as { project: target } (target.id)}
        <button class="menu-row" role="menuitem" style="padding-left: {8 + depth(target.id) * INDENT_PX}px" onclick={() => onmove(target.id)}>
          <span class="h-2 w-2 shrink-0 rounded-full" style="background: {ws.projectColor.get(target.id)}"></span>
          <span class="truncate">{target.name}</span>
        </button>
      {:else}
        {#if !project.parent_id}
          <p class="px-2 py-2 text-xs text-faint">Es gibt noch kein anderes Projekt.</p>
        {/if}
      {/each}
    </div>
  {:else}
    <button class="menu-row" role="menuitem" onclick={onrename}><Pencil size={13} class="text-faint" /> Umbenennen</button>
    <button class="menu-row" role="menuitem" onclick={onsubproject}><FolderPlus size={13} class="text-faint" /> Unterprojekt anlegen</button>
    <button class="menu-row" role="menuitem" onclick={() => (moving = true)}><FolderInput size={13} class="text-faint" /> Verschieben nach …</button>
    <div class="my-1 h-px bg-line-soft"></div>
    <button class="menu-row" role="menuitem" onclick={() => (ws.toggleProjectHidden(project.id), onclose())}>
      {#if ws.hiddenProjects.has(project.id)}<Eye size={13} class="text-faint" /> Einblenden{:else}<EyeOff size={13} class="text-faint" /> Ausblenden{/if}
    </button>
    <button class="menu-row" role="menuitem" onclick={() => (ws.showOnlyProject(project.id), onclose())}><Focus size={13} class="text-faint" /> Nur dieses anzeigen</button>
    <div class="my-1 h-px bg-line-soft"></div>
    <button class="menu-row hover:!bg-danger/12 hover:!text-danger" role="menuitem" onclick={ondelete}><Trash2 size={13} /> Löschen …</button>
  {/if}
</div>

<style>
  .menu-row {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 8px;
    border-radius: 8px;
    padding: 6px 8px;
    text-align: left;
    transition: background-color 100ms;
  }
  .menu-row:hover {
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
  }
</style>
