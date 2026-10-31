<script lang="ts">
  import { flip } from 'svelte/animate';
  import { Check, CheckCheck, Link2Off, ShieldCheck, X } from '@lucide/svelte';
  import { api, errorMessage, type NodeBrief } from '#lib/api.ts';
  import { KIND_COLORS, ws } from '#lib/workspace.svelte.ts';
  import Segmented from '#lib/ui/Segmented.svelte';
  import { pop } from '#lib/ui/motion.ts';

  type Tab = 'unreviewed' | 'orphans';
  let tab = $state<Tab>('unreviewed');
  let orphans = $state.raw<NodeBrief[]>([]);

  const TABS: { value: Tab; label: string }[] = [
    { value: 'unreviewed', label: 'Ungeprüft' },
    { value: 'orphans', label: 'Unverbunden' },
  ];

  $effect(() => {
    void ws.revision;
    api.orphans().then((found) => (orphans = found)).catch(() => {});
  });

  const items = $derived(tab === 'unreviewed' ? ws.unreviewed : orphans);

  async function markReviewed(ids: string[]) {
    try {
      await Promise.all(ids.map((id) => api.updateNode(id, { reviewed: true })));
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }
</script>

<aside class="flex h-full w-100 flex-col border-l border-line bg-panel">
  <header class="px-4 pt-3 pb-3" data-tauri-drag-region>
    <div class="mb-3 flex items-center">
      <h2 class="flex-1 text-[15px] font-semibold tracking-tight">Prüfen</h2>
      <button class="icon-btn -mr-1" aria-label="Schließen" onclick={() => (ws.panel = null)}><X size={15} /></button>
    </div>
    <Segmented bind:value={tab} options={TABS} label="Liste" />
  </header>

  <div class="flex-1 overflow-y-auto px-2 pb-3">
    {#if tab === 'unreviewed' && items.length > 1}
      <div class="flex justify-end px-1 pb-1">
        <button class="btn-ghost !h-7 text-xs" onclick={() => markReviewed(items.map((n) => n.id))}>
          <CheckCheck size={13} /> Alle als geprüft markieren
        </button>
      </div>
    {/if}

    {#each items as n (n.id)}
      <div class="group flex h-9 items-center gap-2.5 rounded-lg px-2 transition-colors hover:bg-elevated" animate:flip={{ duration: 180 }}>
        <span class="h-2 w-2 shrink-0 rounded-full" style="background: {KIND_COLORS[n.kind] ?? KIND_COLORS.note}"></span>
        <button class="min-w-0 flex-1 truncate text-left" onclick={() => ws.select(n.id, true)}>{n.title}</button>
        {#if tab === 'unreviewed'}
          <button class="icon-btn !h-6.5 !w-6.5 opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:!text-ok"
            aria-label="Als geprüft markieren" title="Als geprüft markieren" onclick={() => markReviewed([n.id])}><Check size={14} /></button>
        {:else}
          <span class="shrink-0 text-[11px] text-faint">{n.kind}</span>
        {/if}
      </div>
    {:else}
      <div class="flex flex-col items-center px-8 pt-16 text-center" in:pop={{ duration: 240, y: 8 }}>
        <div class="mb-3 flex h-11 w-11 items-center justify-center rounded-xl bg-ok/12 text-ok">
          {#if tab === 'unreviewed'}<ShieldCheck size={20} />{:else}<Link2Off size={20} />{/if}
        </div>
        <div class="font-medium">{tab === 'unreviewed' ? 'Alles geprüft' : 'Alles verbunden'}</div>
        <p class="mt-1 text-xs leading-relaxed text-muted">
          {tab === 'unreviewed' ? 'Hier landen Einträge, die eine KI geschrieben hat, bis du sie gesehen hast.' : 'Hier landen Einträge ohne jede Verbindung.'}
        </p>
      </div>
    {/each}
  </div>
</aside>
