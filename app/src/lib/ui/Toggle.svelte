<script lang="ts">
  let {
    checked = $bindable(false),
    disabled = false,
    label,
    onchange,
  }: {
    checked?: boolean;
    disabled?: boolean;
    /** Accessible name when there is no visible label next to it. */
    label?: string;
    onchange?: (checked: boolean) => void;
  } = $props();

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={toggle}
  class="toggle"
  class:on={checked}>
  <span class="knob"></span>
</button>

<style>
  .toggle {
    position: relative;
    width: 36px;
    height: 21px;
    flex-shrink: 0;
    border-radius: 999px;
    background: var(--color-line);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.04);
    transition: background-color 200ms ease, box-shadow 200ms ease;
    cursor: pointer;
  }
  .toggle:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-line) 70%, var(--color-muted));
  }
  .toggle.on {
    background: var(--color-accent);
    box-shadow: 0 0 12px -2px color-mix(in srgb, var(--color-accent) 60%, transparent);
  }
  .toggle:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--color-accent) 70%, transparent);
    outline-offset: 2px;
  }
  .toggle:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 17px;
    height: 17px;
    border-radius: 999px;
    background: white;
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.35);
    transition: transform 220ms cubic-bezier(0.34, 1.4, 0.64, 1), width 160ms ease;
  }
  .toggle:active:not(:disabled) .knob {
    width: 21px;
  }
  .toggle.on .knob {
    transform: translateX(15px);
  }
  .toggle.on:active:not(:disabled) .knob {
    transform: translateX(11px);
  }
  @media (prefers-reduced-motion: reduce) {
    .toggle,
    .knob {
      transition: none;
    }
  }
</style>
