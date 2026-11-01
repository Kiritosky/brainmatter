<script lang="ts">
  import { onDestroy, onMount, untrack } from 'svelte';
  import { Check } from '@lucide/svelte';
  import { api, errorMessage, type Node } from '#lib/api.ts';
  import { MAX_ATTACHMENT_BYTES, pastedAttachmentMarkdown, pastedFile } from '#lib/attachments.ts';
  import { EntryDraft } from '#lib/autosave.svelte.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';
  import MarkdownEditor from './MarkdownEditor.svelte';

  // Writing a documentation page, on the page itself and with all its width.
  let { node, focus, onclose }: { node: Node; focus: 'title' | 'body'; onclose: () => void } = $props();

  const draft = new EntryDraft(untrack(() => node.id), (message) => ws.notify(message));
  draft.accept(untrack(() => node));

  let titleField: HTMLInputElement | undefined = $state();
  let bodyStartsFocused = untrack(() => focus === 'body');

  $effect(() => {
    const n = node;
    untrack(() => draft.accept(n));
  });

  onMount(() => {
    if (untrack(() => focus) === 'title') titleField?.select();
  });
  onDestroy(() => draft.save());

  async function finish() {
    await draft.save();
    onclose();
  }

  async function completeTitles(prefix: string) {
    const hits = await api.search(prefix, { limit: 8 });
    return hits.map((h) => h.node.title).filter((t) => t !== draft.title);
  }

  async function pasteFile(file: File) {
    if (file.size > MAX_ATTACHMENT_BYTES) {
      ws.notify(`„${file.name}“ ist zu groß. Anhänge dürfen höchstens 200 MB haben.`);
      return null;
    }
    try {
      return pastedAttachmentMarkdown(await api.addAttachment(node.id, pastedFile(file)));
    } catch (e) {
      ws.notify(errorMessage(e));
      return null;
    }
  }

  let body: HTMLDivElement | undefined = $state();
</script>

<main class="min-w-0 flex-1" in:pop={{ duration: 140, y: 2 }}>
  <header class="sticky top-0 z-10 -mx-2 mb-3 flex h-10 items-center gap-2 bg-bg px-2">
    <span class="text-xs text-faint" aria-live="polite">
      {#if draft.inFlight || draft.dirty}Speichert …{:else}Gespeichert{/if}
    </span>
    <span class="min-w-0 flex-1 truncate text-xs text-faint"><kbd class="kbd">/</kbd> Formate <kbd class="ml-2 kbd">⌘B</kbd> Fett <kbd class="ml-2 kbd">⌘I</kbd> Kursiv <kbd class="ml-2 kbd">[[</kbd> Verweis</span>
    <button class="btn-ghost !h-7.5 shrink-0 text-xs" onclick={finish}><Check size={13} /> Fertig</button>
  </header>

  <input class="mb-3 w-full bg-transparent text-[26px] leading-tight font-semibold tracking-tight outline-none placeholder:text-faint"
    placeholder="Titel der Seite" aria-label="Titel der Seite" bind:this={titleField} bind:value={draft.title}
    oninput={() => draft.changed()}
    onkeydown={(e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        body?.querySelector<HTMLElement>('.cm-content')?.focus();
      } else if (e.key === 'Escape') {
        e.stopPropagation();
        finish();
      }
    }} />

  <div bind:this={body} class="min-h-[50vh] cursor-text">
    <MarkdownEditor
      value={draft.body}
      large
      autofocus={bodyStartsFocused}
      titles={completeTitles}
      onpastefile={pasteFile}
      onsave={() => draft.save()}
      onescape={finish}
      onchange={(text) => ((draft.body = text), draft.changed())} />
  </div>
</main>
