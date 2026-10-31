<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { ChevronRight, Clock, Sparkles } from '@lucide/svelte';
  import { api, createdAt, type Status } from '#lib/api.ts';
  import { ws, type ColorMode } from '#lib/workspace.svelte.ts';
  import Segmented from '#lib/ui/Segmented.svelte';
  import Sidebar from './Sidebar.svelte';
  import GraphView from './GraphView.svelte';
  import Inspector from './Inspector.svelte';
  import CommandPalette from './CommandPalette.svelte';
  import ActivityPanel from './ActivityPanel.svelte';
  import ReviewPanel from './ReviewPanel.svelte';
  import SettingsDialog from './SettingsDialog.svelte';
  import TimeSlider from './TimeSlider.svelte';
  import { pop, slideIn } from '#lib/ui/motion.ts';

  let { status }: { status: Status } = $props();

  const COLOR_MODES: { value: ColorMode; label: string }[] = [
    { value: 'project', label: 'Projekt' },
    { value: 'kind', label: 'Art' },
    { value: 'theme', label: 'Thema' },
  ];

  const suggested = $derived(ws.suggestions.map((s) => s.node.id));

  /** The time slider's position; null while it is closed. */
  let until = $state<number | null>(null);

  function openTimeSlider() {
    const nodes = ws.graph?.nodes ?? [];
    until = Math.max(...nodes.map((n) => createdAt(n.id)));
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
    } else if (mod && e.key === ',') {
      e.preventDefault();
      ws.settingsOpen = true;
    } else if (mod && e.key === 'n') {
      e.preventDefault();
      ws.createNode('Neuer Eintrag');
    } else if (mod && e.key === 'l' && !e.shiftKey && ws.selected) {
      e.preventDefault();
      ws.palette = 'link';
    } else if (mod && e.shiftKey && e.key.toLowerCase() === 'l') {
      e.preventDefault();
      api.lock();
    } else if (e.key === 'Escape' && !e.defaultPrevented && ws.settingsOpen) {
      ws.settingsOpen = false;
    } else if (e.key === 'Escape' && !ws.palette) {
      const tag = (e.target as HTMLElement).tagName;
      if (tag !== 'INPUT' && tag !== 'TEXTAREA' && !(e.target as HTMLElement).isContentEditable) ws.select(null);
    }
  }

  onMount(() => ws.start());
  onDestroy(() => ws.stop());
</script>

<svelte:window {onkeydown} onmousemove={activity} />

<div class="workspace grid h-full" style="grid-template-columns: 232px minmax(0, 1fr) {ws.detail || ws.panel ? '400px' : '0px'}">
  <Sidebar userName={status.user_name} />

  <main class="canvas-dots relative flex min-w-0 flex-col border-l border-line-soft">
    <header class="relative z-10 flex h-11 shrink-0 items-center gap-1 px-4 text-muted" data-tauri-drag-region>
      <button class="rounded px-1 transition-colors hover:text-text {ws.view === null ? 'text-text' : ''}" onclick={() => ws.setView(null)}>Alles</button>
      {#each ws.breadcrumb as p (p.id)}
        <ChevronRight size={13} class="text-faint" />
        <button class="rounded px-1 transition-colors hover:text-text {p.id === ws.view ? 'font-medium text-text' : ''}" onclick={() => ws.setView(p.id)}>{p.name}</button>
      {/each}
      <div class="h-full flex-1" data-tauri-drag-region></div>
      {#if ws.graph}
        <span class="text-[11px] text-faint tabular-nums">{ws.graph.nodes.length} Einträge · {ws.graph.edges.length} Verbindungen</span>
      {/if}
    </header>

    <div class="relative flex-1">
      <GraphView
        snapshot={ws.graph}
        selected={ws.selected}
        focusTick={ws.focusTick}
        {suggested}
        {until}
        colorFor={(n) => ws.colorFor(n)}
        onselect={(id) => ws.select(id)}
        onopen={(id) => ws.select(id)} />

      {#if ws.graph && ws.graph.nodes.length > 0}
        <div class="absolute bottom-4 left-4 z-10 flex items-center gap-2">
          <div class="w-56">
            <Segmented value={ws.colorMode} options={COLOR_MODES} label="Einfärben nach" onchange={(m) => ws.setColorMode(m)} />
          </div>
          {#if until === null}
            <button class="glass-pill !h-8 gap-1.5 !px-2.5 text-xs text-muted transition-colors hover:text-text" title="Das Wachstum des Graphen ansehen" onclick={openTimeSlider}>
              <Clock size={13} /> Zeitregler
            </button>
          {/if}
        </div>
        {#if until !== null}
          <div class="absolute right-34 bottom-4 left-64 z-10">
            <TimeSlider nodes={ws.graph.nodes} bind:until onclose={() => (until = null)} />
          </div>
        {/if}
      {/if}

      {#if ws.graph && ws.graph.nodes.length === 0}
        <div class="pointer-events-none absolute inset-0 flex items-center justify-center">
          <div class="pointer-events-auto flex max-w-sm flex-col items-center text-center" in:pop={{ duration: 320, y: 10 }}>
            <div class="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-accent/12 text-accent"><Sparkles size={26} /></div>
            <h2 class="text-base font-semibold tracking-tight">{ws.currentProject ? `„${ws.currentProject.name}“ ist noch leer` : 'Dein Graph ist noch leer'}</h2>
            <p class="mt-2 leading-relaxed text-muted">
              Lege mit <kbd class="kbd">⌘N</kbd> einen Eintrag an oder suche mit <kbd class="kbd">⌘K</kbd>.
              Mit <code class="rounded bg-raised px-1 font-mono text-xs">[[Titel]]</code> im Text verbindest du Einträge.
            </p>
            <button class="btn-primary mt-5" onclick={() => ws.createNode('Neuer Eintrag')}>Ersten Eintrag anlegen</button>
          </div>
        </div>
      {/if}
    </div>
  </main>

  <div class="min-w-0 overflow-hidden">
    <!-- Fixed width inside the animated column, so content never reflows. -->
    {#if ws.detail}
      <div class="h-full w-100" in:slideIn>
        {#key ws.detail.node.id}
          <Inspector />
        {/key}
      </div>
    {:else if ws.panel === 'activity'}
      <div class="h-full w-100" in:slideIn>
        <ActivityPanel userName={status.user_name} />
      </div>
    {:else if ws.panel === 'review'}
      <div class="h-full w-100" in:slideIn>
        <ReviewPanel />
      </div>
    {/if}
  </div>
</div>

{#if ws.palette}
  <CommandPalette />
{/if}

{#if ws.settingsOpen}
  <SettingsDialog />
{/if}

{#if ws.toast}
  <div class="pointer-events-none fixed inset-x-0 bottom-5 z-[60] flex justify-center">
    <div class="surface pointer-events-auto rounded-xl px-4 py-2.5" transition:pop={{ duration: 200, y: 12 }}>{ws.toast}</div>
  </div>
{/if}

<style>
  .workspace {
    transition: grid-template-columns 240ms var(--ease-out-soft);
  }
</style>
