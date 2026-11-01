<script lang="ts">
  import { tick } from 'svelte';
  import { Plus } from '@lucide/svelte';
  import type { Id } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';

  // Choosing a project, wherever one is assigned: type to narrow it down.
  let {
    exclude,
    onpick,
    oncreate,
    onclose,
    top,
    placeholder = 'Projekt suchen …',
  }: {
    /** Projects that cannot be chosen here. */
    exclude?: (id: Id) => boolean;
    onpick: (id: Id | null) => void;
    /** Offers to create what was searched for and did not exist. */
    oncreate?: (name: string) => void;
    onclose?: () => void;
    /** A choice above the projects, such as "top level". */
    top?: string;
    placeholder?: string;
  } = $props();

  let query = $state('');
  let index = $state(0);
  let list: HTMLDivElement | undefined = $state();

  const byId = $derived(new Map(ws.projects.map((p) => [p.project.id, p.project])));

  /** The way to a project, so that two with the same name can be told apart. */
  function pathTo(id: Id) {
    const names: string[] = [];
    for (let p = byId.get(id)?.parent_id ?? null; p; p = byId.get(p)?.parent_id ?? null) names.unshift(byId.get(p)?.name ?? '');
    return names.join(' › ');
  }

  type Option = { id: Id | null; name: string; path: string; create?: boolean };
  const options = $derived.by<Option[]>(() => {
    const q = query.trim().toLowerCase();
    const found = ws.projects
      .filter((p) => !exclude?.(p.project.id))
      .map((p) => ({ id: p.project.id as Id | null, name: p.project.name, path: pathTo(p.project.id) }))
      .filter((p) => !q || p.name.toLowerCase().includes(q) || p.path.toLowerCase().includes(q))
      .sort((a, b) => (q ? Number(!a.name.toLowerCase().startsWith(q)) - Number(!b.name.toLowerCase().startsWith(q)) : 0));
    const start: Option[] = top && !q ? [{ id: null, name: top, path: '' }] : [];
    const exact = found.some((p) => p.name.toLowerCase() === q);
    const end: Option[] = oncreate && q && !exact ? [{ id: null, name: query.trim(), path: '', create: true }] : [];
    return [...start, ...found, ...end];
  });

  function choose(option: Option | undefined) {
    if (!option) return;
    // Ready for the next one, where several can be chosen.
    query = '';
    index = 0;
    if (option.create) oncreate?.(option.name);
    else onpick(option.id);
  }

  async function move(by: number) {
    index = Math.max(0, Math.min(options.length - 1, index + by));
    await tick();
    list?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') move(1);
    else if (e.key === 'ArrowUp') move(-1);
    else if (e.key === 'Enter') choose(options[index]);
    else if (e.key === 'Escape' && onclose) {
      e.stopPropagation();
      onclose();
    } else return;
    e.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_autofocus -- this appears because the user wants to pick something now -->
<input class="w-full border-b border-line-soft bg-transparent px-2.5 pt-1.5 pb-2 text-[13px] outline-none placeholder:text-faint"
  {placeholder} aria-label={placeholder} autofocus bind:value={query} oninput={() => (index = 0)} {onkeydown} />
<div class="max-h-52 overflow-y-auto pt-1" role="listbox" aria-label="Projekte" bind:this={list}>
  {#each options as option, i (option.create ? 'create' : (option.id ?? 'top'))}
    <button type="button" class="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left transition-colors duration-75 {i === index ? 'bg-text/8 text-text' : 'text-muted'}"
      role="option" aria-selected={i === index} onmouseenter={() => (index = i)} onclick={() => choose(option)}>
      {#if option.create}
        <Plus size={12} class="shrink-0 text-faint" /> <span class="truncate">„{option.name}“ neu anlegen</span>
      {:else}
        {#if option.id}<span class="h-2 w-2 shrink-0 rounded-full" style="background: {ws.projectColor.get(option.id)}"></span>{/if}
        <span class="truncate">{option.name}</span>
        {#if option.path}<span class="ml-auto truncate pl-2 text-[11px] text-faint">{option.path}</span>{/if}
      {/if}
    </button>
  {:else}
    <p class="px-2.5 py-2 text-xs text-faint">Kein Projekt gefunden.</p>
  {/each}
</div>
