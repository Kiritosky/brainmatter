<script lang="ts">
  import { onDestroy } from 'svelte';
  import { Pause, Play, X } from '@lucide/svelte';
  import { createdAt, type GraphNode } from '#lib/api.ts';
  import { pop } from '#lib/ui/motion.ts';

  let {
    nodes,
    until = $bindable(),
    onclose,
  }: { nodes: GraphNode[]; until: number; onclose: () => void } = $props();

  const PLAY_MS = 8000;
  /** Redrawing a large graph is not free; a dozen steps a second look smooth. */
  const FRAME_MS = 80;

  // Sorted once per snapshot, so the count below is a quick binary search.
  const born = $derived(nodes.map((n) => createdAt(n.id)).sort((a, b) => a - b));
  const start = $derived(born[0] ?? 0);
  const end = $derived(Math.max(born[born.length - 1] ?? 0, start + 1));
  const share = $derived(Math.min(1, Math.max(0, (until - start) / (end - start))));

  const count = $derived.by(() => {
    let [lo, hi] = [0, born.length];
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (born[mid] <= until) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  });

  const label = $derived(new Date(until).toLocaleDateString('de-DE', { day: 'numeric', month: 'short', year: 'numeric' }));

  let track: HTMLDivElement | undefined = $state();
  let playing = $state(false);
  let timer: ReturnType<typeof setInterval> | undefined;

  function pause() {
    playing = false;
    clearInterval(timer);
  }

  function play() {
    // From the end, playing starts over; that is what the button promises.
    if (share >= 1) until = start;
    playing = true;
    const step = ((end - start) * FRAME_MS) / PLAY_MS;
    timer = setInterval(() => {
      until = Math.min(end, until + step);
      if (until >= end) pause();
    }, FRAME_MS);
  }

  onDestroy(pause);

  function seek(e: PointerEvent) {
    const box = track!.getBoundingClientRect();
    until = start + ((e.clientX - box.left) / box.width) * (end - start);
    until = Math.min(end, Math.max(start, until));
  }

  function onpointerdown(e: PointerEvent) {
    pause();
    track!.setPointerCapture(e.pointerId);
    seek(e);
  }

  function onkeydown(e: KeyboardEvent) {
    const step = (end - start) / 50;
    if (e.key === 'ArrowLeft') until = Math.max(start, until - step);
    else if (e.key === 'ArrowRight') until = Math.min(end, until + step);
    else if (e.key === ' ') playing ? pause() : play();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<div class="glass-pill w-full gap-2 !px-1.5" in:pop={{ duration: 180, y: 8 }}>
  <button class="icon-btn shrink-0" aria-label={playing ? 'Anhalten' : 'Abspielen'} title={playing ? 'Anhalten' : 'Wachstum abspielen'} onclick={() => (playing ? pause() : play())}>
    {#if playing}<Pause size={14} />{:else}<Play size={14} />{/if}
  </button>

  <div class="group relative flex h-7 min-w-0 flex-1 cursor-pointer items-center" bind:this={track}
    role="slider" tabindex="0" aria-label="Zeitpunkt" aria-valuemin={start} aria-valuemax={end} aria-valuenow={Math.round(until)} aria-valuetext={label}
    {onpointerdown}
    onpointermove={(e) => e.buttons && seek(e)}
    {onkeydown}>
    <div class="h-1 w-full overflow-hidden rounded-full bg-line">
      <div class="h-full rounded-full bg-accent" style="width: {share * 100}%"></div>
    </div>
    <div class="absolute h-3.5 w-3.5 -translate-x-1/2 rounded-full bg-white shadow-md ring-1 ring-black/15 transition-transform duration-100 group-hover:scale-110 group-" style="left: {share * 100}%"></div>
  </div>

  <div class="w-34 shrink-0 text-right text-[11px] leading-tight tabular-nums">
    <div class="font-medium text-text">{label}</div>
    <div class="text-faint">{count} von {born.length} Einträgen</div>
  </div>
  <button class="icon-btn shrink-0" aria-label="Zeitregler schließen" title="Zeitregler schließen" onclick={onclose}><X size={14} /></button>
</div>
