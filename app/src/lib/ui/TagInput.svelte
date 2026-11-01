<script lang="ts">
  import { ws } from '#lib/workspace.svelte.ts';

  // Adding a tag: what exists already is offered, so that one word stays one tag.
  let { exclude = [], onadd }: { exclude?: string[]; onadd: (tag: string) => void } = $props();

  const MAX_SUGGESTIONS = 8;

  let text = $state('');
  let open = $state(false);
  let index = $state(-1);

  const typed = $derived(text.trim().replace(/^#/, '').toLowerCase());
  const suggestions = $derived(
    ws.tagCounts
      .map((t) => t.tag)
      .filter((t) => !exclude.includes(t) && t.includes(typed))
      .slice(0, MAX_SUGGESTIONS),
  );

  function add(tag: string) {
    text = '';
    index = -1;
    if (tag) onadd(tag);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') index = Math.min(index + 1, suggestions.length - 1);
    else if (e.key === 'ArrowUp') index = Math.max(index - 1, -1);
    else if (e.key === 'Enter') add(suggestions[index] ?? typed);
    else if (e.key === 'Escape' && (text || open)) {
      e.stopPropagation();
      text = '';
      open = false;
    } else return;
    e.preventDefault();
  }
</script>

<div class="relative">
  <input class="h-6 w-24 rounded-md bg-transparent px-1 text-xs text-muted outline-none placeholder:text-faint focus:bg-elevated" placeholder="+ Tag" aria-label="Tag hinzufügen"
    bind:value={text} {onkeydown} oninput={() => ((index = -1), (open = true))} onfocus={() => (open = true)} onblur={() => (open = false)} />
  {#if open && suggestions.length}
    <div class="surface absolute top-7 left-0 z-50 w-44 rounded-lg p-1" role="listbox" aria-label="Vorhandene Tags">
      {#each suggestions as tag, i (tag)}
        <!-- On mousedown, because the click would come after the field lost focus. -->
        <button type="button" class="flex w-full rounded-md px-2 py-1 text-left text-xs transition-colors duration-75 {i === index ? 'bg-text/8 text-text' : 'text-muted hover:bg-text/5'}"
          role="option" aria-selected={i === index} onmousedown={(e) => (e.preventDefault(), add(tag))}><span class="truncate">#{tag}</span></button>
      {/each}
    </div>
  {/if}
</div>
