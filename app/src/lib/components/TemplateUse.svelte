<script lang="ts">
  import { Check, Copy, FilePlus } from '@lucide/svelte';
  import { api, errorMessage, type Id } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';

  let { body, title, projects }: { body: string; title: string; projects: Id[] } = $props();

  // Same rules as crates/bm-core/src/templates.rs.
  const PLACEHOLDER = /\{\{\s*([\p{L}\p{N}_-]+)\s*\}\}/gu;

  let values = $state<Record<string, string>>({});
  let copied = $state(false);

  const names = $derived([...new Set([...body.matchAll(PLACEHOLDER)].map((m) => m[1]))]);
  const filled = $derived(body.replace(PLACEHOLDER, (whole, name) => values[name] || whole));
  const open = $derived(names.filter((n) => !values[n]).length);

  /** Turns the filled-in text into an entry of its own, in the template's projects. */
  async function createEntry() {
    try {
      const first = names.map((n) => values[n]).find(Boolean);
      const node = await api.createNode({ title: first ? `${title}: ${first}` : title, body: filled, projects });
      await ws.select(node.id, true);
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  async function copy() {
    await navigator.clipboard.writeText(filled);
    copied = true;
    setTimeout(() => (copied = false), 1600);
  }
</script>

<div class="section-title mt-7">Verwenden</div>
<div class="card p-3">
  {#if names.length}
    <div class="mb-3 flex flex-col gap-1.5">
      {#each names as name (name)}
        <label class="flex items-center gap-2.5">
          <span class="w-24 shrink-0 truncate text-xs text-muted" title={name}>{name}</span>
          <input class="field !h-7.5 min-w-0 flex-1" bind:value={values[name]} aria-label="Wert für {name}" />
        </label>
      {/each}
    </div>
  {/if}
  <pre class="selectable max-h-48 overflow-auto rounded-lg border border-line-soft bg-bg/50 p-2.5 font-mono text-[11.5px] leading-relaxed whitespace-pre-wrap text-muted">{filled}</pre>
  <div class="mt-2.5 flex items-center gap-1.5">
    <span class="min-w-0 flex-1 truncate text-xs text-faint">{open ? `${open} Platzhalter noch offen` : names.length ? 'Alles ausgefüllt' : 'Ohne Platzhalter'}</span>
    <button class="btn-ghost !h-7 shrink-0 text-xs" onclick={copy}>
      {#if copied}<Check size={13} class="text-ok" /> Kopiert{:else}<Copy size={13} /> Kopieren{/if}
    </button>
    <button class="btn-primary !h-7 shrink-0 !px-2.5 text-xs" onclick={createEntry}><FilePlus size={13} /> Als Eintrag anlegen</button>
  </div>
</div>
