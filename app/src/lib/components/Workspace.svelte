<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { ChevronRight, Clock, Network } from '@lucide/svelte';
  import { api, createdAt, errorMessage, type Id, type Status } from '#lib/api.ts';
  import { ws, type ColorMode } from '#lib/workspace.svelte.ts';
  import { prefs } from '#lib/prefs.svelte.ts';
  import Segmented from '#lib/ui/Segmented.svelte';
  import Sidebar from './Sidebar.svelte';
  import GraphView from './GraphView.svelte';
  import Inspector from './Inspector.svelte';
  import TemplateWizard from './TemplateWizard.svelte';
  import Tour from './Tour.svelte';
  import { TOUR_SEEN_KEY } from '#lib/help.ts';
  import CommandPalette from './CommandPalette.svelte';
  import ActivityPanel from './ActivityPanel.svelte';
  import DocsView from './DocsView.svelte';
  import GraphFilter from './GraphFilter.svelte';
  import LibraryPanel from './LibraryPanel.svelte';
  import ReviewPanel from './ReviewPanel.svelte';
  import SettingsDialog from './SettingsDialog.svelte';
  import TimeSlider from './TimeSlider.svelte';
  import TrashPanel from './TrashPanel.svelte';
  import WhatsNew from './WhatsNew.svelte';
  import { pop, slideIn } from '#lib/ui/motion.ts';

  let { status }: { status: Status } = $props();

  const MAIN_VIEWS: { value: 'graph' | 'docs'; label: string }[] = [
    { value: 'graph', label: 'Graph' },
    { value: 'docs', label: 'Doku' },
  ];

  const COLOR_MODES: { value: ColorMode; label: string }[] = [
    { value: 'project', label: 'Projekt' },
    { value: 'kind', label: 'Art' },
    { value: 'theme', label: 'Thema' },
  ];

  const suggested = $derived(ws.suggestions.map((s) => s.node.id));
  const NOTHING_PULSING = new Set<Id>();
  const graphProjects = $derived(
    ws.projects.map(({ project }) => ({
      id: project.id,
      parent: project.parent_id,
      name: project.name,
      color: ws.projectColor.get(project.id) ?? 'var(--color-faint)',
    })),
  );

  /** The time slider's position; null while it is closed. */
  let until = $state<number | null>(null);

  function openTimeSlider() {
    const nodes = ws.graph?.nodes ?? [];
    until = Math.max(...nodes.map((n) => createdAt(n.id)));
  }

  // With the inspector open in a small window, the graph may be under 300 px wide.
  let mainWidth = $state(Infinity);
  const narrow = $derived(mainWidth < 620);
  const tight = $derived(mainWidth < 400);

  async function linkEntries(src: Id, dst: Id) {
    try {
      await api.link(src, dst);
      const title = (id: Id) => ws.graph?.nodes.find((n) => n.id === id)?.title ?? '';
      ws.notify(`„${title(src)}“ mit „${title(dst)}“ verbunden`);
    } catch (e) {
      ws.notify(errorMessage(e));
    }
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
      if (tag === 'INPUT' || tag === 'TEXTAREA' || (e.target as HTMLElement).isContentEditable) return;
      // One step back at a time: the selection first, then up the projects.
      if (ws.selected) ws.select(null);
      else if (ws.view && !ws.panel && !ws.changelog && ws.main === 'graph') ws.setView(ws.breadcrumb.at(-2)?.id ?? null);
    }
  }

  onMount(() => ws.start());

  // Once, when the workspace has something to show and "what's new" is out of the way.
  let tourOffered = false;
  $effect(() => {
    if (tourOffered || !ws.graph || ws.changelog) return;
    tourOffered = true;
    if (!localStorage.getItem(TOUR_SEEN_KEY)) ws.tourOpen = true;
  });
  onDestroy(() => ws.stop());
</script>

<svelte:window {onkeydown} onmousemove={activity} />

