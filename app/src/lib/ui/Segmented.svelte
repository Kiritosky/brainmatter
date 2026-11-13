<script lang="ts" generics="T extends string">
  let {
    value = $bindable(),
    options,
    label,
    onchange,
    compact = false,
  }: {
    value: T;
    options: { value: T; label: string }[];
    label?: string;
    onchange?: (value: T) => void;
    /** For many options in a narrow panel: each as wide as its label needs, the rest shared. */
    compact?: boolean;
  } = $props();

  const index = $derived(options.findIndex((o) => o.value === value));
  const share = $derived(`(100% - 4px) / ${options.length}`);

  // Compact options differ in width, so the highlight follows the chosen one's box.
  let group: HTMLDivElement | undefined = $state();
  let box = $state<{ left: number; width: number } | null>(null);
  $effect(() => {
    if (!compact || !group) return;
    const chosen = group.querySelectorAll<HTMLButtonElement>('button')[index];
    if (!chosen) return;
    const measure = () => (box = { left: chosen.offsetLeft, width: chosen.offsetWidth });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(group);
    return () => observer.disconnect();
  });
  const highlight = $derived(
    compact && box ? `left: ${box.left}px; width: ${box.width}px` : `left: calc(2px + ${index} * ${share}); width: calc(${share})`,
  );
</script>

<div
  role="radiogroup"
  aria-label={label}
  bind:this={group}
  class="relative grid rounded-lg bg-bg/70 p-0.5 ring-1 ring-line-soft"
  style="grid-template-columns: repeat({options.length}, {compact ? 'minmax(max-content, 1fr)' : 'minmax(0, 1fr)'})">
  <span
    aria-hidden="true"
    class="absolute top-0.5 bottom-0.5 rounded-md bg-raised shadow-sm ring-1 ring-white/5 transition-[left,width] duration-200 ease-(--ease-out-soft) {compact && !box ? 'opacity-0' : ''}"
    style={highlight}></span>
  {#each options as opt (opt.value)}
    <button
      type="button"
      role="radio"
      aria-checked={opt.value === value}
      class="relative h-6.5 rounded-md font-medium transition-colors duration-150 {compact ? 'px-2 text-[11.5px]' : 'truncate px-3 text-xs'} {opt.value === value ? 'text-text' : 'text-muted hover:text-text'}"
      onclick={() => ((value = opt.value), onchange?.(opt.value))}>
      {opt.label}
    </button>
  {/each}
</div>
