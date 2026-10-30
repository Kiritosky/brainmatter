<script lang="ts">
  import { onMount } from 'svelte';
  import { flip } from 'svelte/animate';
  import {
    Bot,
    FolderPlus,
    History,
    Link2,
    Pencil,
    Plus,
    Sparkles,
    Trash2,
    Undo2,
    X,
  } from '@lucide/svelte';
  import { api, errorMessage, isApiError, type ChangeSetInfo, type RecentChange } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import Segmented from '#lib/ui/Segmented.svelte';
  import { avatarStyle, tint } from '#lib/ui/tint.ts';
  import { pop } from '#lib/ui/motion.ts';

  let { userName }: { userName: string | null } = $props();

  type Filter = 'all' | 'ai' | 'me';
  let filter = $state<Filter>('all');
  let items = $state<RecentChange[]>([]);
  let loaded = $state(false);
  let now = $state(Date.now());

  const isAi = (c: ChangeSetInfo) => c.client !== null || c.actor_name !== userName;

  const visible = $derived(
    items.filter(({ change_set: c }) => filter === 'all' || (filter === 'ai') === isAi(c)),
  );

  const dayKey = (ms: number) => new Date(ms).toDateString();

  function dayLabel(ms: number) {
    const key = dayKey(ms);
    if (key === dayKey(now)) return 'Heute';
    if (key === dayKey(now - 86_400_000)) return 'Gestern';
    const sameYear = new Date(ms).getFullYear() === new Date(now).getFullYear();
    return new Date(ms).toLocaleDateString('de-DE', {
      weekday: 'long',
      day: 'numeric',
      month: 'long',
      year: sameYear ? undefined : 'numeric',
    });
  }

  const days = $derived.by(() => {
    const out: { key: string; label: string; entries: RecentChange[] }[] = [];
    for (const entry of visible) {
      const key = dayKey(entry.change_set.at);
      if (out.at(-1)?.key !== key) out.push({ key, label: dayLabel(entry.change_set.at), entries: [] });
      out.at(-1)!.entries.push(entry);
    }
    return out;
  });

  $effect(() => {
    void ws.revision;
    reload();
  });

  onMount(() => {
    const timer = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(timer);
  });

  async function reload() {
    try {
      items = await api.recentChanges(100);
      now = Date.now();
    } catch {
      // Locked meanwhile: the workspace is about to unmount.
    }
    loaded = true;
  }

  async function revert(c: ChangeSetInfo) {
    try {
      await api.revert(c.id);
      await reload();
    } catch (e) {
      ws.notify(
        isApiError(e) && e.code === 'conflict'
          ? 'Das wurde danach noch geändert. Nimm zuerst die neuere Änderung zurück.'
          : errorMessage(e),
      );
    }
  }

  async function open(id: string) {
    await ws.select(id, true);
    if (!ws.detail) ws.notify('Dieser Eintrag liegt im Papierkorb.');
  }

  function ago(ms: number) {
    const s = Math.max(0, Math.round((now - ms) / 1000));
    if (s < 60) return 'gerade eben';
    if (s < 3600) return `vor ${Math.round(s / 60)} Min.`;
    if (s < 86_400) return `vor ${Math.round(s / 3600)} Std.`;
    return new Date(ms).toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' }) + ' Uhr';
  }

  const fullTime = (ms: number) =>
    new Date(ms).toLocaleString('de-DE', { dateStyle: 'medium', timeStyle: 'short' });

  /** Icon and colour of the small badge on the avatar. */
  function action(summary: string): { icon: typeof Undo2; color: string } {
    const verb = summary.replace(/„.*“/, '');
    if (verb.includes('rückgängig')) return { icon: Undo2, color: 'var(--color-warn)' };
    if (verb.includes('gelöscht')) return { icon: Trash2, color: 'var(--color-danger)' };
    if (verb.includes('Projekt')) return { icon: FolderPlus, color: 'var(--color-accent)' };
    if (verb.includes('Verbindung')) return { icon: Link2, color: 'var(--color-accent)' };
    if (verb.includes('erfasst')) return { icon: Sparkles, color: 'var(--color-ai)' };
    if (verb.includes('bearbeitet')) return { icon: Pencil, color: 'var(--color-muted)' };
    return { icon: Plus, color: 'var(--color-ok)' };
  }

  /** Splits `„Titel“ bearbeitet` so the title can be emphasised and clicked. */
  function parts(summary: string) {
    const m = /^(.*?)„(.+)“(.*)$/.exec(summary);
    return m ? { before: m[1], title: m[2], after: m[3] } : { before: summary, title: null, after: '' };
  }

  const FILTERS: { value: Filter; label: string }[] = [
    { value: 'all', label: 'Alle' },
    { value: 'ai', label: 'KI' },
    { value: 'me', label: 'Ich' },
  ];
</script>

