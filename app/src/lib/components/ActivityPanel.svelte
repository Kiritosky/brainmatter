<script lang="ts">
  import { Bot, Undo2, X } from '@lucide/svelte';
  import { api, errorMessage, type ChangeSetInfo } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';

  let { userName }: { userName: string | null } = $props();

  let items = $state<ChangeSetInfo[]>([]);
  let onlyAi = $state(false);

  const isAi = (c: ChangeSetInfo) => c.client !== null || c.actor_name !== userName;
  const visible = $derived(onlyAi ? items.filter(isAi) : items);

  $effect(() => {
    void ws.revision;
    api.changeSets(100).then((list) => (items = list)).catch(() => {});
  });

  async function revert(c: ChangeSetInfo) {
    try {
      await api.revert(c.id);
      items = await api.changeSets(100);
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  function ago(ms: number) {
    const s = Math.round((Date.now() - ms) / 1000);
    if (s < 60) return 'gerade eben';
    if (s < 3600) return `vor ${Math.round(s / 60)} min`;
    if (s < 86400) return `vor ${Math.round(s / 3600)} h`;
    return new Date(ms).toLocaleDateString('de-DE');
  }
</script>

<aside class="flex h-full flex-col border-l border-line bg-panel">
  <header class="flex items-center gap-2 border-b border-line px-3 py-2">
    <span class="flex-1 font-semibold">Aktivität</span>
    <label class="flex items-center gap-1.5 text-xs text-muted">
      <input type="checkbox" bind:checked={onlyAi} /> nur KI
    </label>
    <button class="icon-btn" onclick={() => (ws.panel = null)}><X size={15} /></button>
  </header>
  <div class="flex-1 overflow-y-auto px-2 py-1">
    {#each visible as c (c.id)}
      <div class="group flex items-start gap-2 rounded-md px-2 py-2 hover:bg-elevated">
        <div class="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-[10px] font-semibold
          {isAi(c) ? 'bg-ai/20 text-ai' : 'bg-accent/20 text-accent'}">
          {#if isAi(c)}<Bot size={11} />{:else}{c.actor_name.slice(0, 1).toUpperCase()}{/if}
        </div>
        <div class="min-w-0 flex-1">
          <div class="truncate {c.reverted_by ? 'text-faint line-through' : ''}">{c.summary}</div>
          <div class="text-[11px] text-faint">{c.actor_name}{c.client ? ` · ${c.client}` : ''} · {ago(c.at)}</div>
        </div>
        {#if !c.reverted_by}
          <button class="icon-btn !hidden !h-6 !w-6 group-hover:!inline-flex" title="Rückgängig machen" onclick={() => revert(c)}>
            <Undo2 size={13} />
          </button>
        {/if}
      </div>
    {:else}
      <div class="px-3 py-8 text-center text-faint">Noch keine Änderungen.</div>
    {/each}
  </div>
</aside>
