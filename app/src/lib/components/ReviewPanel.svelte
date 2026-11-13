<script lang="ts">
  import { day, plural, t } from '#lib/i18n.ts';
  import { flip } from 'svelte/animate';
  import { Check, CheckCheck, FileExclamationPoint, Hourglass, Link2Off, Plus, ShieldCheck, Unlink, X } from '@lucide/svelte';
  import { api, errorMessage, type MissingLink, type NodeBrief, type OutdatedEntry, type Unread } from '#lib/api.ts';
  import { KIND_COLORS, ws } from '#lib/workspace.svelte.ts';
  import Segmented from '#lib/ui/Segmented.svelte';
  import { pop } from '#lib/ui/motion.ts';

  type Tab = 'unreviewed' | 'orphans' | 'outdated' | 'unread' | 'missing';
  let tab = $state<Tab>('unreviewed');
  let orphans = $state.raw<NodeBrief[]>([]);
  let outdated = $state.raw<OutdatedEntry[]>([]);
  let unread = $state.raw<Unread>({ since: 0, entries: [] });
  let missing = $state.raw<MissingLink[]>([]);

  const TABS: { value: Tab; label: string }[] = [
    { value: 'unreviewed', label: t('Ungeprüft', 'Unreviewed') },
    { value: 'orphans', label: t('Unverbunden', 'Unlinked') },
    { value: 'outdated', label: t('Veraltet', 'Outdated') },
    { value: 'unread', label: t('Selten', 'Unread') },
    { value: 'missing', label: t('Fehlend', 'Missing') },
  ];

  /** Reads are no change, so the lists load when their tab opens and after changes. */
  async function load(which: Tab) {
    try {
      if (which === 'orphans') orphans = await api.orphans();
      else if (which === 'outdated') outdated = await api.outdated();
      else if (which === 'unread') unread = await api.unread();
      else if (which === 'missing') missing = await api.missingLinks();
    } catch {
      // Locked meanwhile; the lists come back with the next change.
    }
  }

  $effect(() => {
    void ws.revision;
    void ws.upkeep;
    void load(tab);
  });

  const hasFolder = $derived(ws.projects.some((p) => p.folder));
  const counting = $derived(Date.now() - unread.since < 90 * 86_400_000);

  async function markReviewed(ids: string[]) {
    try {
      await Promise.all(ids.map((id) => api.updateNode(id, { reviewed: true })));
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  async function confirmCurrent(id: string) {
    try {
      await api.confirmCurrent(id);
      outdated = outdated.filter((e) => e.node.id !== id);
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  /** Creates the entry in the projects of the first one linking to it. */
  async function create(link: MissingLink) {
    const first = link.sources[0]?.id;
    const projects = ws.graph?.nodes.find((n) => n.id === first)?.projects ?? (ws.view ? [ws.view] : []);
    try {
      const node = await api.createNode({ title: link.title, body: '', projects });
      missing = missing.filter((m) => m !== link);
      await ws.select(node.id, true);
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  const fileName = (path: string) => path.replace(/\/$/, '').split('/').pop() ?? path;

  const EMPTY: Record<Tab, { title: () => string; text: () => string }> = {
    unreviewed: {
      title: () => t('Alles geprüft', 'All reviewed'),
      text: () => t('Hier landen Einträge, die eine KI geschrieben hat, bis du sie gesehen hast.', 'Entries an AI client wrote land here until you have seen them.'),
    },
    orphans: {
      title: () => t('Alles verbunden', 'All linked'),
      text: () => t('Hier landen Einträge ohne jede Verbindung.', 'Entries without any link land here.'),
    },
    outdated: {
      title: () => t('Alles aktuell', 'All up to date'),
      text: () =>
        hasFolder
          ? t('Hier landen Einträge, deren Dateien fehlen oder sich seitdem geändert haben.', 'Entries whose files are gone or have changed since land here.')
          : t('Gib einem Projekt im Projekt-Menü seinen Ordner, dann prüft Brainmatter die Pfade in seinen Einträgen.', 'Give a project its folder in the project menu, and Brainmatter checks the paths in its entries.'),
    },
    unread: {
      title: () => (counting ? t('Noch zu früh', 'Too early yet') : t('Alles in Gebrauch', 'All in use')),
      text: () =>
        counting
          ? t(`Gezählt wird seit ${day(unread.since)}. Nach 90 Tagen erscheinen hier Einträge, die seitdem niemand gelesen oder geändert hat.`, `Reads are counted since ${day(unread.since)}. After 90 days, entries nobody has read or changed since appear here.`)
          : t('Hier landen Einträge, die 90 Tage lang niemand gelesen oder geändert hat.', 'Entries nobody has read or changed for 90 days land here.'),
    },
    missing: {
      title: () => t('Nichts fehlt', 'Nothing missing'),
      text: () => t('Hier landen [[Links]] auf Einträge, die es noch nicht gibt.', 'Links like [[this]] to entries that do not exist yet land here.'),
    },
  };
  const EMPTY_ICON = { unreviewed: ShieldCheck, orphans: Link2Off, outdated: FileExclamationPoint, unread: Hourglass, missing: Unlink };
  const count = $derived(
    { unreviewed: ws.unreviewed.length, orphans: orphans.length, outdated: outdated.length, unread: unread.entries.length, missing: missing.length }[tab],
  );
</script>

<aside class="flex h-full w-100 flex-col border-l border-line bg-panel">
  <header class="px-4 pt-3 pb-3" data-tauri-drag-region>
    <div class="mb-3 flex items-center">
      <h2 class="flex-1 text-[15px] font-semibold tracking-tight">{t('Prüfen', 'Review')}</h2>
      <button class="icon-btn -mr-1" aria-label={t('Schließen', 'Close')} onclick={() => (ws.panel = null)}><X size={15} /></button>
    </div>
    <Segmented bind:value={tab} options={TABS} label={t('Liste', 'List')} compact />
  </header>

  <div class="flex-1 overflow-y-auto px-2 pb-3">
    {#if tab === 'unreviewed' && ws.unreviewed.length > 1}
      <div class="flex justify-end px-1 pb-1">
        <button class="btn-ghost !h-7 text-xs" onclick={() => markReviewed(ws.unreviewed.map((n) => n.id))}>
          <CheckCheck size={13} /> {t('Alle als geprüft markieren', 'Mark all as reviewed')}
        </button>
      </div>
    {/if}

    {#if tab === 'unreviewed' || tab === 'orphans' || tab === 'unread'}
      {#each tab === 'unreviewed' ? ws.unreviewed : tab === 'orphans' ? orphans : unread.entries as n (n.id)}
        <div class="group flex h-9 items-center gap-2.5 rounded-lg px-2 transition-colors hover:bg-elevated" animate:flip={{ duration: 180 }}>
          <span class="h-2 w-2 shrink-0 rounded-full" style="background: {KIND_COLORS[n.kind] ?? KIND_COLORS.note}"></span>
          <button class="min-w-0 flex-1 truncate text-left" onclick={() => ws.select(n.id, true)}>{n.title}</button>
          {#if tab === 'unreviewed'}
            <button class="icon-btn !h-6.5 !w-6.5 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:!text-ok"
              aria-label={t('Als geprüft markieren', 'Mark as reviewed')} title={t('Als geprüft markieren', 'Mark as reviewed')} onclick={() => markReviewed([n.id])}><Check size={14} /></button>
          {:else}
            <span class="shrink-0 text-[11px] text-faint">{n.kind}</span>
          {/if}
        </div>
      {/each}
    {:else if tab === 'outdated'}
      {#each outdated as e (e.node.id)}
        <div class="group flex items-start gap-2.5 rounded-lg px-2 py-1.5 transition-colors hover:bg-elevated" animate:flip={{ duration: 180 }}>
          <span class="mt-1.5 h-2 w-2 shrink-0 rounded-full" style="background: {KIND_COLORS[e.node.kind] ?? KIND_COLORS.note}"></span>
          <button class="min-w-0 flex-1 text-left" onclick={() => ws.select(e.node.id, true)}>
            <span class="block truncate">{e.node.title}</span>
            <span class="mt-0.5 flex flex-wrap gap-x-2 text-[11px] leading-snug">
              {#each [...e.issues].sort((a, b) => Number(b.missing) - Number(a.missing)) as issue (issue.path)}
                <span class={issue.missing ? 'text-warn' : 'text-faint'} title={issue.path}>
                  <span class="font-mono">{fileName(issue.path)}</span> {issue.missing ? t('fehlt', 'gone') : t('geändert', 'changed')}
                </span>
              {/each}
            </span>
          </button>
          <button class="icon-btn !h-6.5 !w-6.5 shrink-0 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:!text-ok"
            aria-label={t('Stimmt noch', 'Still current')} title={t('Stimmt noch', 'Still current')} onclick={() => confirmCurrent(e.node.id)}><Check size={14} /></button>
        </div>
      {/each}
    {:else}
      {#each missing as m (m.title)}
        <div class="group flex h-9 items-center gap-2.5 rounded-lg px-2 transition-colors hover:bg-elevated" animate:flip={{ duration: 180 }}>
          <span class="h-2 w-2 shrink-0 rounded-full border border-dashed border-faint"></span>
          <span class="min-w-0 flex-1 truncate text-muted" title={m.sources.map((s) => s.title).join(', ')}>{m.title}</span>
          <button class="shrink-0 text-[11px] text-faint hover:text-text" onclick={() => ws.select(m.sources[0].id, true)}
            title={m.sources.map((s) => s.title).join(', ')}>{plural(m.sources.length, ['Verweis', 'Verweise'], ['link', 'links'])}</button>
          <button class="icon-btn !h-6.5 !w-6.5 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:!text-accent"
            aria-label={t('Eintrag anlegen', 'Create the entry')} title={t('Eintrag anlegen', 'Create the entry')} onclick={() => create(m)}><Plus size={14} /></button>
        </div>
      {/each}
    {/if}

    {#if count === 0}
      {@const Icon = EMPTY_ICON[tab]}
      <div class="flex flex-col items-center px-8 pt-16 text-center" in:pop={{ duration: 240, y: 8 }}>
        <div class="mb-3 flex h-11 w-11 items-center justify-center rounded-xl bg-ok/12 text-ok"><Icon size={20} /></div>
        <div class="font-medium">{EMPTY[tab].title()}</div>
        <p class="mt-1 text-xs leading-relaxed text-muted">{EMPTY[tab].text()}</p>
      </div>
    {/if}
  </div>
</aside>
