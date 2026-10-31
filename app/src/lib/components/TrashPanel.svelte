<script lang="ts">
  import { flip } from 'svelte/animate';
  import { RotateCcw, Trash2, X } from '@lucide/svelte';
  import { api, errorMessage, type TrashedNode } from '#lib/api.ts';
  import { KIND_COLORS, ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';

  let items = $state.raw<TrashedNode[]>([]);
  /** The entry to erase for good, or `all`, once clicked and not yet confirmed. */
  let confirming = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    void ws.revision;
    load();
  });

  const load = async () => (items = await api.trash().catch(() => []));

  async function run(fn: () => Promise<unknown>) {
    busy = true;
    try {
      await fn();
    } catch (e) {
      ws.notify(errorMessage(e));
    } finally {
      busy = false;
      confirming = null;
      await load();
    }
  }

  const restore = (t: TrashedNode) =>
    run(async () => {
      await api.restoreNode(t.node.id);
      await ws.select(t.node.id, true);
    });

  const emptyTrash = () =>
    run(async () => {
      for (const t of items) await api.purgeNode(t.node.id);
    });

  const when = (ms: number) => new Date(ms).toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' });
</script>

<aside class="flex h-full w-100 flex-col border-l border-line bg-panel">
  <header class="px-4 pt-3 pb-3" data-tauri-drag-region>
    <div class="flex items-center">
      <h2 class="flex-1 text-[15px] font-semibold tracking-tight">Papierkorb</h2>
      <button class="icon-btn -mr-1" aria-label="Schließen" onclick={() => (ws.panel = null)}><X size={15} /></button>
    </div>
  </header>

  <div class="flex-1 overflow-y-auto px-2 pb-3">
    {#each items as t (t.node.id)}
      <div animate:flip={{ duration: 180 }}>
        <div class="group flex h-9 items-center gap-2.5 rounded-lg px-2 transition-colors hover:bg-elevated">
          <span class="h-2 w-2 shrink-0 rounded-full opacity-60" style="background: {KIND_COLORS[t.node.kind] ?? KIND_COLORS.note}"></span>
          <span class="min-w-0 flex-1 truncate text-muted">{t.node.title}</span>
          <span class="shrink-0 text-[11px] text-faint tabular-nums">{when(t.deleted_at)}</span>
          <div class="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
            <button class="icon-btn !h-6.5 !w-6.5 hover:!text-accent" aria-label="„{t.node.title}“ wiederherstellen" title="Wiederherstellen" disabled={busy} onclick={() => restore(t)}><RotateCcw size={13} /></button>
            <button class="icon-btn !h-6.5 !w-6.5 hover:!text-danger" aria-label="„{t.node.title}“ endgültig löschen" title="Endgültig löschen" disabled={busy}
              onclick={() => (confirming = confirming === t.node.id ? null : t.node.id)}><Trash2 size={13} /></button>
          </div>
        </div>
        {#if confirming === t.node.id}
          <div class="mx-2 mb-1.5 rounded-lg border border-danger/25 bg-danger/8 p-2.5" in:pop={{ duration: 140, y: -3 }}>
            <p class="text-xs leading-relaxed text-muted">Entfernt den Eintrag mit Verlauf, Verbindungen und Anhängen. Das lässt sich nicht rückgängig machen.</p>
            <div class="mt-2 flex justify-end gap-1.5">
              <button class="btn-ghost !h-7 text-xs" onclick={() => (confirming = null)}>Behalten</button>
              <button class="btn-danger !h-7 text-xs" disabled={busy} onclick={() => run(() => api.purgeNode(t.node.id))}>Endgültig löschen</button>
            </div>
          </div>
        {/if}
      </div>
    {:else}
      <div class="flex flex-col items-center px-8 pt-16 text-center" in:pop={{ duration: 240, y: 8 }}>
        <div class="mb-3 flex h-11 w-11 items-center justify-center rounded-xl bg-raised text-faint"><Trash2 size={20} /></div>
        <div class="font-medium">Der Papierkorb ist leer</div>
        <p class="mt-1 text-xs leading-relaxed text-muted">Gelöschte Einträge bleiben hier, bis du sie wiederherstellst oder endgültig entfernst.</p>
      </div>
    {/each}
  </div>

  {#if items.length}
    <footer class="border-t border-line-soft p-3">
      {#if confirming === 'all'}
        <div in:pop={{ duration: 140, y: 3 }}>
          <p class="mb-2 text-xs leading-relaxed text-muted">{items.length === 1 ? 'Ein Eintrag wird' : `${items.length} Einträge werden`} mit Verlauf und Anhängen entfernt. In älteren Backups bleibt der Text bis zu deren Ablauf erhalten, die Anhänge nicht.</p>
          <div class="flex justify-end gap-1.5">
            <button class="btn-ghost !h-7 text-xs" onclick={() => (confirming = null)}>Behalten</button>
            <button class="btn-danger !h-7 text-xs" disabled={busy} onclick={emptyTrash}>Endgültig löschen</button>
          </div>
        </div>
      {:else}
        <button class="btn-ghost !h-7.5 w-full text-xs" onclick={() => (confirming = 'all')}><Trash2 size={13} /> Papierkorb leeren</button>
      {/if}
    </footer>
  {/if}
</aside>
