<script lang="ts">
  import { t } from '#lib/i18n.ts';
  import { onMount, tick, untrack } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { Check, CornerDownLeft, Fingerprint, FolderOpen, Lock, Zap } from '@lucide/svelte';
  import { api, errorMessage, isApiError, onCaptureOpen, onSession, type Id, type ProjectSummary, type Status } from '#lib/api.ts';
  import { pop } from '#lib/ui/motion.ts';

  let { status: initial }: { status: Status } = $props();

  let status = $state(untrack(() => initial));
  let text = $state('');
  let saved = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  // A Touch ID prompt takes the focus; that must not close this window.
  let unlocking = $state(false);
  let field: HTMLTextAreaElement | undefined = $state();

  const locked = $derived(status.state !== 'unlocked');

  // Where it goes. Remembered, because captures tend to come in runs for one topic.
  const PROJECT_KEY = 'bm.captureProject';
  let projects = $state.raw<ProjectSummary[]>([]);
  let project = $state<Id | null>(localStorage.getItem(PROJECT_KEY));
  let picking = $state(false);
  let pickerQuery = $state('');
  let pickerIndex = $state(0);
  let pickerField: HTMLInputElement | undefined = $state();

  const projectName = $derived(projects.find((p) => p.project.id === project)?.project.name ?? null);
  const pickable = $derived.by(() => {
    const query = pickerQuery.trim().toLowerCase();
    const found = projects.map((p) => ({ id: p.project.id as Id | null, name: p.project.name })).filter((p) => p.name.toLowerCase().includes(query));
    return query ? found : [{ id: null, name: t('Ohne Projekt', 'No project') }, ...found];
  });

  async function openPicker() {
    picking = true;
    pickerQuery = '';
    pickerIndex = 0;
    await tick();
    pickerField?.focus();
  }

  async function pick(id: Id | null) {
    project = id;
    if (id) localStorage.setItem(PROJECT_KEY, id);
    else localStorage.removeItem(PROJECT_KEY);
    picking = false;
    await tick();
    field?.focus();
  }

  async function reset() {
    text = '';
    saved = false;
    error = null;
    picking = false;
    status = await api.status();
    await loadProjects();
    await tick();
    field?.focus();
  }

  // On every opening and after unlocking here, since nothing can be read before.
  async function loadProjects() {
    if (status.state !== 'unlocked') return;
    try {
      projects = await api.projects();
      // Its project may have been deleted since.
      if (project && !projects.some((p) => p.project.id === project)) project = null;
    } catch {
      // Without the list the entry still saves, in the project already chosen.
    }
  }

  async function close() {
    text = '';
    await api.hideCapture();
  }

  /** The first line is the title; the rest is the text of the entry. */
  async function submit() {
    const [first, ...rest] = text.split('\n');
    const title = first.trim();
    if (!title || saving) return;
    saving = true;
    error = null;
    try {
      await api.createNode({ title, body: rest.join('\n').trim(), projects: project ? [project] : [] });
      saved = true;
      setTimeout(close, 450);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = false;
    }
  }

  async function unlock() {
    unlocking = true;
    error = null;
    try {
      await api.unlockTouchId();
    } catch (e) {
      // The prompt's password button: the password is typed in the main window.
      if (isApiError(e) && e.code === 'touch_id_fallback') {
        api.showMainWindow();
        return;
      }
      error = errorMessage(e);
    } finally {
      unlocking = false;
    }
    await tick();
    field?.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    if (picking) {
      if (e.key === 'Escape') pick(project);
      else if (e.key === 'ArrowDown') pickerIndex = Math.min(pickerIndex + 1, pickable.length - 1);
      else if (e.key === 'ArrowUp') pickerIndex = Math.max(pickerIndex - 1, 0);
      else if (e.key === 'Enter' && pickable[pickerIndex]) pick(pickable[pickerIndex].id);
      else return;
      e.preventDefault();
      return;
    }
    if (e.key === 'Escape') close();
    else if ((e.metaKey || e.ctrlKey) && e.key === 'p' && !locked) {
      e.preventDefault();
      openPicker();
    } else if (e.key === 'Enter' && !e.shiftKey && !locked) {
      e.preventDefault();
      submit();
    }
  }

  onMount(() => {
    const cleanups = [
      onCaptureOpen(reset),
      onSession((s) => {
        status = s;
        loadProjects();
      }),
      // Clicking elsewhere dismisses it, like Spotlight. Not in a plain browser.
      document.documentElement.classList.contains('tauri')
        ? getCurrentWindow().onFocusChanged(({ payload: focused }) => {
            if (!focused && !unlocking) close();
          })
        : Promise.resolve(() => {}),
    ];
    reset();
    return () => cleanups.forEach((c) => c.then((off) => off()));
  });
