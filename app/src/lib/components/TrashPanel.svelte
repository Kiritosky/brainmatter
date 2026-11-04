<script lang="ts">
  import { locale, num, t } from '#lib/i18n.ts';
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

  const restore = (item: TrashedNode) =>
    run(async () => {
      await api.restoreNode(item.node.id);
      await ws.select(item.node.id, true);
    });

  const emptyTrash = () =>
    run(async () => {
      for (const item of items) await api.purgeNode(item.node.id);
    });

  const when = (ms: number) => new Date(ms).toLocaleDateString(locale, { day: '2-digit', month: '2-digit', year: 'numeric' });
</script>

<aside class="flex h-full w-100 flex-col border-l border-line bg-panel">
  <header class="px-4 pt-3 pb-3" data-tauri-drag-region>
    <div class="flex items-center">
      <h2 class="flex-1 text-[15px] font-semibold tracking-tight">{t('Papierkorb', 'Trash')}</h2>
      <button class="icon-btn -mr-1" aria-label={t('Schließen', 'Close')} onclick={() => (ws.panel = null)}><X size={15} /></button>
    </div>
  </header>

  <div class="flex-1 overflow-y-auto px-2 pb-3">
    {#each items as item (item.node.id)}
      <div animate:flip={{ duration: 180 }}>
        <div class="group flex h-9 items-center gap-2.5 rounded-lg px-2 transition-colors hover:bg-elevated">
          <span class="h-2 w-2 shrink-0 rounded-full opacity-60" style="background: {KIND_COLORS[item.node.kind] ?? KIND_COLORS.note}"></span>
          <span class="min-w-0 flex-1 truncate text-muted">{item.node.title}</span>
          <span class="shrink-0 text-[11px] text-faint tabular-nums">{when(item.deleted_at)}</span>
          <div class="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
            <button class="icon-btn !h-6.5 !w-6.5 hover:!text-accent" aria-label={t(`„${item.node.title}“ wiederherstellen`, `Restore “${item.node.title}”`)} title={t('Wiederherstellen', 'Restore')} disabled={busy} onclick={() => restore(item)}><RotateCcw size={13} /></button>
            <button class="icon-btn !h-6.5 !w-6.5 hover:!text-danger" aria-label={t(`„${item.node.title}“ endgültig löschen`, `Delete “${item.node.title}” for good`)} title={t('Endgültig löschen', 'Delete for good')} disabled={busy}
              onclick={() => (confirming = confirming === item.node.id ? null : item.node.id)}><Trash2 size={13} /></button>
          </div>
        </div>
        {#if confirming === item.node.id}
          <div class="mx-2 mb-1.5 rounded-lg border border-danger/25 bg-danger/8 p-2.5" in:pop={{ duration: 140, y: -3 }}>
            <p class="text-xs leading-relaxed text-muted">{t('Entfernt den Eintrag mit Verlauf, Verbindungen und Anhängen. Das lässt sich nicht rückgängig machen.', 'Removes the entry with its history, links and attachments. This cannot be undone.')}</p>
            <div class="mt-2 flex justify-end gap-1.5">
              <button class="btn-ghost !h-7 text-xs" onclick={() => (confirming = null)}>{t('Behalten', 'Keep')}</button>
              <button class="btn-danger !h-7 text-xs" disabled={busy} onclick={() => run(() => api.purgeNode(item.node.id))}>{t('Endgültig löschen', 'Delete for good')}</button>
            </div>
          </div>
        {/if}
      </div>
    {:else}
      <div class="flex flex-col items-center px-8 pt-16 text-center" in:pop={{ duration: 240, y: 8 }}>
        <div class="mb-3 flex h-11 w-11 items-center justify-center rounded-xl bg-raised text-faint"><Trash2 size={20} /></div>
        <div class="font-medium">{t('Der Papierkorb ist leer', 'The trash is empty')}</div>
        <p class="mt-1 text-xs leading-relaxed text-muted">{t('Gelöschte Einträge bleiben hier, bis du sie wiederherstellst oder endgültig entfernst.', 'Deleted entries stay here until you restore them or remove them for good.')}</p>
      </div>
    {/each}
  </div>

  {#if items.length}
    <footer class="border-t border-line-soft p-3">
      {#if confirming === 'all'}
        <div in:pop={{ duration: 140, y: 3 }}>
          <p class="mb-2 text-xs leading-relaxed text-muted">{t(
            `${items.length === 1 ? 'Ein Eintrag wird' : `${num(items.length)} Einträge werden`} mit Verlauf und Anhängen entfernt. In älteren Backups bleibt der Text bis zu deren Ablauf erhalten, die Anhänge nicht.`,
            `${items.length === 1 ? 'One entry is' : `${num(items.length)} entries are`} removed with their history and attachments. Older backups keep the text until they expire, but not the attachments.`,
          )}</p>
          <div class="flex justify-end gap-1.5">
            <button class="btn-ghost !h-7 text-xs" onclick={() => (confirming = null)}>{t('Behalten', 'Keep')}</button>
            <button class="btn-danger !h-7 text-xs" disabled={busy} onclick={emptyTrash}>{t('Endgültig löschen', 'Delete for good')}</button>
          </div>
        </div>
      {:else}
        <button class="btn-ghost !h-7.5 w-full text-xs" onclick={() => (confirming = 'all')}><Trash2 size={13} /> {t('Papierkorb leeren', 'Empty trash')}</button>
      {/if}
    </footer>
  {/if}
</aside>
