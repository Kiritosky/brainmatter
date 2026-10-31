<script lang="ts" generics="T extends string">
  let {
    value = $bindable(),
    options,
    label,
    onchange,
  }: {
    value: T;
    options: { value: T; label: string }[];
    label?: string;
    onchange?: (value: T) => void;
  } = $props();

  const index = $derived(options.findIndex((o) => o.value === value));
  const share = $derived(`(100% - 4px) / ${options.length}`);
</script>

<div
  role="radiogroup"
  aria-label={label}
  class="relative grid rounded-lg bg-bg/70 p-0.5 ring-1 ring-line-soft"
  style="grid-template-columns: repeat({options.length}, minmax(0, 1fr))">
  <span
    aria-hidden="true"
    class="absolute top-0.5 bottom-0.5 rounded-md bg-raised shadow-sm ring-1 ring-white/5 transition-[left] duration-200 ease-(--ease-out-soft)"
    style="left: calc(2px + {index} * {share}); width: calc({share})"></span>
  {#each options as opt (opt.value)}
    <button
      type="button"
      role="radio"
      aria-checked={opt.value === value}
      class="relative h-6.5 rounded-md px-3 text-xs font-medium transition-colors duration-150 {opt.value === value ? 'text-text' : 'text-muted hover:text-text'}"
      onclick={() => ((value = opt.value), onchange?.(opt.value))}>
      {opt.label}
    </button>
  {/each}
</div>
