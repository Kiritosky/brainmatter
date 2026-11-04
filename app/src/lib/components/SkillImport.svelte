<script lang="ts">
  import { num, plural, t } from '#lib/i18n.ts';
  import { ArrowLeft, Check, ChevronRight, CircleAlert, CircleCheck, FolderGit2, LoaderCircle, X } from '@lucide/svelte';
  import { api, errorMessage, type Id, type SkillImportPreview, type SkillInfo } from '#lib/api.ts';
  import { formatSize } from '#lib/attachments.ts';
  import { filesLabel, parseSkillAddress, shortRevision } from '#lib/skills.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { fadeQuick, pop } from '#lib/ui/motion.ts';
  import ProjectPicker from '#lib/ui/ProjectPicker.svelte';
  import Segmented from '#lib/ui/Segmented.svelte';
  import Toggle from '#lib/ui/Toggle.svelte';

  type Step = 'address' | 'choose' | 'done';
  let step = $state<Step>('address');
  let url = $state('');
  let loading = $state(false);
  let importing = $state(false);
  let error = $state<string | null>(null);
  let preview = $state.raw<SkillImportPreview | null>(null);
  let picked = $state.raw(new Set<string>());
  let everywhere = $state<'all' | 'some'>('all');
  let chosen = $state.raw<Id[]>([]);
  let result = $state.raw<{ skills: SkillInfo[]; updated: number }>({ skills: [], updated: 0 });
  /** Names of the skills here, for those an import would update. */
  let existingNames = $state.raw(new Map<Id, string>());
  /** Answers to an address typed over since are dropped. */
  let asked = 0;

  const address = $derived(parseSkillAddress(url));
  const candidates = $derived(preview?.skills ?? []);
  const anyNew = $derived(candidates.some((c) => picked.has(c.path) && !c.existing));
  const canImport = $derived(picked.size > 0 && !importing && (!anyNew || everywhere === 'all' || chosen.length > 0));

  async function load(e?: SubmitEvent) {
    e?.preventDefault();
    if (!url.trim() || loading) return;
    const mine = ++asked;
    loading = true;
    error = null;
    try {
      const [found, skills] = await Promise.all([api.previewSkillImport(url.trim()), api.skills()]);
      if (mine !== asked) return;
      existingNames = new Map(skills.map((s) => [s.node.id, s.node.title]));
      preview = found;
      picked = new Set(found.skills.map((s) => s.path));
      step = 'choose';
    } catch (err) {
      if (mine === asked) error = errorMessage(err);
    } finally {
      if (mine === asked) loading = false;
    }
  }

  function toggle(path: string, on: boolean) {
    const next = new Set(picked);
    if (on) next.add(path);
    else next.delete(path);
    picked = next;
  }

  async function run() {
    if (!preview || !canImport) return;
    importing = true;
    error = null;
    try {
      const paths = candidates.filter((c) => picked.has(c.path)).map((c) => c.path);
      const skills = await api.importSkills(preview.url, preview.revision, paths, everywhere === 'some' ? chosen : []);
      result = { skills, updated: candidates.filter((c) => picked.has(c.path) && c.existing).length };
      step = 'done';
      ws.panel = 'library';
      ws.libraryTab = 'skills';
    } catch (err) {
      error = errorMessage(err);
    } finally {
      importing = false;
    }
  }

  function back() {
    step = 'address';
    error = null;
  }

  const close = () => (ws.skillImport = false);

  async function open(id: Id) {
    close();
    await ws.select(id);
  }

  function onkeydowncapture(e: KeyboardEvent) {
    if (e.key !== 'Escape' || (e.target as HTMLElement).closest('[role="listbox"]')) return;
    e.stopPropagation();
    close();
  }

  const projectName = (id: Id) => ws.projects.find((p) => p.project.id === id)?.project.name ?? '';
</script>

<svelte:window {onkeydowncapture} />