<!-- The row is as high as the window whatever its content: what does not fit scrolls inside. -->
<div class="workspace grid h-full" style="grid-template-rows: minmax(0, 1fr); grid-template-columns: 232px minmax(0, 1fr) {ws.detail || ws.panel ? '400px' : '0px'}">
  <Sidebar userName={status.user_name} />

  <main class="canvas relative flex min-h-0 min-w-0 flex-col border-l border-line-soft" bind:clientWidth={mainWidth}>
    <header class="relative z-10 flex h-11 shrink-0 items-center gap-1 px-4 text-muted" data-tauri-drag-region>
      <button class="shrink-0 rounded px-1 transition-colors hover:text-text {ws.view === null ? 'text-text' : ''}" onclick={() => ws.setView(null)}>Alles</button>
      {#each ws.breadcrumb as p (p.id)}
        <ChevronRight size={13} class="shrink-0 text-faint" />
        <button class="min-w-0 truncate rounded px-1 transition-colors hover:text-text {p.id === ws.view ? 'font-medium text-text' : ''}" onclick={() => ws.setView(p.id)}>{p.name}</button>
      {/each}
      <div class="h-full min-w-2 flex-1" data-tauri-drag-region></div>
      {#if ws.graph && ws.main === 'graph' && !narrow}
        {@const edges = ws.graph.edges.filter((e) => !ws.hiddenNodes.has(e.src) && !ws.hiddenNodes.has(e.dst)).length}
        <span class="shrink-0 text-[11px] text-faint tabular-nums">{ws.hiddenNodes.size ? `${ws.graph.nodes.length - ws.hiddenNodes.size} von ${ws.graph.nodes.length}` : ws.graph.nodes.length} Einträge · {edges} Verbindungen</span>
      {/if}
      {#if ws.view}
        <div class="ml-3 w-36 shrink-0"><Segmented bind:value={ws.main} options={MAIN_VIEWS} label="Ansicht" /></div>
      {/if}
    </header>

    <div class="relative min-h-0 flex-1" data-tour="graph">
      <GraphView
        snapshot={ws.graph}
        selected={ws.selected}
        focusTick={ws.focusTick}
        {suggested}
        {until}
        hidden={ws.hiddenNodes}
        pulsing={prefs.values.aiActivity ? ws.pulsing : NOTHING_PULSING}
        projects={graphProjects}
        onopenproject={(id) => ws.setView(id)}
        onlink={linkEntries}
        colorFor={(n) => ws.colorFor(n)}
        onselect={(id) => ws.select(id)}
        onopen={(id) => ws.select(id)} />

      {#if ws.aiActivity && prefs.values.aiActivity && ws.main === 'graph'}
        <div class="glass-pill pointer-events-none absolute top-3 left-1/2 z-10 !h-8 max-w-[70%] -translate-x-1/2 gap-2 !px-3 text-xs" role="status" transition:pop={{ duration: 180, y: -6 }}>
          <span class="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-ai"></span>
          <span class="shrink-0 font-medium text-ai">{ws.aiActivityName}</span>
          <span class="truncate text-muted">{ws.aiActivity.summary}</span>
        </div>
      {/if}

      {#if ws.graph && ws.graph.nodes.length > 0}
        <div class="absolute bottom-4 left-4 z-10 flex items-center gap-2" data-tour="tools">
          {#if !narrow}
            <div class="w-56">
              <Segmented value={ws.colorMode} options={COLOR_MODES} label="Einfärben nach" onchange={(m) => ws.setColorMode(m)} />
            </div>
          {/if}
          <GraphFilter />
          {#if until === null}
            <button class="glass-pill !h-8 gap-1.5 !px-2.5 text-xs text-muted transition-colors hover:text-text" aria-label="Zeitregler" title="Das Wachstum des Graphen ansehen" onclick={openTimeSlider}>
              <Clock size={13} />{#if !tight}<span>Zeitregler</span>{/if}
            </button>
          {/if}
        </div>
        {#if until !== null}
          <div class="absolute z-10 {narrow ? 'right-4 bottom-15 left-4' : 'right-34 bottom-4 left-[352px]'}">
            <TimeSlider nodes={ws.graph.nodes} bind:until onclose={() => (until = null)} />
          </div>
        {/if}
      {/if}

      <!-- On top of the graph, which stays alive so its camera and layout survive. -->
      {#if ws.main === 'docs' && ws.view}
        {#key ws.view}
          <DocsView project={ws.view} />
        {/key}
      {/if}

      {#if ws.graph && ws.graph.nodes.length === 0}
        <div class="pointer-events-none absolute inset-0 flex items-center justify-center">
          <div class="pointer-events-auto flex max-w-sm flex-col items-center text-center" in:pop={{ duration: 320, y: 10 }}>
            <Network size={26} class="mb-4 text-faint" />
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
    {:else if ws.panel === 'library'}
      <div class="h-full w-100" in:slideIn>
        <LibraryPanel />
      </div>
    {:else if ws.panel === 'trash'}
      <div class="h-full w-100" in:slideIn>
        <TrashPanel />
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

{#if ws.templateWizard}
  <TemplateWizard />
{/if}

{#if ws.tourOpen}
  <Tour />
{/if}

{#if ws.changelog}
  <WhatsNew heading={ws.changelog.heading} items={ws.changelog.items} onclose={() => (ws.changelog = null)} />
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
