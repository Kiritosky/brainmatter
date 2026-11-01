<script lang="ts">
  import { Sparkles, X } from '@lucide/svelte';
  import type { Release } from '#lib/changelog.ts';
  import { renderMarkdown } from '#lib/markdown.ts';
  import { fadeQuick, pop } from '#lib/ui/motion.ts';

  let { items, heading, onclose }: { items: Release[]; heading: string; onclose: () => void } = $props();

  const when = (date: string) => {
    const parsed = new Date(date);
    return isNaN(parsed.getTime()) ? date : parsed.toLocaleDateString('de-DE', { day: 'numeric', month: 'long', year: 'numeric' });
  };

  // Capture: Esc must close this and nothing behind it.
  function onkeydowncapture(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<svelte:window {onkeydowncapture} />

<div class="fixed inset-0 z-[55] flex items-center justify-center bg-black/45 p-10 backdrop-blur-[2px]" role="presentation" transition:fadeQuick onclick={onclose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -- the window handles Esc -->
  <div class="surface flex max-h-full w-[560px] flex-col overflow-hidden rounded-2xl" role="dialog" aria-label={heading} tabindex="-1" transition:pop onclick={(e) => e.stopPropagation()}>
    <header class="flex shrink-0 items-center gap-3 border-b border-line-soft py-3 pr-3 pl-5">
      <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent/12 text-accent"><Sparkles size={16} /></div>
      <h2 class="min-w-0 flex-1 truncate text-[15px] font-semibold tracking-tight">{heading}</h2>
      <button class="icon-btn" aria-label="Schließen" onclick={onclose}><X size={16} /></button>
    </header>
    <div class="min-h-0 overflow-y-auto px-5 pt-4 pb-5">
      {#each items as release (release.version)}
        <section class="mb-6 last:mb-0">
          <div class="mb-1 flex items-baseline gap-2">
            <span class="font-semibold">Version {release.version}</span>
            {#if release.date}<span class="text-xs text-faint">{when(release.date)}</span>{/if}
          </div>
          <!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized by DOMPurify -->
          <div class="prose-bm selectable">{@html renderMarkdown(release.notes)}</div>
        </section>
      {:else}
        <p class="py-6 text-center text-faint">Für diese Version gibt es keine Notizen.</p>
      {/each}
    </div>
  </div>
</div>
