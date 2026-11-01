<script lang="ts">
  import { flip } from 'svelte/animate';
  import { BookMarked, Bot, Plus, X } from '@lucide/svelte';
  import { api, errorMessage, TEMPLATE_KIND, type TemplateInfo } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';

  const STARTER = 'Beschreibe hier, was immer gleich bleibt.\n\nWas sich ändert, wird zum Platzhalter: {{thema}}';

  let items = $state.raw<TemplateInfo[]>([]);

  $effect(() => {
    void ws.revision;
    api.templates().then((found) => (items = found)).catch(() => {});
  });

  async function create() {
    try {
      const node = await api.createNode({ title: 'Neue Vorlage', body: STARTER, kind: TEMPLATE_KIND });
      await ws.select(node.id);
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }
</script>

<aside class="flex h-full w-100 flex-col border-l border-line bg-panel">
  <header class="px-4 pt-3 pb-3" data-tauri-drag-region>
    <div class="flex items-center gap-1">
      <h2 class="flex-1 text-[15px] font-semibold tracking-tight">Bibliothek</h2>
      <button class="btn-ghost !h-7 text-xs" onclick={create}><Plus size={13} /> Neue Vorlage</button>
      <button class="icon-btn -mr-1" aria-label="Schließen" onclick={() => (ws.panel = null)}><X size={15} /></button>
    </div>
    <p class="mt-1.5 text-xs leading-relaxed text-muted">Wiederverwendbare Texte mit Platzhaltern. Geprüfte Vorlagen stehen verbundenen KIs auch als Prompt zur Verfügung.</p>
  </header>

  <div class="flex-1 overflow-y-auto px-2 pb-3">
    {#each items as t (t.node.id)}
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
      <div class="flex flex-col items-center px-8 pt-16 text-center" in:pop={{ duration: 240, y: 8 }}>
        <div class="mb-3 flex h-11 w-11 items-center justify-center text-faint"><BookMarked size={20} /></div>
        <div class="font-medium">Noch keine Vorlagen</div>
        <p class="mt-1 text-xs leading-relaxed text-muted">Zum Beispiel eine Code-Review-Checkliste oder dein Prompt für ein Projekt. Mit <code class="rounded bg-raised px-1 font-mono">{'{{name}}'}</code> markierst du, was sich ändert.</p>
      </div>
    {/each}
  </div>
</aside>