<aside class="flex h-full w-100 flex-col border-l border-line bg-panel">
  <header class="px-4 pt-3 pb-3" data-tauri-drag-region>
    <div class="mb-3 flex items-center">
      <h2 class="flex-1 text-[15px] font-semibold tracking-tight">Aktivität</h2>
      <button class="icon-btn -mr-1" aria-label="Schließen" onclick={() => (ws.panel = null)}><X size={15} /></button>
    </div>
    <Segmented bind:value={filter} options={FILTERS} label="Filter" />
  </header>

  <div class="flex-1 overflow-y-auto px-2 pb-4">
    {#each days as day (day.key)}
      <section>
        <h3 class="sticky top-0 z-10 bg-panel/95 px-2 pt-3 pb-1.5 text-[11px] font-semibold text-faint backdrop-blur-sm">
          {day.label}
        </h3>
        {#each day.entries as { change_set: c, nodes }, i (c.id)}
          {@const a = action(c.summary)}
          {@const p = parts(c.summary)}
          {@const ai = isAi(c)}
          <article
            class="group relative flex gap-3 rounded-xl px-2 py-2 transition-colors duration-150 hover:bg-elevated/70"
            animate:flip={{ duration: 200 }}
            in:pop={{ duration: 240, y: 6 }}>
            {#if i < day.entries.length - 1}
              <span aria-hidden="true" class="absolute top-10.5 -bottom-2 left-[23px] w-px bg-line"></span>
            {/if}

            <div class="relative h-7.5 w-7.5 shrink-0 {c.reverted_by ? 'opacity-45' : ''}">
              <div
                class="flex h-full w-full items-center justify-center rounded-full text-[11px] font-semibold"
                style={avatarStyle(ai ? tint(c.actor_name) : 'var(--color-accent)')}>
                {#if ai}<Bot size={15} />{:else}{c.actor_name.slice(0, 1).toUpperCase()}{/if}
              </div>
              <span
                class="absolute -right-1 -bottom-1 flex h-4 w-4 items-center justify-center rounded-full bg-panel ring-2 ring-panel transition-colors group-hover:bg-elevated group-hover:ring-elevated"
                style="color: {a.color}">
                <a.icon size={10} strokeWidth={2.5} />
              </span>
            </div>

            <div class="min-w-0 flex-1 pt-px">
              <p class="leading-snug break-words {c.reverted_by ? 'text-faint line-through decoration-faint/60' : 'text-text'}">
                {p.before}{#if p.title}{#if nodes.length === 1 && !c.reverted_by}<button
                      class="font-medium text-text underline-offset-2 hover:text-accent hover:underline"
                      onclick={() => open(nodes[0].id)}>{p.title}</button>{:else}<span class="font-medium">{p.title}</span>{/if}{/if}{p.after}
              </p>
              <p class="mt-0.5 flex items-center gap-1.5 text-[11.5px] text-faint">
                <span style={ai ? `color: ${tint(c.actor_name)}` : ''}>{c.actor_name}</span>
                <span aria-hidden="true">·</span>
                <time title={fullTime(c.at)}>{ago(c.at)}</time>
                {#if c.reverted_by}
                  <span class="ml-0.5 rounded bg-raised px-1.5 py-px text-[10px] text-muted no-underline">zurückgenommen</span>
                {/if}
              </p>
              {#if nodes.length > 1}
                <div class="mt-1.5 flex flex-wrap gap-1">
                  {#each nodes.slice(0, 4) as n (n.id)}
                    <button class="max-w-40 truncate rounded-md bg-raised/70 px-1.5 py-0.5 text-[11px] text-muted transition-colors hover:bg-raised hover:text-text"
                      onclick={() => open(n.id)}>{n.title}</button>
                  {/each}
                  {#if nodes.length > 4}
                    <span class="px-1 py-0.5 text-[11px] text-faint">+{nodes.length - 4}</span>
                  {/if}
                </div>
              {/if}
            </div>

            {#if !c.reverted_by}
              <button
                class="absolute top-2 right-2 inline-flex h-7 items-center gap-1 rounded-md border border-line bg-raised px-2 text-[11px] text-muted opacity-0 shadow-sm transition-[opacity,color,scale] duration-150 group-hover:opacity-100 hover:text-text focus-visible:opacity-100 active:scale-95"
                onclick={() => revert(c)}>
                <Undo2 size={12} /> Rückgängig
              </button>
            {/if}
          </article>
        {/each}
      </section>
    {:else}
      {#if loaded}
        <div class="flex flex-col items-center px-8 pt-20 text-center" in:pop>
          <div class="mb-3 flex h-11 w-11 items-center justify-center rounded-full bg-elevated text-faint">
            {#if filter === 'ai'}<Bot size={20} />{:else}<History size={20} />{/if}
          </div>
          <p class="font-medium">{filter === 'ai' ? 'Noch keine KI-Aktivität' : 'Noch keine Aktivität'}</p>
          <p class="mt-1 text-xs leading-relaxed text-faint">
            {filter === 'ai'
              ? 'Sobald eine KI etwas schreibt, erscheint es hier und lässt sich zurücknehmen.'
              : 'Änderungen an Einträgen, Verbindungen und Projekten erscheinen hier.'}
          </p>
        </div>
      {/if}
    {/each}
  </div>
</aside>
