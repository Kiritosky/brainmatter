<script lang="ts">
  import { flip } from 'svelte/animate';
  import { BookMarked, Bot, Plus, Search, X } from '@lucide/svelte';
  import { api, type TemplateInfo } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';

  const SEARCH_FROM = 6;

  let items = $state.raw<TemplateInfo[]>([]);
  let query = $state('');
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? items.filter((t) => t.node.title.toLowerCase().includes(q) || t.projects.some((p) => p.name.toLowerCase().includes(q))) : items;
  });

  $effect(() => {
    void ws.revision;
    api.templates().then((found) => (items = found)).catch(() => {});
  });
</script>

<aside class="flex h-full w-100 flex-col border-l border-line bg-panel">
  <header class="px-4 pt-3 pb-3" data-tauri-drag-region>
    <div class="flex items-center gap-1">
      <h2 class="flex-1 text-[15px] font-semibold tracking-tight">Bibliothek</h2>
      <button class="btn-ghost !h-7 text-xs" onclick={() => (ws.templateWizard = true)}><Plus size={13} /> Neue Vorlage</button>
      <button class="icon-btn -mr-1" aria-label="Schließen" onclick={() => (ws.panel = null)}><X size={15} /></button>
    </div>
    <p class="mt-1.5 text-xs leading-relaxed text-muted">Wiederverwendbare Texte mit Platzhaltern. Geprüfte Vorlagen stehen verbundenen KIs auch als Prompt zur Verfügung.</p>
  </header>

  {#if items.length >= SEARCH_FROM}
    <div class="relative mx-4 mb-2">
      <Search size={13} class="absolute top-2 left-2 text-faint" />
      <input class="field !h-7.5 w-full !pl-7 text-xs" placeholder="Vorlage suchen" aria-label="Vorlage suchen" bind:value={query} />
    </div>
  {/if}

  <div class="flex-1 overflow-y-auto px-2 pb-3">
    {#each shown as t (t.node.id)}
      <button class="flex w-full items-center gap-2.5 rounded-lg px-2 py-2 text-left transition-colors hover:bg-elevated" animate:flip={{ duration: 180 }}
        onclick={() => ws.select(t.node.id)}>
        <div class="min-w-0 flex-1">
          <div class="truncate font-medium">{t.node.title}</div>
          <div class="truncate text-xs text-faint">
            {[t.projects.length ? t.projects.map((p) => p.name).join(', ') : 'Überall verfügbar', ...(t.arguments.length ? [t.arguments.join(', ')] : [])].join(' · ')}
          </div>
        </div>
        {#if !t.reviewed}
          <span class="chip shrink-0 gap-1 !border-ai/30 !bg-ai/10 !text-ai" title="Von einer KI geschrieben. Wird erst nach deiner Prüfung als Prompt angeboten."><Bot size={11} /> ungeprüft</span>
        {/if}
      </button>
    {:else}
      {#if query.trim()}
        <p class="px-2 py-6 text-center text-xs text-faint">Keine Vorlage gefunden.</p>
      {:else}
      <div class="flex flex-col items-center px-8 pt-16 text-center" in:pop={{ duration: 240, y: 8 }}>
        <div class="mb-3 flex h-11 w-11 items-center justify-center text-faint"><BookMarked size={20} /></div>
        <div class="font-medium">Noch keine Vorlagen</div>
        <p class="mt-1 text-xs leading-relaxed text-muted">Zum Beispiel eine Code-Review-Checkliste oder dein Prompt für ein Projekt.</p>
        <button class="btn-primary mt-4 !h-8 text-xs" onclick={() => (ws.templateWizard = true)}>Erste Vorlage anlegen</button>
      </div>
      {/if}
    {/each}
  </div>
</aside>
