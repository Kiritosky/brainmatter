<script lang="ts">
  import { plural, t } from '#lib/i18n.ts';
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

  
  async function confirm() {
    busy = true;
    try {
      const subtree = ws.subtreeOf(project.id);
      await api.deleteProject(project.id, withEntries);
      if (ws.view && subtree.has(ws.view)) await ws.setView(null);
      ws.notify(t(`„${project.name}“ gelöscht. Rückgängig über die Aktivität.`, `Deleted “${project.name}”. Undo it from the activity.`));
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
  <div class="surface w-[440px] rounded-xl p-5" role="dialog" aria-label={t('Projekt löschen', 'Delete project')} tabindex="-1" transition:pop onclick={(e) => e.stopPropagation()}>
    <div class="mb-3 flex items-center gap-3">
      <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-danger/12 text-danger"><Trash2 size={17} /></div>
      <h2 class="min-w-0 truncate text-[15px] font-semibold tracking-tight">{t(`„${project.name}“ löschen?`, `Delete “${project.name}”?`)}</h2>
    </div>

    {#if preview}
      <ul class="flex list-disc flex-col gap-1 pl-5 text-muted marker:text-faint">
        {#if preview.subprojects}
          <li>{plural(preview.subprojects, ['Unterprojekt wird', 'Unterprojekte werden'], ['subproject is', 'subprojects are'])} {t('mit gelöscht.', 'deleted with it.')}</li>
        {/if}
        {#if preview.documents}
          <li>{plural(preview.documents, ['Doku-Seite oder Vorlage geht', 'Doku-Seiten und Vorlagen gehen'], ['documentation page or template goes', 'documentation pages and templates go'])} {t('in den Papierkorb.', 'to the trash.')}</li>
        {/if}
        {#if preview.entries_elsewhere}
          <li>{plural(preview.entries_elsewhere, ['Eintrag bleibt', 'Einträge bleiben'], ['entry stays', 'entries stay'])} {t('in den anderen Projekten.', 'in the other projects.')}</li>
        {/if}
        <li>{t('Du kannst das über die Aktivität rückgängig machen.', 'You can undo this from the activity.')}</li>
      </ul>

      {#if preview.entries_only_here}
        <div class="card mt-4">
          <div class="row">
            <div class="min-w-0 flex-1">
              <div>{t('Einträge auch in den Papierkorb', 'Move entries to the trash too')}</div>
              <div class="text-xs text-faint">
                {plural(preview.entries_only_here, ['Eintrag ist', 'Einträge sind'], ['entry is', 'entries are'])}
                {preview.entries_only_here === 1
                  ? t('nur hier. Sonst bleibt er unter „Alles“.', 'only here. Otherwise it stays under “All”.')
                  : t('nur hier. Sonst bleiben sie unter „Alles“.', 'only here. Otherwise they stay under “All”.')}
              </div>
            </div>
            <Toggle bind:checked={withEntries} label={t('Einträge auch in den Papierkorb', 'Move entries to the trash too')} />
          </div>
        </div>
      {/if}
    {/if}

    <div class="mt-5 flex justify-end gap-2">
      <button class="btn-ghost" onclick={onclose}>{t('Abbrechen', 'Cancel')}</button>
      <button class="btn-danger" disabled={busy || !preview} onclick={confirm}>{t('Projekt löschen', 'Delete project')}</button>
    </div>
  </div>
</div>
