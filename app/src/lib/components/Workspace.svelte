<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { ChevronRight, Sparkles } from '@lucide/svelte';
  import { api, type Id, type Status } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import Sidebar from './Sidebar.svelte';
  import GraphView from './GraphView.svelte';
  import Inspector from './Inspector.svelte';
  import CommandPalette from './CommandPalette.svelte';

  let { status }: { status: Status } = $props();

  const KIND_COLORS: Record<string, string> = {
    note: '#8f9bb3',
    concept: '#5fd3a5',
    task: '#f5c451',
    person: '#ff6fb5',
    source: '#4cc9f0',
    decision: '#ffa24c',
    snippet: '#c38bff',
  };

  function colorFor(projects: Id[], kind: string) {
    // Inside a project view, colour by subproject; otherwise by first project.
    const p = projects.find((id) => id !== ws.view) ?? projects[0];
    return (p && ws.projectColor.get(p)) || KIND_COLORS[kind] || '#8f9bb3';
  }

  let lastTouch = 0;
  function activity() {
    const now = Date.now();
    if (now - lastTouch > 30_000) {
      lastTouch = now;
      api.touch().catch(() => {});
    }
  }

  function onkeydown(e: KeyboardEvent) {
    activity();
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === 'k') {
      e.preventDefault();
      ws.palette = ws.palette ? null : 'open';
    } else if (mod && e.key === 'n') {
      e.preventDefault();
      ws.createNode('Neuer Eintrag');
    } else if (mod && e.key === 'l' && !e.shiftKey && ws.selected) {
      e.preventDefault();
      ws.palette = 'link';
    } else if (mod && e.shiftKey && e.key.toLowerCase() === 'l') {
      e.preventDefault();
      api.lock();
    } else if (e.key === 'Escape' && !ws.palette) {
      const tag = (e.target as HTMLElement).tagName;
      if (tag !== 'INPUT' && tag !== 'TEXTAREA' && !(e.target as HTMLElement).isContentEditable) ws.select(null);
    }
  }

  onMount(() => ws.start());
  onDestroy(() => ws.stop());
</script>

<svelte:window {onkeydown} onmousemove={activity} />

<div class="grid h-full" style="grid-template-columns: 232px 1fr {ws.detail ? '400px' : '0px'}">
  <Sidebar userName={status.user_name} />

  <main class="relative flex min-w-0 flex-col border-l border-line">
    <header class="relative z-10 flex h-10 shrink-0 items-center gap-1 px-4 text-muted" data-tauri-drag-region>
      <button class="hover:text-text" onclick={() => ws.setView(null)}>Alles</button>
      {#each ws.breadcrumb as p (p.id)}
        <ChevronRight size={13} class="text-faint" />
        <button class="hover:text-text {p.id === ws.view ? 'text-text' : ''}" onclick={() => ws.setView(p.id)}>{p.name}</button>
      {/each}
      <div class="flex-1" data-tauri-drag-region></div>
      {#if ws.graph}
        <span class="text-[11px] text-faint">{ws.graph.nodes.length} Einträge · {ws.graph.edges.length} Verbindungen</span>
      {/if}
    </header>

    <div class="relative flex-1">
      <GraphView
        snapshot={ws.graph}
        selected={ws.selected}
        focusTick={ws.focusTick}
        {colorFor}
        onselect={(id) => ws.select(id)}
        onopen={(id) => ws.select(id)} />

      {#if ws.graph && ws.graph.nodes.length === 0}
        <div class="pointer-events-none absolute inset-0 flex items-center justify-center">
          <div class="pointer-events-auto flex max-w-sm flex-col items-center gap-3 text-center">
            <Sparkles class="text-accent" size={28} />
            <h2 class="text-[15px] font-semibold">{ws.currentProject ? `„${ws.currentProject.name}“ ist noch leer` : 'Dein Graph ist noch leer'}</h2>
            <p class="text-muted">Lege mit <kbd class="chip">⌘N</kbd> einen Eintrag an oder suche mit <kbd class="chip">⌘K</kbd>. Mit <code>[[Titel]]</code> im Text verbindest du Einträge.</p>
            <button class="btn-primary mt-1" onclick={() => ws.createNode('Neuer Eintrag')}>Ersten Eintrag anlegen</button>
          </div>
        </div>
      {/if}
    </div>
  </main>

  <div class="min-w-0 overflow-hidden">
    {#if ws.detail}
      {#key ws.detail.node.id}
        <Inspector />
      {/key}
    {/if}
  </div>
</div>

{#if ws.palette}
  <CommandPalette />
{/if}

{#if ws.toast}
  <div class="fixed bottom-4 left-1/2 z-50 -translate-x-1/2 rounded-lg border border-line bg-elevated px-4 py-2 shadow-xl">{ws.toast}</div>
{/if}
