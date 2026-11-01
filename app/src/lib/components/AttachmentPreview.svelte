<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Download, FileQuestion, X } from '@lucide/svelte';
  import { api, errorMessage, type Attachment } from '#lib/api.ts';
  import { attachmentText, attachmentUrl, formatSize, previewKind } from '#lib/attachments.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { fadeQuick, pop } from '#lib/ui/motion.ts';

  let { attachment, onclose }: { attachment: Attachment; onclose: () => void } = $props();

  const MAX_TEXT_CHARS = 200_000;

  const kind = $derived(previewKind(attachment.mime));
  let url = $state<string | null>(null);
  let text = $state<string | null>(null);
  let failed = $state<string | null>(null);

  onMount(async () => {
    try {
      if (kind === 'text') text = (await attachmentText(attachment)).slice(0, MAX_TEXT_CHARS);
      else if (kind !== 'none') url = await attachmentUrl(attachment);
    } catch (e) {
      failed = errorMessage(e);
    }
  });

  onDestroy(() => {
    if (url) URL.revokeObjectURL(url);
  });

  async function save() {
    try {
      if (await api.exportAttachment(attachment.id)) ws.notify(`„${attachment.filename}“ gespeichert. Diese Kopie ist nicht verschlüsselt.`);
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  // Capture: Esc must close this and not also the entry behind it.
  function onkeydowncapture(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<svelte:window {onkeydowncapture} />

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-10 backdrop-blur-[2px]" role="presentation" transition:fadeQuick onclick={onclose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -- the window handles Esc -->
  <div class="surface flex h-full max-h-[780px] w-full max-w-[980px] flex-col overflow-hidden rounded-xl" role="dialog" aria-label={attachment.filename} tabindex="-1"
    transition:pop onclick={(e) => e.stopPropagation()}>
    <header class="flex h-12 shrink-0 items-center gap-3 border-b border-line-soft pr-2 pl-5">
      <span class="min-w-0 truncate font-medium">{attachment.filename}</span>
      <span class="shrink-0 text-xs text-faint tabular-nums">{formatSize(attachment.size)}</span>
      <div class="flex-1"></div>
      <button class="btn-ghost !h-7.5 text-xs" onclick={save}><Download size={13} /> Sichern unter …</button>
      <button class="icon-btn" aria-label="Schließen" onclick={onclose}><X size={16} /></button>
    </header>

    <div class="relative min-h-0 flex-1 bg-bg/40">
      {#if failed}
        <div class="flex h-full items-center justify-center px-10 text-center text-danger">{failed}</div>
      {:else if kind === 'image' && url}
        <img class="h-full w-full object-contain p-4" src={url} alt={attachment.filename} in:pop={{ duration: 180, y: 0 }} />
      {:else if kind === 'pdf' && url}
        <iframe class="h-full w-full border-0" src={url} title={attachment.filename}></iframe>
      {:else if kind === 'text' && text !== null}
        <pre class="selectable h-full overflow-auto px-6 py-5 font-mono text-[12.5px] leading-relaxed whitespace-pre-wrap text-muted">{text}</pre>
      {:else if kind === 'none'}
        <div class="flex h-full flex-col items-center justify-center gap-3 text-center">
          <div class="flex h-12 w-12 items-center justify-center rounded-xl bg-raised text-faint"><FileQuestion size={22} /></div>
          <p class="max-w-xs text-muted">Für diesen Dateityp gibt es keine Vorschau. Du kannst die Datei über „Sichern unter“ öffnen.</p>
        </div>
      {/if}
    </div>
  </div>
</div>
