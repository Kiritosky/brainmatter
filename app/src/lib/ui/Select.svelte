<script lang="ts" generics="T extends string | number">
  import { ChevronDown, Check } from '@lucide/svelte';
  import { pop } from './motion';

  let {
    value = $bindable(),
    options,
    onchange,
    label,
  }: {
    value: T;
    options: { value: T; label: string }[];
    onchange?: (value: T) => void;
    label?: string;
  } = $props();

  let open = $state(false);
  let active = $state(0);
  let button: HTMLButtonElement | undefined = $state();

  const current = $derived(options.find((o) => o.value === value)?.label ?? '');

  function show() {
    active = Math.max(0, options.findIndex((o) => o.value === value));
    open = true;
  }

  function choose(i: number) {
    const opt = options[i];
    open = false;
    button?.focus();
    if (opt && opt.value !== value) {
      value = opt.value;
      onchange?.(opt.value);
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (!open) {
      if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(e.key)) (e.preventDefault(), show());
      return;
    }
    if (e.key === 'Escape') (e.preventDefault(), e.stopPropagation(), (open = false));
    else if (e.key === 'ArrowDown') (e.preventDefault(), (active = Math.min(options.length - 1, active + 1)));
    else if (e.key === 'ArrowUp') (e.preventDefault(), (active = Math.max(0, active - 1)));
    else if (e.key === 'Enter' || e.key === ' ') (e.preventDefault(), choose(active));
  }
</script>

<div class="relative">
  <button
    bind:this={button}
    type="button"
    class="flex h-8 min-w-36 items-center gap-2 rounded-lg border border-line bg-elevated pr-2 pl-3 text-left transition-colors hover:border-faint focus-visible:border-accent focus-visible:outline-none"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-label={label}
    onclick={() => (open ? (open = false) : show())}
    {onkeydown}>
    <span class="flex-1">{current}</span>
    <ChevronDown size={14} class="text-muted transition-transform duration-200 {open ? 'rotate-180' : ''}" />
  </button>

  {#if open}
    <div class="fixed inset-0 z-40" role="presentation" onclick={() => (open = false)}></div>
    <ul
      role="listbox"
      class="absolute right-0 z-50 mt-1.5 min-w-full overflow-hidden rounded-lg border border-line bg-elevated p-1 shadow-2xl shadow-black/40"
      transition:pop={{ duration: 140, y: -4 }}>
      {#each options as opt, i (opt.value)}
        <li
          role="option"
          aria-selected={opt.value === value}
          class="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-1.5 transition-colors {i === active ? 'bg-accent/15 text-text' : 'text-muted'}"
          onmouseenter={() => (active = i)}
          onclick={() => choose(i)}
          onkeydown={() => {}}>
          <span class="flex-1 whitespace-nowrap">{opt.label}</span>
          {#if opt.value === value}<Check size={13} class="text-accent" />{/if}
        </li>
      {/each}
    </ul>
  {/if}
</div>
