<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    checked = $bindable(false),
    onchange,
    children,
  }: {
    checked?: boolean;
    onchange?: (checked: boolean) => void;
    children?: Snippet;
  } = $props();
</script>

<label class="group flex cursor-pointer items-center gap-2.5">
  <input type="checkbox" class="peer sr-only" bind:checked onchange={() => onchange?.(checked)} />
  <span class="box" class:on={checked} aria-hidden="true">
    <svg viewBox="0 0 16 16"><path d="M3.5 8.5l3 3 6-7" /></svg>
  </span>
  {#if children}<span>{@render children()}</span>{/if}
</label>

<style>
  .box {
    display: inline-flex;
    width: 18px;
    height: 18px;
    flex-shrink: 0;
    border-radius: 6px;
    border: 1.5px solid var(--color-faint);
    transition: background-color 160ms ease, border-color 160ms ease, transform 120ms ease;
  }
  label:active .box {
    transform: scale(0.92);
  }
  .box.on {
    background: var(--color-accent);
    border-color: var(--color-accent);
  }
  :global(.peer:focus-visible) + .box {
    outline: 2px solid color-mix(in srgb, var(--color-accent) 70%, transparent);
    outline-offset: 2px;
  }
  svg {
    width: 100%;
    height: 100%;
    fill: none;
    stroke: white;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-dasharray: 14;
    stroke-dashoffset: 14;
    transition: stroke-dashoffset 220ms ease 40ms;
  }
  .box.on svg {
    stroke-dashoffset: 0;
  }
  @media (prefers-reduced-motion: reduce) {
    .box,
    svg {
      transition: none;
    }
  }
</style>
