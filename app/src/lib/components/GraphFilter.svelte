<script lang="ts">
  import { ListFilter, X } from '@lucide/svelte';
  import { KIND_COLORS, ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';

  const MAX_TAGS = 16;
  const DEPTHS: { value: number | null; label: string }[] = [
    { value: null, label: 'Aus' },
    { value: 1, label: '1' },
    { value: 2, label: '2' },
    { value: 3, label: '3' },
  ];

  let open = $state(false);

  const kinds = $derived([...new Set((ws.graph?.nodes ?? []).map((n) => n.kind))].sort());
  // Only people who wrote something in this view are useful to filter by.
  const authors = $derived.by(() => {
    const present = new Set((ws.graph?.nodes ?? []).map((n) => n.created_by));
    return ws.actors.filter((a) => present.has(a.id));
  });
  const shown = $derived((ws.graph?.nodes.length ?? 0) - ws.hiddenNodes.size);

  let viewName = $state('');

  function saveView(e: SubmitEvent) {
    e.preventDefault();
    ws.saveCurrentView(viewName);
    ws.notify(`Ansicht „${viewName.trim()}“ gespeichert`);
    viewName = '';
  }

  function onkeydowncapture(e: KeyboardEvent) {
    if (open && e.key === 'Escape') {
      e.stopPropagation();
      open = false;
    }
  }
</script>

<svelte:window {onkeydowncapture} />

<div class="relative">
  <button class="glass-pill !h-8 gap-1.5 !px-2.5 text-xs transition-colors {ws.activeFilters ? '!border-accent/50 text-accent' : 'text-muted hover:text-text'}"
    aria-expanded={open} title="Graph filtern" onclick={() => (open = !open)}>
    <ListFilter size={13} /> Filter
    {#if ws.activeFilters}
      <span class="flex h-4 min-w-4 items-center justify-center rounded-full bg-accent/20 px-1 text-[10px] font-semibold tabular-nums">{ws.activeFilters}</span>
    {/if}
  </button>

  {#if open}
    <div class="fixed inset-0 z-30" role="presentation" onclick={() => (open = false)}></div>
    <div class="surface absolute bottom-10 left-0 z-40 w-80 rounded-xl p-3" role="dialog" aria-label="Graph filtern" tabindex="-1" transition:pop={{ duration: 150, y: 6 }}>
      <div class="mb-2.5 flex items-center">
        <span class="flex-1 text-xs text-muted tabular-nums">{shown} von {ws.graph?.nodes.length ?? 0} Einträgen</span>
        {#if ws.activeFilters}
          <button class="flex items-center gap-1 text-xs text-faint transition-colors hover:text-text" onclick={() => ws.clearFilters()}><X size={12} /> Zurücksetzen</button>
        {/if}
      </div>

      {#snippet chip(label: string, active: boolean, onclick: () => void, color?: string)}
        <button class="flex h-6.5 max-w-full items-center gap-1.5 rounded-md border px-2 text-xs transition-colors {active ? 'border-accent/50 bg-accent/15 text-text' : 'border-line-soft text-muted hover:border-line hover:text-text'}"
          aria-pressed={active} {onclick}>
          {#if color}<span class="h-1.5 w-1.5 shrink-0 rounded-full" style="background: {color}"></span>{/if}
          <span class="truncate">{label}</span>
        </button>
      {/snippet}

      {#if ws.projects.length > 1}
        <div class="section-title">Projekte</div>
        <div class="mb-3 flex max-h-24 flex-wrap gap-1 overflow-y-auto">
          {#each ws.projects as { project } (project.id)}
            {@render chip(project.name, !ws.hiddenProjects.has(project.id), () => ws.toggleProjectHidden(project.id), ws.projectColor.get(project.id))}
          {/each}
        </div>
      {/if}

      <div class="section-title">Art</div>
      <div class="mb-3 flex flex-wrap gap-1">
        {#each kinds as kind (kind)}
          {@render chip(kind, ws.filter.kinds.includes(kind), () => ws.toggleFilter('kinds', kind), KIND_COLORS[kind] ?? KIND_COLORS.note)}
        {/each}
      </div>

      {#if ws.tagCounts.length}
        <div class="section-title">Tags</div>
        <div class="mb-3 flex flex-wrap gap-1">
          {#each ws.tagCounts.slice(0, MAX_TAGS) as { tag } (tag)}
            {@render chip(`#${tag}`, ws.filter.tags.includes(tag), () => ws.toggleFilter('tags', tag))}
          {/each}
        </div>
      {/if}

      {#if authors.length > 1}
        <div class="section-title">Geschrieben von</div>
        <div class="mb-3 flex flex-wrap gap-1">
          {#each authors as actor (actor.id)}
            {@render chip(actor.name, ws.filter.actors.includes(actor.id), () => ws.toggleFilter('actors', actor.id), actor.kind === 'ai' ? 'var(--color-ai)' : undefined)}
          {/each}
        </div>
      {/if}

      <div class="section-title">Nur die Nachbarschaft</div>
      <div class="flex items-center gap-2">
        <div class="flex gap-1">
          {#each DEPTHS as depth (depth.label)}
            {@render chip(depth.label, ws.neighbourhood === depth.value, () => (ws.neighbourhood = depth.value))}
          {/each}
        </div>
        <span class="text-xs text-faint">{ws.selected ? 'Verbindungen um die Auswahl' : 'Wähle zuerst einen Eintrag'}</span>
      </div>

      <form class="mt-3 flex gap-1.5 border-t border-line-soft pt-3" onsubmit={saveView}>
        <input class="field !h-7.5 min-w-0 flex-1 text-xs" placeholder="Name für diese Ansicht" aria-label="Name für diese Ansicht" bind:value={viewName} />
        <button class="btn-ghost !h-7.5 shrink-0 text-xs" disabled={!viewName.trim()}>Speichern</button>
      </form>
    </div>
  {/if}
</div>