<div class="fixed inset-0 z-[55] flex items-center justify-center bg-black/45 p-10 backdrop-blur-[2px]" role="presentation" transition:fadeQuick onclick={close}>
  <!-- svelte-ignore a11y_click_events_have_key_events -- the window handles Esc -->
  <div class="surface flex h-[640px] max-h-full w-[600px] flex-col overflow-hidden rounded-xl" role="dialog" aria-label={t('Skills von GitHub importieren', 'Import skills from GitHub')} tabindex="-1" transition:pop onclick={(e) => e.stopPropagation()}>
    <header class="flex h-12 shrink-0 items-center gap-3 border-b border-line-soft pr-3 pl-5">
      <FolderGit2 size={16} class="shrink-0 text-faint" />
      <h2 class="flex-1 text-[15px] font-semibold tracking-tight">{t('Skills von GitHub importieren', 'Import skills from GitHub')}</h2>
      <button class="icon-btn" aria-label={t('Schließen', 'Close')} onclick={close}><X size={15} /></button>
    </header>

    <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      {#if step === 'address'}
        <h3 class="font-medium">{t('Wo liegen die Skills?', 'Where are the skills?')}</h3>
        <p class="mt-0.5 mb-3 text-xs leading-relaxed text-muted">{t('Ein öffentliches Repo, ein Ordner darin oder eine einzelne SKILL.md. Zuerst holt Brainmatter nur die Beschreibungen, damit du auswählen kannst.', 'A public repo, a folder in it, or a single SKILL.md. Brainmatter first fetches only the descriptions, so that you can choose.')}</p>
        <form class="flex gap-1.5" onsubmit={load}>
          <!-- svelte-ignore a11y_autofocus -- the address is all this step asks for -->
          <input class="field min-w-0 flex-1 {error ? '!border-danger/60' : ''}" placeholder="https://github.com/owner/repo" aria-label={t('GitHub-Adresse', 'GitHub address')} autofocus spellcheck="false" autocomplete="off"
            bind:value={url} oninput={() => (error = null)} />
        </form>
        <div class="mt-2 flex h-5 items-center gap-1 text-xs text-faint">
          {#if address}
            <span class="flex min-w-0 items-center gap-1" in:fadeQuick>
              <FolderGit2 size={12} class="shrink-0" /><span class="truncate font-medium text-muted">{address.repo}</span>
              {#each address.path.split('/').filter(Boolean) as part, i (i)}<ChevronRight size={11} class="shrink-0" /><span class="truncate">{part}</span>{/each}
              {#if address.ref}<span class="ml-1 shrink-0 font-mono text-[11px]">@{address.ref.length > 12 ? shortRevision(address.ref) : address.ref}</span>{/if}
            </span>
          {/if}
        </div>

        {#if error}
          <div class="mt-1 flex gap-2 rounded-lg border border-danger/25 bg-danger/8 px-3 py-2.5 text-xs leading-relaxed text-danger" role="alert" in:pop={{ duration: 160, y: -3 }}>
            <CircleAlert size={14} class="mt-px shrink-0" /><span>{error}</span>
          </div>
        {:else if loading}
          <div class="mt-1 flex flex-col gap-2" aria-busy="true" in:fadeQuick>
            <p class="flex items-center gap-1.5 text-xs text-muted"><LoaderCircle size={13} class="animate-spin" /> {t(`Sieht in ${address?.repo ?? 'dem Repo'} nach …`, `Looking in ${address?.repo ?? 'the repo'} …`)}</p>
            {#each [0, 1, 2] as i (i)}
              <div class="rounded-lg border border-line-soft p-3" style="opacity: {1 - i * 0.25}">
                <div class="h-3 w-32 animate-pulse rounded bg-text/8"></div>
                <div class="mt-2 h-2.5 w-full animate-pulse rounded bg-text/5"></div>
                <div class="mt-1.5 h-2.5 w-2/3 animate-pulse rounded bg-text/5"></div>
              </div>
            {/each}
          </div>
        {/if}
      {:else if step === 'choose' && preview}
        <div class="mb-3 flex items-center gap-2">
          <FolderGit2 size={14} class="shrink-0 text-faint" />
          <span class="min-w-0 truncate font-medium">{preview.repo}</span>
          <span class="shrink-0 font-mono text-[11px] text-faint" title={preview.revision}>{shortRevision(preview.revision)}</span>
          <span class="ml-auto shrink-0 text-xs text-faint">{t(`${num(picked.size)} von ${plural(candidates.length, ['Skill', 'Skills'], ['skill', 'skills'])}`, `${num(picked.size)} of ${plural(candidates.length, ['Skill', 'Skills'], ['skill', 'skills'])}`)}</span>
        </div>
        <div class="flex flex-col gap-1.5">
          {#each candidates as c (c.path)}
            {@const on = picked.has(c.path)}
            <div class="flex gap-3 rounded-lg border p-3 transition-colors {on ? 'border-line bg-elevated/40' : 'border-line-soft'}" in:pop={{ duration: 180, y: 4 }}>
              <div class="min-w-0 flex-1 transition-opacity {on ? '' : 'opacity-60'}">
                <div class="flex items-center gap-2">
                  <span class="truncate font-medium">{c.name}</span>
                  {#if c.existing}
                    <span class="chip !h-5 shrink-0 !border-accent/30 !bg-accent/10 !px-1.5 text-[11px] !text-accent">{t(`aktualisiert „${existingNames.get(c.existing) ?? c.name}“`, `updates “${existingNames.get(c.existing) ?? c.name}”`)}</span>
                  {:else}
                    <span class="chip !h-5 shrink-0 !border-ok/30 !bg-ok/10 !px-1.5 text-[11px] !text-ok">{t('neu', 'new')}</span>
                  {/if}
                </div>
                <p class="mt-1 line-clamp-2 text-xs leading-relaxed text-muted">{c.description}</p>
                <p class="mt-1.5 truncate text-[11px] text-faint"><span class="font-mono">{c.path || '/'}</span> · {filesLabel(c.files.length)} · {formatSize(c.size)}</p>
              </div>
              <div class="pt-0.5"><Toggle checked={on} label={t(`${c.name} importieren`, `Import ${c.name}`)} onchange={(v) => toggle(c.path, v)} /></div>
            </div>
          {/each}
        </div>

        {#if anyNew}
          <div class="mt-5" in:pop={{ duration: 160, y: 4 }}>
            <h3 class="font-medium">{t('Wo sollen neue Skills verfügbar sein?', 'Where should new skills be available?')}</h3>
            <p class="mt-0.5 mb-3 text-xs leading-relaxed text-muted">{t('Skills, die es schon gibt, behalten ihre Projekte.', 'Skills that are here already keep their projects.')}</p>
            <div class="mb-3 w-72">
              <Segmented bind:value={everywhere} options={[{ value: 'all', label: t('Überall', 'Everywhere') }, { value: 'some', label: t('In Projekten', 'In projects') }]} label={t('Verfügbarkeit', 'Availability')} />
            </div>
            {#if everywhere === 'some'}
              <div in:pop={{ duration: 140, y: -3 }}>
                {#if chosen.length}
                  <div class="mb-2 flex flex-wrap gap-1">
                    {#each chosen as id (id)}
                      <span class="chip">
                        <span class="h-2 w-2 rounded-full" style="background: {ws.projectColor.get(id)}"></span>{projectName(id)}
                        <button class="-mr-0.5 text-faint transition-colors hover:text-danger" aria-label={t(`${projectName(id)} entfernen`, `Remove ${projectName(id)}`)} onclick={() => (chosen = chosen.filter((c) => c !== id))}><X size={11} /></button>
                      </span>
                    {/each}
                  </div>
                {/if}
                <div class="rounded-lg border border-line-soft p-1">
                  <ProjectPicker exclude={(id) => chosen.includes(id)} onpick={(id) => id && (chosen = [...chosen, id])} />
                </div>
              </div>
            {/if}
          </div>
        {/if}

        {#if error}
          <div class="mt-4 flex gap-2 rounded-lg border border-danger/25 bg-danger/8 px-3 py-2.5 text-xs leading-relaxed text-danger" role="alert" in:pop={{ duration: 160, y: -3 }}>
            <CircleAlert size={14} class="mt-px shrink-0" /><span>{error}</span>
          </div>
        {/if}
      {:else if step === 'done'}
        {@const added = result.skills.length - result.updated}
        <div class="flex flex-col items-center px-8 pt-10 text-center" in:pop={{ duration: 240, y: 8 }}>
          <div class="mb-3 flex h-11 w-11 items-center justify-center rounded-xl bg-ok/12 text-ok"><CircleCheck size={20} /></div>
          <div class="font-medium">{plural(result.skills.length, ['Skill importiert', 'Skills importiert'], ['skill imported', 'skills imported'])}</div>
          <p class="mt-1 max-w-sm text-xs leading-relaxed text-muted">
            {[added ? t(`${num(added)} neu`, `${num(added)} new`) : '', result.updated ? t(`${num(result.updated)} aktualisiert`, `${num(result.updated)} updated`) : ''].filter(Boolean).join(', ')}.
            {t('Sie gelten als geprüft, verbundene KIs können sie gleich nutzen.', 'They count as reviewed, so connected AI clients can use them right away.')}
          </p>
          <div class="mt-5 flex w-full max-w-sm flex-col gap-1">
            {#each result.skills as skill (skill.node.id)}
              <button class="flex h-9 w-full items-center gap-2 rounded-lg px-3 text-left transition-colors hover:bg-elevated" onclick={() => open(skill.node.id)}>
                <span class="min-w-0 flex-1 truncate font-medium">{skill.node.title}</span>
                <span class="shrink-0 text-[11px] text-faint">{filesLabel(skill.files.length)}</span>
                <ChevronRight size={13} class="shrink-0 text-faint" />
              </button>
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <footer class="flex h-13 shrink-0 items-center gap-2 border-t border-line-soft px-5">
      {#if step === 'choose'}
        <button class="btn-ghost !h-8 text-xs" onclick={back}><ArrowLeft size={13} /> {t('Andere Adresse', 'Other address')}</button>
      {/if}
      <div class="flex-1"></div>
      {#if step === 'address'}
        <button class="btn-primary !h-8 text-xs" disabled={!url.trim() || loading} onclick={() => load()}>
          {#if loading}<LoaderCircle size={13} class="animate-spin" />{/if} {t('Vorschau laden', 'Load preview')}
        </button>
      {:else if step === 'choose'}
        <button class="btn-primary !h-8 text-xs" disabled={!canImport} onclick={run}>
          {#if importing}<LoaderCircle size={13} class="animate-spin" />{:else}<Check size={13} />{/if}
          {picked.size === 1 ? t('Einen Skill importieren', 'Import one skill') : t(`${num(picked.size)} Skills importieren`, `Import ${num(picked.size)} skills`)}
        </button>
      {:else}
        <button class="btn-primary !h-8 text-xs" onclick={close}>{t('Fertig', 'Done')}</button>
      {/if}
    </footer>
  </div>
</div>
