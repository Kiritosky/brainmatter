<script lang="ts">
  import { Activity, BookMarked, CornerDownLeft, Link2, ListChecks, ListFilter, Lock, Monitor, Moon, Plus, Search, Settings, Sparkles, Sun, Trash2 } from '@lucide/svelte';
  import { releases } from '#lib/changelog.ts';
  import { theme } from '#lib/theme.svelte.ts';
  import { api, errorMessage, type SearchHit } from '#lib/api.ts';
  import { KIND_COLORS, ws } from '#lib/workspace.svelte.ts';
  import Select from '#lib/ui/Select.svelte';
  import { fadeQuick, pop } from '#lib/ui/motion.ts';

  const mode = $derived(ws.palette!);
  let text = $state('');
  let relation = $state('related_to');
  let hits = $state<SearchHit[]>([]);
  let active = $state(0);
  let seq = 0;

  type Command = { label: string; icon: typeof Search; hint?: string; run: () => unknown };

  const panel = (name: 'activity' | 'review' | 'library' | 'trash') => () => {
    ws.select(null);
    ws.panel = name;
  };
  const setTheme = (mode: 'system' | 'light' | 'dark') => async () => {
    theme.apply(mode);
    const settings = await api.getSettings();
    await api.updateSettings({ ...settings, theme: mode });
  };

  const COMMANDS: Command[] = [
    { label: 'Neuer Eintrag', icon: Plus, hint: '⌘N', run: () => ws.createNode('Neuer Eintrag') },
    { label: 'Aktivität zeigen', icon: Activity, run: panel('activity') },
    { label: 'Prüfen', icon: ListChecks, run: panel('review') },
    { label: 'Bibliothek öffnen', icon: BookMarked, run: panel('library') },
    { label: 'Papierkorb öffnen', icon: Trash2, run: panel('trash') },
    { label: 'Einstellungen', icon: Settings, hint: '⌘,', run: () => (ws.settingsOpen = true) },
    { label: 'Was ist neu', icon: Sparkles, run: () => (ws.changelog = { heading: 'Alle Änderungen', items: releases }) },
    { label: 'Filter zurücksetzen', icon: ListFilter, run: () => ws.clearFilters() },
    { label: 'Helles Design', icon: Sun, run: setTheme('light') },
    { label: 'Dunkles Design', icon: Moon, run: setTheme('dark') },
    { label: 'Design wie macOS', icon: Monitor, run: setTheme('system') },
    { label: 'Sperren', icon: Lock, hint: '⌘⇧L', run: () => api.lock() },
  ];

  const matches = (label: string) => label.toLowerCase().includes(text.trim().toLowerCase());

  // Projects and commands only when opening things, and only once there is
  // something to match: an empty field shows the latest entries.
  const projects = $derived(mode === 'open' && text.trim() ? ws.projects.filter((p) => matches(p.project.name)).slice(0, 5) : []);
  const commands = $derived(mode === 'open' && text.trim() ? COMMANDS.filter((c) => matches(c.label)).slice(0, 5) : []);

  // Entries first, since finding them is what the palette is for, then projects and
  // commands, and "create" as the last option. Only a project named exactly what was
  // typed goes ahead of them, unless an entry has that exact title too.
  const options = $derived.by(() => {
    const typed = text.trim().toLowerCase();
    const exactEntry = hits.some((h) => h.node.title.toLowerCase() === typed);
    const isFirst = (name: string) => !exactEntry && name.toLowerCase() === typed;
    const project = (p: (typeof projects)[number]) => ({ kind: 'project' as const, project: p.project });
    return [
      ...projects.filter((p) => isFirst(p.project.name)).map(project),
      ...hits.filter((h) => mode !== 'link' || h.node.id !== ws.selected).map((h) => ({ kind: 'hit' as const, hit: h })),
      ...projects.filter((p) => !isFirst(p.project.name)).map(project),
      ...commands.map((command) => ({ kind: 'command' as const, command })),
      ...(text.trim() ? [{ kind: 'create' as const }] : []),
    ];
  });

  $effect(() => {
    const q = text;
    const mine = ++seq;
    const timer = setTimeout(async () => {
      const res = await api.search(q, { limit: 12 }).catch(() => []);
      if (mine === seq) {
        hits = res;
        active = 0;
      }
    }, 60);
    return () => clearTimeout(timer);
  });

  const RELATIONS = [
    { value: 'related_to', label: 'verwandt mit' },
    { value: 'part_of', label: 'Teil von' },
    { value: 'depends_on', label: 'hängt ab von' },
    { value: 'example_of', label: 'Beispiel für' },
    { value: 'supports', label: 'stützt' },
    { value: 'contradicts', label: 'widerspricht' },
    { value: 'derived_from', label: 'abgeleitet von' },
  ];

  let list: HTMLDivElement | undefined = $state();
  $effect(() => {
    list?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: 'nearest' });
  });

  function close() {
    ws.palette = null;
  }

  async function choose(i: number) {
    const opt = options[i];
    if (!opt) return;
    // Read everything first: `mode` derives from ws.palette, which close() clears.
    const current = mode;
    const title = text.trim();
    close();
    try {
      if (opt.kind === 'project') await ws.setView(opt.project.id);
      else if (opt.kind === 'command') await opt.command.run();
      else if (current === 'open') {
        if (opt.kind === 'hit') await ws.select(opt.hit.node.id, true);
        else await ws.createNode(title);
      } else if (ws.selected) {
        const src = ws.selected;
        const dst = opt.kind === 'hit' ? opt.hit.node.id : (await api.createNode({ title })).id;
        await api.link(src, dst, relation);
      }
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  function onkeydown(e: KeyboardEvent) {
    // An open relation picker handles its own keys first.
    if (e.defaultPrevented) return;
    if (e.key === 'Escape') close();
    else if (e.key === 'ArrowDown') (active = Math.min(options.length - 1, active + 1)), e.preventDefault();
    else if (e.key === 'ArrowUp') (active = Math.max(0, active - 1)), e.preventDefault();
    else if (e.key === 'Enter') choose(e.metaKey ? options.length - 1 : active), e.preventDefault();
  }

  // Snippets mark matches with **…**; render them as <mark> safely.
  const parts = (s: string) =>
    s.replace(/\[\[|\]\]/g, '').split('**').map((p, i) => ({ text: p, mark: i % 2 === 1 }));
</script>

<div class="fixed inset-0 z-50 flex justify-center bg-black/45 pt-[13vh] backdrop-blur-[2px]" role="presentation" transition:fadeQuick onclick={close}>
  <div class="surface h-fit w-[640px] rounded-2xl" role="dialog" aria-label="Suche" tabindex="-1"
    transition:pop onclick={(e) => e.stopPropagation()} {onkeydown}>
    <div class="flex items-center gap-3 border-b border-line-soft px-4">
      {#if mode === 'link'}<Link2 size={17} class="shrink-0 text-accent" />{:else}<Search size={17} class="shrink-0 text-faint" />{/if}
      <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
      <input
        class="h-13 min-w-0 flex-1 bg-transparent text-[15px] outline-none placeholder:text-faint"
        placeholder={mode === 'link' ? 'Verbinden mit …' : 'Suchen oder neuen Eintrag anlegen …'}
        bind:value={text}
        autofocus />
      {#if mode === 'link'}
        <Select bind:value={relation} options={RELATIONS} label="Art der Beziehung" />
      {/if}
    </div>
    <div class="max-h-[48vh] overflow-y-auto p-1.5" bind:this={list}>
      {#each options as opt, i}
        <button
          data-index={i}
          class="flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left transition-colors duration-100 {i === active ? 'bg-accent/12' : ''}"
          onmouseenter={() => (active = i)}
          onclick={() => choose(i)}>
          {#if opt.kind === 'project'}
            <span class="h-2 w-2 shrink-0 rounded-full" style="background: {ws.projectColor.get(opt.project.id)}"></span>
            <span class="min-w-0 flex-1 truncate font-medium">{opt.project.name}</span>
            <span class="shrink-0 text-[11px] text-faint">Projekt</span>
          {:else if opt.kind === 'command'}
            <span class="flex h-5 w-5 shrink-0 items-center justify-center text-faint"><opt.command.icon size={14} /></span>
            <span class="min-w-0 flex-1 truncate">{opt.command.label}</span>
            {#if opt.command.hint}<span class="kbd shrink-0">{opt.command.hint}</span>{:else}<span class="shrink-0 text-[11px] text-faint">Aktion</span>{/if}
          {:else if opt.kind === 'hit'}
            <span class="h-2 w-2 shrink-0 rounded-full" style="background: {KIND_COLORS[opt.hit.node.kind] ?? KIND_COLORS.note}"></span>
            <div class="min-w-0 flex-1">
              <div class="flex items-baseline gap-2">
                <span class="truncate font-medium">{opt.hit.node.title}</span>
                <span class="shrink-0 text-[11px] text-faint">{opt.hit.node.kind}</span>
                {#if opt.hit.semantic}
                  <span class="flex shrink-0 items-center gap-1 self-center text-[11px] text-accent" title="Passt vom Sinn her"><Sparkles size={10} /> ähnlich</span>
                {/if}
              </div>
              {#if opt.hit.snippet}
                <div class="mt-0.5 truncate text-xs text-muted">
                  {#each parts(opt.hit.snippet) as p}{#if p.mark}<mark class="bg-transparent font-medium text-accent">{p.text}</mark>{:else}{p.text}{/if}{/each}
                </div>
              {/if}
            </div>
            {#if i === active}<CornerDownLeft size={13} class="shrink-0 text-faint" />{/if}
          {:else}
            <span class="flex h-5 w-5 shrink-0 items-center justify-center rounded-md bg-accent/15 text-accent"><Plus size={13} /></span>
            <div class="min-w-0 flex-1 truncate">„<span class="font-medium">{text.trim()}</span>“ {mode === 'link' ? 'anlegen und verbinden' : 'neu anlegen'}</div>
            <span class="kbd shrink-0">⌘↵</span>
          {/if}
        </button>
      {:else}
        <div class="px-3 py-10 text-center text-faint">Noch keine Einträge. Tippe einen Titel, um den ersten anzulegen.</div>
      {/each}
    </div>
    <footer class="flex items-center gap-4 border-t border-line-soft px-4 py-2 text-[11px] text-faint">
      <span class="flex items-center gap-1.5"><span class="kbd">↑</span><span class="kbd">↓</span> Auswählen</span>
      <span class="flex items-center gap-1.5"><span class="kbd">↵</span> {mode === 'link' ? 'Verbinden' : 'Öffnen'}</span>
      <span class="ml-auto flex items-center gap-1.5"><span class="kbd">esc</span> Schließen</span>
    </footer>
  </div>
</div>
