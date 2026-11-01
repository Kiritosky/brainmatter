<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { X } from '@lucide/svelte';
  import { TOUR, TOUR_SEEN_KEY } from '#lib/help.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { fadeQuick } from '#lib/ui/motion.ts';

  // The first look around: a spot on the real controls, one thing at a time.
  const CARD_WIDTH = 320;
  const GAP = 14;
  const MARGIN = 12;
  const SPOT_PADDING = 5;

  let index = $state(0);
  let spot = $state<{ x: number; y: number; width: number; height: number } | null>(null);
  let cardHeight = $state(180);
  let viewport = $state({ width: window.innerWidth, height: window.innerHeight });
  let card: HTMLDivElement | undefined = $state();

  const step = $derived(TOUR[index]);

  /** Beside the spot where there is room, otherwise above or below it. */
  const position = $derived.by(() => {
    if (!spot) return { left: (viewport.width - CARD_WIDTH) / 2, top: (viewport.height - cardHeight) / 2 };
    const clampTop = (y: number) => Math.max(MARGIN, Math.min(y, viewport.height - cardHeight - MARGIN));
    const clampLeft = (x: number) => Math.max(MARGIN, Math.min(x, viewport.width - CARD_WIDTH - MARGIN));
    // Nothing is beside an area this large, so the card goes on it, out of the way of its edges.
    if (spot.width > viewport.width / 2) return { left: spot.x + GAP * 2, top: spot.y + GAP * 2 };
    const right = spot.x + spot.width + GAP;
    if (right + CARD_WIDTH + MARGIN <= viewport.width) return { left: right, top: clampTop(spot.y) };
    const left = spot.x - GAP - CARD_WIDTH;
    if (left >= MARGIN) return { left, top: clampTop(spot.y) };
    const above = spot.y - GAP - cardHeight;
    return { left: clampLeft(spot.x), top: above >= MARGIN ? above : clampTop(spot.y + spot.height + GAP) };
  });

  function measure() {
    viewport = { width: window.innerWidth, height: window.innerHeight };
    const target = step.target ? document.querySelector(`[data-tour="${step.target}"]`) : null;
    const box = target?.getBoundingClientRect();
    // A control may be missing, say the graph's tools while it is empty.
    spot = box && box.width > 0
      ? { x: box.x - SPOT_PADDING, y: box.y - SPOT_PADDING, width: box.width + SPOT_PADDING * 2, height: box.height + SPOT_PADDING * 2 }
      : null;
  }

  async function go(to: number) {
    if (to >= TOUR.length) return finish();
    index = Math.max(0, to);
    await tick();
    measure();
    card?.focus();
  }

  function finish() {
    localStorage.setItem(TOUR_SEEN_KEY, '1');
    ws.tourOpen = false;
  }

  function onkeydowncapture(e: KeyboardEvent) {
    if (e.key === 'Escape') finish();
    else if (e.key === 'ArrowRight' || e.key === 'Enter') go(index + 1);
    else if (e.key === 'ArrowLeft') go(index - 1);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  onMount(() => {
    // Nothing else may be open on top of or under the tour.
    ws.select(null);
    ws.panel = null;
    go(0);
  });
</script>

<svelte:window {onkeydowncapture} onresize={measure} />

<div class="fixed inset-0 z-[70]" transition:fadeQuick>
  <!-- Catches clicks: this is a look at the controls, not yet their use. -->
  <div class="absolute inset-0" role="presentation"></div>
  <div class="spot" style={spot
    ? `left: ${spot.x}px; top: ${spot.y}px; width: ${spot.width}px; height: ${spot.height}px`
    : `left: ${viewport.width / 2}px; top: ${viewport.height / 2}px; width: 0; height: 0`}></div>

  <div class="surface card absolute rounded-xl p-4 outline-none" style="left: {position.left}px; top: {position.top}px; width: {CARD_WIDTH}px"
    role="dialog" aria-label="Einführung" tabindex="-1" bind:this={card} bind:clientHeight={cardHeight}>
    <div class="flex items-start gap-2">
      <h2 class="min-w-0 flex-1 text-[15px] font-semibold tracking-tight">{step.title}</h2>
      <button class="icon-btn -mt-1 -mr-1 shrink-0" aria-label="Einführung beenden" title="Einführung beenden (Esc)" onclick={finish}><X size={14} /></button>
    </div>
    <p class="mt-1 leading-relaxed text-muted">{step.text}</p>
    {#if step.keys}<p class="mt-2"><kbd class="kbd !h-6 !px-1.5 !text-xs">{step.keys}</kbd></p>{/if}
    <div class="mt-3.5 flex items-center gap-1.5">
      <span class="flex-1 text-xs text-faint tabular-nums">{index + 1} von {TOUR.length}</span>
      {#if index > 0}
        <button class="btn-ghost !h-7.5 text-xs" onclick={() => go(index - 1)}>Zurück</button>
      {/if}
      <button class="btn-primary !h-7.5 !px-3 text-xs" onclick={() => go(index + 1)}>{index === TOUR.length - 1 ? 'Fertig' : 'Weiter'}</button>
    </div>
  </div>
</div>

<style>
  .spot {
    position: absolute;
    border-radius: 10px;
    box-shadow: 0 0 0 9999px rgb(0 0 0 / 0.55);
    pointer-events: none;
    transition: all 260ms var(--ease-out-soft);
  }
  .card {
    transition: left 260ms var(--ease-out-soft), top 260ms var(--ease-out-soft);
  }
</style>
