<script lang="ts">
  import { t } from '#lib/i18n.ts';
  import { ArrowLeft, ClipboardCopy, Eye, EyeOff, Focus, FolderCode, FolderInput, FolderPlus, FolderX, Pencil, Trash2 } from '@lucide/svelte';
  import { api, errorMessage, type Id, type Project } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';
  import ProjectPicker from '#lib/ui/ProjectPicker.svelte';

  let {
    project,
    x,
    y,
    onclose,
    onrename,
    onsubproject,
    onmove,
    ondelete,
  }: {
    project: Project;
    x: number;
    y: number;
    onclose: () => void;
    onrename: () => void;
    onsubproject: () => void;
    onmove: (parent: Id | null) => void;
    ondelete: () => void;
  } = $props();

  const WIDTH = 216;
  const MARGIN = 8;

  let moving = $state(false);
  let panel: HTMLDivElement | undefined = $state();
  let height = $state(0);

  // Everything but itself, what is below it, and where it already is.
  const targets = $derived.by(() => {
    const own = ws.subtreeOf(project.id);
    return ws.projects.filter((p) => !own.has(p.project.id) && p.project.id !== project.parent_id);
  });

  const folder = $derived(ws.projects.find((p) => p.project.id === project.id)?.folder ?? null);
  const folderName = $derived(folder?.split('/').filter(Boolean).pop() ?? '');

  // Closing the menu lets go of `project`, so these keep what they need first.
  async function chooseFolder() {
    const { id, name } = project;
    onclose();
    try {
      const picked = await api.chooseProjectFolder(id);
      if (!picked) return;
      await ws.loadProjects();
      ws.upkeep++;
      ws.notify(t(`Pfade in „${name}“ beziehen sich jetzt auf ${picked}.`, `Paths in “${name}” now refer to ${picked}.`));
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  async function clearFolder() {
    const { id } = project;
    onclose();
    try {
      await api.clearProjectFolder(id);
      await ws.loadProjects();
      ws.upkeep++;
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  /** The project as Markdown, for chat apps that cannot reach Brainmatter themselves. */
  async function copyContext() {
    const { id, name } = project;
    onclose();
    try {
      const text = await api.projectContext(id);
      await navigator.clipboard.writeText(text);
      const words = text.split(/\s+/).filter(Boolean).length.toLocaleString();
      ws.notify(t(`Kontext von „${name}“ kopiert, rund ${words} Wörter.`, `Copied the context of “${name}”, about ${words} words.`));
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  const left = $derived(Math.min(x, window.innerWidth - WIDTH - MARGIN));
  const top = $derived(Math.max(MARGIN, Math.min(y, window.innerHeight - height - MARGIN)));

  function onkeydowncapture(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      if (moving) moving = false;
      else onclose();
    }
  }
</script>

<svelte:window {onkeydowncapture} />

<div class="fixed inset-0 z-40" role="presentation" onclick={onclose}></div>
<div class="surface fixed z-50 rounded-xl p-1" style="left: {left}px; top: {top}px; width: {WIDTH}px" role="menu" aria-label={t(`Projekt „${project.name}“`, `Project “${project.name}”`)} tabindex="-1"
  bind:this={panel} bind:clientHeight={height} transition:pop={{ duration: 130, y: -3 }}>
  {#if moving}
    <button class="menu-row text-muted" role="menuitem" onclick={() => (moving = false)}><ArrowLeft size={13} /> {t('Verschieben nach …', 'Move to …')}</button>
    <div class="my-1 h-px bg-line-soft"></div>
    <ProjectPicker exclude={(id) => !targets.some((target) => target.project.id === id)} top={project.parent_id ? t('Oberste Ebene', 'Top level') : undefined}
      onpick={onmove} onclose={() => (moving = false)} />
  {:else}
    <button class="menu-row" role="menuitem" onclick={onrename}><Pencil size={13} class="text-faint" /> {t('Umbenennen', 'Rename')}</button>
    <button class="menu-row" role="menuitem" onclick={onsubproject}><FolderPlus size={13} class="text-faint" /> {t('Unterprojekt anlegen', 'New subproject')}</button>
    <button class="menu-row" role="menuitem" onclick={() => (moving = true)}><FolderInput size={13} class="text-faint" /> {t('Verschieben nach …', 'Move to …')}</button>
    <div class="my-1 h-px bg-line-soft"></div>
    <button class="menu-row" role="menuitem" onclick={chooseFolder} title={folder ?? undefined}>
      <FolderCode size={13} class="text-faint" />
      {#if folder}<span class="min-w-0 flex-1 truncate">{t('Ordner:', 'Folder:')} <span class="text-muted">{folderName}</span></span>{:else}{t('Ordner festlegen …', 'Set folder …')}{/if}
    </button>
    {#if folder}
      <button class="menu-row" role="menuitem" onclick={clearFolder}><FolderX size={13} class="text-faint" /> {t('Ordner entfernen', 'Remove folder')}</button>
    {/if}
    <button class="menu-row" role="menuitem" onclick={copyContext}><ClipboardCopy size={13} class="text-faint" /> {t('Kontext kopieren', 'Copy context')}</button>
    <div class="my-1 h-px bg-line-soft"></div>
    <button class="menu-row" role="menuitem" onclick={() => (ws.toggleProjectHidden(project.id), onclose())}>
      {#if ws.hiddenProjects.has(project.id)}<Eye size={13} class="text-faint" /> {t('Einblenden', 'Show')}{:else}<EyeOff size={13} class="text-faint" /> {t('Ausblenden', 'Hide')}{/if}
    </button>
    <button class="menu-row" role="menuitem" onclick={() => (ws.showOnlyProject(project.id), onclose())}><Focus size={13} class="text-faint" /> {t('Nur dieses anzeigen', 'Show only this one')}</button>
    <div class="my-1 h-px bg-line-soft"></div>
    <button class="menu-row hover:!bg-danger/12 hover:!text-danger" role="menuitem" onclick={ondelete}><Trash2 size={13} /> {t('Löschen …', 'Delete …')}</button>
  {/if}
</div>

<style>
  .menu-row {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 8px;
    border-radius: 8px;
    padding: 6px 8px;
    text-align: left;
    transition: background-color 100ms;
  }
  .menu-row:hover {
    background: color-mix(in srgb, var(--color-text) 8%, transparent);
  }
</style>