</script>

<svelte:window {onkeydown} />

<div class="flex h-full items-start justify-center p-2">
  <div class="surface flex h-full w-full flex-col overflow-hidden rounded-xl" role="dialog" aria-label="Quick Capture" in:pop={{ duration: 160, y: -6 }}>
    {#if locked}
      <div class="flex flex-1 items-center gap-4 px-5">
        <div class="flex h-11 w-11 shrink-0 items-center justify-center text-faint"><Lock size={20} /></div>
        <div class="min-w-0 flex-1">
          <div class="font-medium">{t('Brainmatter ist gesperrt', 'Brainmatter is locked')}</div>
          <div class="truncate text-xs {error ? 'text-danger' : 'text-faint'}">{error ?? t('Zum Festhalten erst entsperren.', 'Unlock first to capture.')}</div>
        </div>
        {#if status.touch_id_enrolled}
          <button class="btn-primary shrink-0" disabled={unlocking} onclick={unlock}><Fingerprint size={15} /> Touch ID</button>
        {:else}
          <button class="btn-primary shrink-0" onclick={() => api.showMainWindow()}>{t('Öffnen', 'Open')}</button>
        {/if}
      </div>
    {:else if saved}
      <div class="flex flex-1 items-center justify-center gap-2.5 font-medium text-ok" in:pop={{ duration: 180, y: 0 }}>
        <span class="flex h-6 w-6 items-center justify-center rounded-full bg-ok/15"><Check size={14} /></span> {t('Festgehalten', 'Captured')}
      </div>
    {:else if picking}
      <!-- In place of the text, because a popup could not leave this small window. -->
      <div class="flex min-h-0 flex-1 flex-col" in:pop={{ duration: 120, y: 2 }}>
        <input class="w-full shrink-0 border-b border-line-soft bg-transparent px-4 py-2.5 outline-none placeholder:text-faint" placeholder={t('Projekt suchen …', 'Search projects …')} aria-label={t('Projekt suchen', 'Search projects')}
          bind:this={pickerField} bind:value={pickerQuery} oninput={() => (pickerIndex = 0)} />
        <div class="min-h-0 flex-1 overflow-y-auto p-1" role="listbox" aria-label={t('Projekte', 'Projects')}>
          {#each pickable as option, i (option.id ?? 'none')}
            <button class="flex w-full items-center gap-2 rounded-md px-3 py-1 text-left transition-colors {i === pickerIndex ? 'bg-text/8 text-text' : 'text-muted'}"
              role="option" aria-selected={i === pickerIndex} onmouseenter={() => (pickerIndex = i)} onclick={() => pick(option.id)}>
              <span class="truncate">{option.name}</span>
              {#if option.id === project}<Check size={13} class="ml-auto shrink-0 text-accent" />{/if}
            </button>
          {:else}
            <div class="px-3 py-1 text-faint">{t('Kein Projekt gefunden', 'No project found')}</div>
          {/each}
        </div>
      </div>
    {:else}
      <div class="flex min-h-0 flex-1 gap-3 px-4 pt-3.5">
        <Zap size={16} class="mt-0.5 shrink-0 text-accent" />
        <textarea
          class="h-full min-w-0 flex-1 resize-none bg-transparent text-[15px] leading-snug outline-none placeholder:text-faint"
          placeholder={t('Was willst du festhalten?', 'What do you want to capture?')}
          aria-label={t('Neuer Eintrag', 'New entry')}
          bind:this={field}
          bind:value={text}></textarea>
      </div>
      <footer class="flex items-center gap-4 border-t border-line-soft px-4 py-2 text-[11px] text-faint">
        {#if error}
          <span class="truncate text-danger">{error}</span>
        {:else}
          <button class="flex min-w-0 items-center gap-1.5 rounded transition-colors hover:text-text" title={t('Projekt wählen', 'Choose project')} onclick={openPicker}>
            <FolderOpen size={12} class="shrink-0" /> <span class="max-w-40 truncate">{projectName ?? t('Ohne Projekt', 'No project')}</span> <span class="kbd shrink-0">⌘P</span>
          </button>
        {/if}
        <span class="ml-auto flex items-center gap-1.5"><span class="kbd">⇧↵</span> {t('Neue Zeile', 'New line')}</span>
        <span class="flex items-center gap-1.5"><span class="kbd"><CornerDownLeft size={10} /></span> {t('Sichern', 'Save')}</span>
        <span class="flex items-center gap-1.5"><span class="kbd">esc</span> {t('Schließen', 'Close')}</span>
      </footer>
    {/if}
  </div>
</div>
