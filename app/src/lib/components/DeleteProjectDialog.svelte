<script lang="ts">
  import { onMount } from 'svelte';
  import { Trash2 } from '@lucide/svelte';
  import { api, errorMessage, type Project, type ProjectDeletePreview } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import Toggle from '#lib/ui/Toggle.svelte';
  import { fadeQuick, pop } from '#lib/ui/motion.ts';

  let { project, onclose }: { project: Project; onclose: () => void } = $props();

  let preview = $state<ProjectDeletePreview | null>(null);
  let withEntries = $state(false);
  let busy = $state(false);

  onMount(async () => {
    preview = await api.projectDeletePreview(project.id);
  });

  const count = (n: number, one: string, many: string) => (n === 1 ? `1 ${one}` : `${n} ${many}`);

  async function confirm() {
    busy = true;
    try {
      const subtree = ws.subtreeOf(project.id);
      await api.deleteProject(project.id, withEntries);
      if (ws.view && subtree.has(ws.view)) await ws.setView(null);
      ws.notify(`„${project.name}“ gelöscht. Rückgängig über die Aktivität.`);
      onclose();
    } catch (e) {
      ws.notify(errorMessage(e));
      busy = false;
    }
  }

  // Capture: Esc must close this dialog and nothing behind it.
  function onkeydowncapture(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onclose();
    }
  }
</script>

<svelte:window {onkeydowncapture} />

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/45 backdrop-blur-[2px]" role="presentation" transition:fadeQuick onclick={onclose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -- the window handles Esc -->
  <div class="surface w-[440px] rounded-2xl p-5" role="dialog" aria-label="Projekt löschen" tabindex="-1" transition:pop onclick={(e) => e.stopPropagation()}>
    <div class="mb-3 flex items-center gap-3">
      <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-danger/12 text-danger"><Trash2 size={17} /></div>
      <h2 class="min-w-0 truncate text-[15px] font-semibold tracking-tight">„{project.name}“ löschen?</h2>
    </div>

    {#if preview}
      <ul class="flex list-disc flex-col gap-1 pl-5 text-muted marker:text-faint">
        {#if preview.subprojects}
          <li>{count(preview.subprojects, 'Unterprojekt wird', 'Unterprojekte werden')} mit gelöscht.</li>
        {/if}
        {#if preview.documents}
          <li>{count(preview.documents, 'Doku-Seite oder Vorlage geht', 'Doku-Seiten und Vorlagen gehen')} in den Papierkorb.</li>
        {/if}
        {#if preview.entries_elsewhere}
          <li>{count(preview.entries_elsewhere, 'Eintrag bleibt', 'Einträge bleiben')} in den anderen Projekten.</li>
        {/if}
        <li>Du kannst das über die Aktivität rückgängig machen.</li>
      </ul>

      {#if preview.entries_only_here}
        <div class="card mt-4">
          <div class="row">
            <div class="min-w-0 flex-1">
              <div>Einträge auch in den Papierkorb</div>
              <div class="text-xs text-faint">
                {count(preview.entries_only_here, 'Eintrag ist', 'Einträge sind')} nur hier. Sonst {preview.entries_only_here === 1 ? 'bleibt er' : 'bleiben sie'} unter „Alles“.
              </div>
            </div>
            <Toggle bind:checked={withEntries} label="Einträge auch in den Papierkorb" />
          </div>
        </div>
      {/if}
    {/if}

    <div class="mt-5 flex justify-end gap-2">
      <button class="btn-ghost" onclick={onclose}>Abbrechen</button>
      <button class="btn-danger" disabled={busy || !preview} onclick={confirm}>Projekt löschen</button>
    </div>
  </div>
</div>
