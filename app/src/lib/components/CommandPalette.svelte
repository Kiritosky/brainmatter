<script lang="ts">
  import { Plus, FileText, Link2 } from '@lucide/svelte';
  import { api, errorMessage, type SearchHit } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';

  const mode = $derived(ws.palette!);
  let text = $state('');
  let relation = $state('related_to');
  let hits = $state<SearchHit[]>([]);
  let active = $state(0);
  let seq = 0;

  // Entries + a "create" action as the last option.
  const options = $derived([
    ...hits.filter((h) => mode !== 'link' || h.node.id !== ws.selected).map((h) => ({ kind: 'hit' as const, hit: h })),
    ...(text.trim() ? [{ kind: 'create' as const }] : []),
  ]);

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
      if (current === 'open') {
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
    if (e.key === 'Escape') close();
    else if (e.key === 'ArrowDown') (active = Math.min(options.length - 1, active + 1)), e.preventDefault();
    else if (e.key === 'ArrowUp') (active = Math.max(0, active - 1)), e.preventDefault();
    else if (e.key === 'Enter') choose(e.metaKey ? options.length - 1 : active), e.preventDefault();
  }

  // Snippets mark matches with **…**; render them as <mark> safely.
  const parts = (s: string) => s.split('**').map((p, i) => ({ text: p, mark: i % 2 === 1 }));
</script>

<div class="fixed inset-0 z-50 flex justify-center bg-black/40 pt-[12vh]" role="presentation" onclick={close}>
  <div class="h-fit w-[620px] overflow-hidden rounded-xl border border-line bg-panel shadow-2xl" role="dialog" tabindex="-1"
    onclick={(e) => e.stopPropagation()} onkeydown={onkeydown}>
    <div class="flex items-center gap-2 border-b border-line px-4">
      {#if mode === 'link'}<Link2 size={15} class="text-accent" />{/if}
      <!-- svelte-ignore a11y_autofocus -- focus belongs in this field -->
      <input
        class="flex-1 bg-transparent py-3.5 text-[15px] outline-none placeholder:text-faint"
        placeholder={mode === 'link' ? 'Verbinden mit …' : 'Suchen oder neuen Eintrag anlegen …'}
        bind:value={text}
        autofocus />
      {#if mode === 'link'}
        <input class="w-32 rounded-md border border-line bg-elevated px-2 py-1 text-xs outline-none" bind:value={relation} title="Art der Beziehung" />
      {/if}
    </div>
    <div class="max-h-[50vh] overflow-y-auto p-1.5">
      {#each options as opt, i}
        <button
          class="flex w-full items-start gap-2.5 rounded-lg px-3 py-2 text-left {i === active ? 'bg-elevated' : ''}"
          onmouseenter={() => (active = i)}
          onclick={() => choose(i)}>
          {#if opt.kind === 'hit'}
            <FileText size={15} class="mt-0.5 shrink-0 text-faint" />
            <div class="min-w-0 flex-1">
              <div class="truncate">{opt.hit.node.title} <span class="text-xs text-faint">{opt.hit.node.kind}</span></div>
              {#if opt.hit.snippet}
                <div class="truncate text-xs text-muted">
                  {#each parts(opt.hit.snippet) as p}{#if p.mark}<mark class="bg-transparent text-accent">{p.text}</mark>{:else}{p.text}{/if}{/each}
                </div>
              {/if}
            </div>
          {:else}
            <Plus size={15} class="mt-0.5 shrink-0 text-accent" />
            <div class="flex-1">„{text.trim()}“ {mode === 'link' ? 'anlegen und verbinden' : 'als neuen Eintrag anlegen'}</div>
            <span class="text-[11px] text-faint">⌘↵</span>
          {/if}
        </button>
      {:else}
        <div class="px-3 py-6 text-center text-faint">Tippe, um zu suchen.</div>
      {/each}
    </div>
  </div>
</div>
