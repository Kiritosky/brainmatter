<script lang="ts">
  import { locale, t } from '#lib/i18n.ts';
  import { onDestroy, untrack } from 'svelte';
  import { Bot, Check, CircleAlert, File as FileIcon, FileCode, FileText, FolderGit2, LoaderCircle, Plus, Puzzle, RefreshCw, Trash2, Upload, X } from '@lucide/svelte';
  import { api, errorMessage, type Attachment, type Id, type SkillCandidate, type SkillInfo } from '#lib/api.ts';
  import { formatSize } from '#lib/attachments.ts';
  import { MAX_SKILL_BYTES, MAX_SKILL_FILES, cleanFilePath, filesLabel, shortRevision, skillBytes, suggestFilePath } from '#lib/skills.ts';
  import ProjectPicker from '#lib/ui/ProjectPicker.svelte';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';
  import AttachmentPreview from './AttachmentPreview.svelte';
  import MarkdownEditor from './MarkdownEditor.svelte';

  /** The skill to edit; none while a new one is written. */
  let { id = null }: { id?: Id | null } = $props();
  const skillId = untrack(() => id);
  const creating = skillId === null;

  const SAVE_DELAY_MS = 900;

  let info = $state.raw<SkillInfo | null>(null);
  let title = $state('');
  let description = $state('');
  let body = $state('');
  let projectIds = $state.raw<Id[]>([]);
  let dirty = $state(false);
  let saving = $state(0);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let pickingProject = $state(false);
  let confirmDelete = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  // Writes go one after the other: a save sends the projects as they are by then,
  // so it never undoes a project change made just before it.
  let writes: Promise<unknown> = Promise.resolve();
  let queued = 0;
  function write(fn: () => Promise<unknown>) {
    queued++;
    const run = writes.then(fn).finally(() => queued--);
    writes = run.catch(() => {});
    return run;
  }

  const missing = $derived(!title.trim() ? 'title' : !description.trim() ? 'description' : null);

  $effect(() => {
    void ws.revision;
    if (!creating) untrack(load);
  });

  async function load() {
    const found = (await api.skills().catch(() => [])).find((s) => s.node.id === skillId);
    if (found) accept(found);
  }

  /** What the vault has now. It never replaces what is still being written. */
  function accept(next: SkillInfo) {
    info = next;
    if (dirty || saving || queued) return;
    // The vault trims; a space typed last must not vanish under the cursor.
    if (title.trim() !== next.node.title) title = next.node.title;
    if (description.trim() !== next.description) description = next.description;
    body = next.body;
    projectIds = next.projects.map((p) => p.id);
  }

  function changed() {
    dirty = true;
    error = null;
    clearTimeout(timer);
    if (!creating) timer = setTimeout(save, SAVE_DELAY_MS);
  }

  function save() {
    clearTimeout(timer);
    if (creating || !dirty || missing) return writes;
    dirty = false;
    const fields = { title, description, body };
    return write(async () => {
      saving++;
      try {
        info = await api.saveSkill({ id: skillId, ...fields, projects: projectIds });
      } catch (e) {
        dirty = true;
        error = errorMessage(e);
      } finally {
        saving--;
      }
    });
  }

  onDestroy(() => {
    clearTimeout(timer);
    if (creating) ws.newSkill = false;
    else if (dirty && missing) ws.notify(t('Nicht gespeichert: Ein Skill braucht Namen und Beschreibung.', 'Not saved: a skill needs a name and a description.'));
    else save();
  });

  const projectName = (pid: Id) => ws.projects.find((p) => p.project.id === pid)?.project.name ?? info?.projects.find((p) => p.id === pid)?.name ?? '';

  function setProject(pid: Id, member: boolean) {
    projectIds = member ? [...projectIds, pid] : projectIds.filter((p) => p !== pid);
    if (creating) return;
    write(() => (member ? api.addToProject(pid, [skillId!]) : api.removeFromProject(pid, [skillId!])).catch((e) => ws.notify(errorMessage(e))));
  }

  async function createProject(name: string) {
    try {
      const project = await api.createProject(name);
      await ws.loadProjects();
      setProject(project.id, true);
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  async function markReviewed() {
    try {
      await api.updateNode(skillId!, { reviewed: true });
      if (info) info = { ...info, reviewed: true };
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  async function remove() {
    clearTimeout(timer);
    dirty = false;
    try {
      await api.deleteNode(skillId!);
      await ws.select(null);
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }

  async function create() {
    if (missing || busy) return;
    busy = true;
    error = null;
    try {
      const saved = await api.saveSkill({ id: null, title, description, body, projects: projectIds });
      if (pending.length) await upload(saved.node.id);
      await ws.select(saved.node.id);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  // Files: picked or dropped, they wait here for their path in the folder.
  interface Pending {
    key: number;
    file: File;
    path: string;
  }
  let pending = $state<Pending[]>([]);
  let pendingKey = 0;
  let uploading = $state(false);
  let dragging = $state(false);
  let previewing = $state<Attachment | null>(null);
  let filePicker: HTMLInputElement | undefined = $state();
  let descriptionField: HTMLTextAreaElement | undefined = $state();

  // The description is read as a whole, so its field grows with it instead of scrolling.
  $effect(() => {
    void description;
    if (!descriptionField) return;
    descriptionField.style.height = 'auto';
    descriptionField.style.height = `${descriptionField.scrollHeight + 2}px`;
  });

  const files = $derived(info?.files ?? []);

  function queue(list: Iterable<File>) {
    pending = [...pending, ...[...list].map((file) => ({ key: ++pendingKey, file, path: suggestFilePath(file.name) }))];
  }

  const problems = $derived.by(() => {
    const seen = new Set(files.map((f) => f.filename.toLowerCase()));
    return pending.map((p) => {
      const clean = cleanFilePath(p.path);
      if (!clean) return t('Ein Pfad im Ordner des Skills, ohne „..“ und nicht SKILL.md.', 'A path inside the skill’s folder, without “..” and not SKILL.md.');
      if (seen.has(clean.toLowerCase())) return t('Diesen Pfad gibt es schon.', 'This path is taken.');
      seen.add(clean.toLowerCase());
      return null;
    });
  });

  const tooMuch = $derived.by(() => {
    if (files.length + pending.length > MAX_SKILL_FILES) return t(`Ein Skill hat höchstens ${MAX_SKILL_FILES} Dateien.`, `A skill has ${MAX_SKILL_FILES} files at most.`);
    const size = skillBytes(body, [...files.map((f) => f.size), ...pending.map((p) => p.file.size)]);
    return size > MAX_SKILL_BYTES ? t('Ein Skill darf mit allen Dateien höchstens 5 MB groß sein.', 'A skill can be 5 MB at most, all its files included.') : null;
  });

  const canUpload = $derived(pending.length > 0 && problems.every((p) => !p) && !tooMuch);

  async function upload(target: Id) {
    uploading = true;
    try {
      for (const p of [...pending]) {
        await api.addAttachment(target, new File([p.file], cleanFilePath(p.path)!, { type: p.file.type }));
        pending = pending.filter((o) => o.key !== p.key);
      }
    } catch (e) {
      ws.notify(errorMessage(e));
    } finally {
      uploading = false;
    }
  }

  function onPathKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !creating && canUpload) upload(skillId!);
  }

  const hasFiles = (e: DragEvent) => e.dataTransfer?.types.includes('Files') ?? false;

  function ondrop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    queue(e.dataTransfer?.files ?? []);
  }

  const iconFor = (path: string) => (/\.(py|sh|bash|js|mjs|ts|rb|pl|go|rs|swift)$/i.test(path) ? FileCode : /\.(md|txt|rst|csv|json|ya?ml)$/i.test(path) ? FileText : FileIcon);

  // Bringing an imported skill up to date with its repo.
  type Refresh =
    | { step: 'checking' | 'current' | 'updating' | 'done' }
    | { step: 'ready'; url: string; revision: string; candidate: SkillCandidate }
    | { step: 'failed'; message: string };
  let refresh = $state.raw<Refresh | null>(null);
  let readyBox: HTMLDivElement | undefined = $state();

  // The offer sits at the very bottom, likely out of sight.
  $effect(() => readyBox?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }));

  const sameAsHere = (c: SkillCandidate) =>
    !!info &&
    c.description === info.description &&
    c.body === info.body &&
    c.files.length === files.length &&
    c.files.every((f) => files.some((a) => a.filename === f.path && a.size === f.size));

  async function checkSource() {
    const source = info?.source;
    if (!source) return;
    refresh = { step: 'checking' };
    try {
      const preview = await api.previewSkillImport(source.url);
      const candidate = preview.skills.find((c) => c.path === source.path);
      if (!candidate) refresh = { step: 'failed', message: t('Im Repo gibt es diesen Skill nicht mehr.', 'This skill is no longer in the repo.') };
      else if (sameAsHere(candidate)) refresh = { step: 'current' };
      else refresh = { step: 'ready', url: preview.url, revision: preview.revision, candidate };
    } catch (e) {
      refresh = { step: 'failed', message: errorMessage(e) };
    }
  }

  async function applyUpdate(url: string, revision: string, path: string) {
    refresh = { step: 'updating' };
    try {
      await save();
      const [updated] = await api.importSkills(url, revision, [path], []);
      clearTimeout(timer);
      dirty = false;
      error = null;
      if (updated) accept(updated);
      refresh = { step: 'done' };
    } catch (e) {
      refresh = { step: 'failed', message: errorMessage(e) };
    }
  }

  const fmt = (ms: number) => new Date(ms).toLocaleString(locale, { day: '2-digit', month: '2-digit', hour: '2-digit', minute: '2-digit' });
  const day = (ms: number) => new Date(ms).toLocaleDateString(locale, { day: '2-digit', month: '2-digit', year: 'numeric' });

  untrack(() => {
    // Shown at once; the rest of what the vault knows follows with the first load.
    const detail = ws.detail;
    if (detail && detail.node.id === skillId) {
      title = detail.node.title;
      body = detail.node.body;
      description = (detail.node.props as { description?: string } | null)?.description ?? '';
      projectIds = detail.projects.map((p) => p.id);
    }
  });
</script>

<aside class="relative flex h-full flex-col border-l border-line bg-panel"
  aria-label={creating ? t('Neuer Skill', 'New skill') : 'Skill'}
  ondragenter={(e) => hasFiles(e) && (dragging = true)}
  ondragover={(e) => hasFiles(e) && e.preventDefault()}
  ondragleave={(e) => e.relatedTarget === null && (dragging = false)}
  {ondrop}>
  {#if dragging}
    <div class="pointer-events-none absolute inset-2 z-30 flex flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-accent/60 bg-panel/90 text-accent" transition:pop={{ duration: 140, y: 0 }}>
      <Upload size={22} />
      <span class="font-medium">{t('Zum Hinzufügen loslassen', 'Drop to add')}</span>
      <span class="text-xs text-faint">{t('Liegt dann im Ordner des Skills', 'It goes into the skill’s folder')}</span>
    </div>
  {/if}

  <header class="flex h-11 shrink-0 items-center gap-1 border-b border-line-soft px-3" data-tauri-drag-region>
    <span class="chip gap-1"><Puzzle size={11} /> {creating ? t('Neuer Skill', 'New skill') : 'Skill'}</span>
    {#if info && !info.reviewed}
      <button class="chip gap-1.5 !border-ai/30 !bg-ai/10 !text-ai transition-colors hover:!bg-ai/20"
        title={t('Von einer KI geschrieben. Klicken, um ihn als geprüft zu markieren; erst dann bekommen KIs ihn auch als Befehl.', 'Written by an AI client. Click to mark it as reviewed; only then do AI clients get it as a command too.')}
        onclick={markReviewed}>
        <Bot size={12} /> {t('ungeprüft', 'unreviewed')} <Check size={12} />
      </button>
    {/if}
    <div class="h-full flex-1" data-tauri-drag-region></div>
    {#if !creating}
      {#if confirmDelete}
        <button class="btn-danger !h-7" in:pop={{ duration: 120, y: 0 }} onclick={remove} onblur={() => (confirmDelete = false)}>{t('Löschen?', 'Delete?')}</button>
      {:else}
        <button class="icon-btn hover:!text-danger" aria-label={t('In den Papierkorb', 'Move to trash')} title={t('In den Papierkorb', 'Move to trash')} onclick={() => (confirmDelete = true)}><Trash2 size={15} /></button>
      {/if}
    {/if}
    <button class="icon-btn" aria-label={t('Schließen', 'Close')} title={t('Schließen (Esc)', 'Close (Esc)')} onclick={() => (creating ? (ws.newSkill = false) : ws.select(null))}><X size={15} /></button>
  </header>

  <div class="flex-1 overflow-y-auto px-5 pt-4 pb-6">
    <!-- svelte-ignore a11y_autofocus -- a new skill starts with its name -->
    <input
      class="w-full bg-transparent text-lg leading-snug font-semibold tracking-tight outline-none placeholder:text-faint"
      aria-label={t('Name des Skills', 'Skill name')}
      placeholder={t('Name des Skills', 'Skill name')}
      autofocus={creating}
      bind:value={title}
      oninput={changed}
      onblur={() => save()}
      onkeydown={(e) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur()} />
    {#if !creating && dirty && missing === 'title'}
      <p class="mt-0.5 text-xs text-warn" in:pop={{ duration: 120, y: -2 }}>{t('Ohne Namen wird nicht gespeichert.', 'Without a name, nothing is saved.')}</p>
    {/if}

    <div class="mt-2.5 mb-5 flex flex-wrap items-center gap-1">
      {#each projectIds as pid (pid)}
        <span class="chip">
          <span class="h-2 w-2 rounded-full" style="background: {ws.projectColor.get(pid)}"></span>{projectName(pid)}
          <button class="-mr-0.5 text-faint transition-colors hover:text-danger" aria-label={t(`Aus „${projectName(pid)}“ nehmen`, `Remove from “${projectName(pid)}”`)} onclick={() => setProject(pid, false)}><X size={11} /></button>
        </span>
      {:else}
        <span class="mr-1 text-xs text-faint">{t('Überall verfügbar', 'Available everywhere')}</span>
      {/each}
      <div class="relative">
        <button class="chip gap-1 !border-dashed !bg-transparent transition-colors hover:!border-faint hover:text-text" onclick={() => (pickingProject = !pickingProject)}>
          <Plus size={11} /> {t('Projekt', 'Project')}
        </button>
        {#if pickingProject}
          <div class="fixed inset-0 z-40" role="presentation" onclick={() => (pickingProject = false)}></div>
          <div class="surface absolute top-7 left-0 z-50 w-64 rounded-xl p-1" transition:pop={{ duration: 140, y: -4 }}>
            <ProjectPicker
              exclude={(pid) => projectIds.includes(pid)}
              onclose={() => (pickingProject = false)}
              onpick={(pid) => ((pickingProject = false), pid && setProject(pid, true))}
              oncreate={(name) => ((pickingProject = false), createProject(name))} />
          </div>
        {/if}
      </div>
    </div>

    <div class="section-title">{t('Beschreibung', 'Description')}</div>
    <textarea
      bind:this={descriptionField}
      class="field block w-full resize-none !py-2 text-[13px] leading-relaxed {!creating && dirty && missing === 'description' ? '!border-warn/60' : ''}"
      rows="3"
      aria-label={t('Beschreibung', 'Description')}
      placeholder={t('Was er tut und wann er passt, z. B. „Füllt PDF-Formulare aus. Nutzen, wenn jemand ein Formular ausgefüllt haben will.“', 'What it does and when it fits, e.g. “Fills in PDF forms. Use when someone wants a form filled in.”')}
      bind:value={description}
      oninput={changed}
      onblur={() => save()}
      onkeydown={(e) => e.key === 'Enter' && (e.preventDefault(), (e.currentTarget as HTMLTextAreaElement).blur())}></textarea>
    <p class="mt-1.5 flex gap-1.5 text-xs leading-relaxed {!creating && dirty && missing === 'description' ? 'text-warn' : 'text-faint'}">
      {#if !creating && dirty && missing === 'description'}
        {t('Ohne Beschreibung wird nicht gespeichert: Nach ihr wählt eine KI den Skill aus.', 'Without a description, nothing is saved: it is what an AI picks the skill by.')}
      {:else}
        {t('Nach diesem Text wählt eine KI den Skill aus. Er ist alles, was sie vorher von ihm sieht.', 'An AI picks the skill by this text. It is all it sees of the skill beforehand.')}
      {/if}
    </p>

    {#if error}
      <div class="mt-3 flex gap-2 rounded-lg border border-danger/25 bg-danger/8 px-3 py-2 text-xs leading-relaxed text-danger" role="alert" in:pop={{ duration: 140, y: -3 }}>
        <CircleAlert size={14} class="mt-px shrink-0" /><span>{error}</span>
      </div>
    {/if}

    <div class="section-title mt-6">{t('Anleitung', 'Instructions')}</div>
    <div class="rounded-lg border border-line-soft bg-bg/40 px-3 py-1">
      <MarkdownEditor value={body} titles={async () => []} onsave={() => (creating ? create() : save())} onchange={(text) => ((body = text), changed())} />
    </div>
    <p class="mt-1.5 text-xs leading-relaxed text-faint">{t('Das liest die KI, sobald sie den Skill lädt. Auf Dateien unten verweist du mit ihrem Pfad.', 'The AI reads this once it loads the skill. Point to the files below by their path.')}</p>

    <input class="hidden" type="file" multiple bind:this={filePicker} aria-hidden="true" tabindex="-1"
      onchange={(e) => {
        queue(e.currentTarget.files ?? []);
        e.currentTarget.value = '';
      }} />

    <div class="mt-6 mb-1 flex h-6 items-center">
      <div class="section-title !mb-0 flex-1">{t('Dateien', 'Files')}</div>
      {#if uploading}
        <span class="mr-1 flex items-center gap-1.5 text-[11px] text-faint" in:pop={{ duration: 120, y: 0 }}>
          <span class="h-1.5 w-1.5 animate-pulse rounded-full bg-accent"></span>{t('Verschlüsselt …', 'Encrypting …')}
        </span>
      {:else if files.length > 1}
        <span class="text-[11px] text-faint tabular-nums">{formatSize(files.reduce((sum, f) => sum + f.size, 0))}</span>
      {/if}
    </div>
    {#each files as f (f.id)}
      {@const Icon = iconFor(f.filename)}
      <div class="group flex h-8 items-center gap-2 rounded-lg px-2 transition-colors hover:bg-elevated" in:pop={{ duration: 160, y: 3 }}>
        <Icon size={14} class="shrink-0 text-faint" />
        <button class="min-w-0 flex-1 truncate text-left font-mono text-[12px]" title={f.filename} onclick={() => (previewing = f)}>{f.filename}</button>
        <span class="shrink-0 text-[11px] text-faint tabular-nums group-focus-within:hidden group-hover:hidden">{formatSize(f.size)}</span>
        <button class="hidden shrink-0 text-faint transition-colors group-focus-within:block group-hover:block hover:text-danger" aria-label={t(`„${f.filename}“ entfernen`, `Remove “${f.filename}”`)} title={t('Datei entfernen', 'Remove file')}
          onclick={() => api.deleteAttachment(f.id).catch((e) => ws.notify(errorMessage(e)))}><X size={13} /></button>
      </div>
    {/each}

    {#if pending.length}
      <div class="mt-1 rounded-lg border border-line-soft bg-bg/40 p-2" in:pop={{ duration: 160, y: 3 }}>
        <p class="mb-1.5 px-0.5 text-xs text-muted">{t('Wo im Ordner des Skills sollen sie liegen?', 'Where in the skill’s folder should they go?')}</p>
        {#each pending as p, i (p.key)}
          <div class="mb-1.5">
            <div class="flex items-center gap-1.5">
              <input class="field !h-7.5 min-w-0 flex-1 font-mono !text-[12px] {problems[i] ? '!border-danger/60' : ''}" aria-label={t(`Pfad für ${p.file.name}`, `Path for ${p.file.name}`)}
                bind:value={p.path} onkeydown={onPathKey} />
              <span class="w-12 shrink-0 text-right text-[11px] text-faint tabular-nums">{formatSize(p.file.size)}</span>
              <button class="icon-btn !h-6 !w-6 shrink-0" aria-label={t(`${p.file.name} nicht hinzufügen`, `Do not add ${p.file.name}`)} onclick={() => (pending = pending.filter((o) => o.key !== p.key))}><X size={13} /></button>
            </div>
            {#if problems[i]}<p class="mt-1 px-0.5 text-[11px] text-danger">{problems[i]}</p>{/if}
          </div>
        {/each}
        {#if tooMuch}<p class="mb-1.5 px-0.5 text-[11px] text-danger">{tooMuch}</p>{/if}
        {#if !creating}
          <div class="flex justify-end gap-1.5">
            <button class="btn-ghost !h-7 text-xs" onclick={() => (pending = [])}>{t('Verwerfen', 'Discard')}</button>
            <button class="btn-primary !h-7 !px-3 text-xs" disabled={!canUpload || uploading} onclick={() => upload(skillId!)}>{t('Hinzufügen', 'Add')}</button>
          </div>
        {/if}
      </div>
    {:else if !files.length}
      <p class="mb-1 text-xs leading-relaxed text-faint">{t('Skripte, Vorlagen oder Nachschlagewerke, die zur Anleitung gehören. Optional.', 'Scripts, templates or references that go with the instructions. Optional.')}</p>
    {/if}
    <button class="chip mt-1.5 gap-1 !bg-transparent transition-colors hover:!border-faint hover:text-text" title={t('Wird verschlüsselt gespeichert. Geht auch per Ziehen.', 'Stored encrypted. Dragging works too.')} onclick={() => filePicker?.click()}>
      <Plus size={11} /> {t('Datei hinzufügen', 'Add a file')}
    </button>

    {#if info?.source}
      {@const source = info.source}
      <div class="section-title mt-7">{t('Herkunft', 'Source')}</div>
      <div class="card p-3">
        <div class="flex items-center gap-2">
          <FolderGit2 size={14} class="shrink-0 text-faint" />
          <span class="min-w-0 truncate font-medium">{source.repo}</span>
          <span class="ml-auto shrink-0 font-mono text-[11px] text-faint" title={source.revision}>{shortRevision(source.revision)}</span>
        </div>
        <div class="mt-0.5 truncate pl-5.5 text-xs text-faint">{source.path || t('Hauptordner', 'Top folder')} · {t(`importiert am ${day(source.imported_at)}`, `imported on ${day(source.imported_at)}`)}</div>

        {#if refresh?.step === 'ready'}
          {@const ready = refresh}
          <div class="mt-3 border-t border-line-soft pt-3" bind:this={readyBox} in:pop={{ duration: 160, y: 3 }}>
            <p class="text-xs leading-relaxed text-muted">
              {t(`Neue Fassung ${shortRevision(ready.revision)}:`, `New version ${shortRevision(ready.revision)}:`)} {filesLabel(ready.candidate.files.length)}, {formatSize(ready.candidate.size)}.
              {t('Ersetzt Beschreibung, Anleitung und Dateien. Die Projekte bleiben.', 'It replaces the description, instructions and files. The projects stay.')}
            </p>
            <div class="mt-2.5 flex justify-end gap-1.5">
              <button class="btn-ghost !h-7 text-xs" onclick={() => (refresh = null)}>{t('Später', 'Later')}</button>
              <button class="btn-primary !h-7 !px-3 text-xs" onclick={() => applyUpdate(ready.url, ready.revision, ready.candidate.path)}>{t('Aktualisieren', 'Update')}</button>
            </div>
          </div>
        {:else}
          <div class="mt-2.5 flex min-h-7 items-center gap-2">
            <button class="btn-ghost !h-7 shrink-0 text-xs" disabled={refresh?.step === 'checking' || refresh?.step === 'updating'} onclick={checkSource}>
              {#if refresh?.step === 'checking' || refresh?.step === 'updating'}<LoaderCircle size={13} class="animate-spin" />{:else}<RefreshCw size={13} />{/if}
              {refresh?.step === 'updating' ? t('Aktualisiert …', 'Updating …') : refresh?.step === 'checking' ? t('Sieht nach …', 'Checking …') : t('Von GitHub aktualisieren', 'Update from GitHub')}
            </button>
            {#if refresh?.step === 'current'}
              <span class="flex min-w-0 items-center gap-1 text-xs text-ok" in:pop={{ duration: 140, y: 0 }}><Check size={13} class="shrink-0" /> {t('Schon aktuell', 'Already up to date')}</span>
            {:else if refresh?.step === 'done'}
              <span class="flex min-w-0 items-center gap-1 text-xs text-ok" in:pop={{ duration: 140, y: 0 }}><Check size={13} class="shrink-0" /> {t('Aktualisiert', 'Updated')}</span>
            {/if}
          </div>
          {#if refresh?.step === 'failed'}
            <p class="mt-2 flex gap-1.5 text-xs leading-relaxed text-danger" role="alert" in:pop={{ duration: 140, y: -2 }}><CircleAlert size={13} class="mt-px shrink-0" />{refresh.message}</p>
          {/if}
        {/if}
      </div>
    {/if}
  </div>

  {#if creating}
    <footer class="flex h-13 shrink-0 items-center gap-2 border-t border-line-soft px-5">
      <span class="min-w-0 flex-1 truncate text-[11px] text-faint">{missing === 'title' ? t('Noch ohne Namen', 'No name yet') : missing === 'description' ? t('Noch ohne Beschreibung', 'No description yet') : ''}</span>
      <button class="btn-ghost !h-8 text-xs" onclick={() => (ws.newSkill = false)}>{t('Abbrechen', 'Cancel')}</button>
      <button class="btn-primary !h-8 text-xs" disabled={!!missing || busy || (pending.length > 0 && !canUpload)} onclick={create}>
        {#if busy}<LoaderCircle size={13} class="animate-spin" />{:else}<Check size={13} />{/if} {t('Skill anlegen', 'Create skill')}
      </button>
    </footer>
  {:else}
    <footer class="flex items-center gap-1.5 border-t border-line-soft px-5 py-2 text-[11px] text-faint tabular-nums">
      {#if saving}
        <span class="h-1.5 w-1.5 animate-pulse rounded-full bg-warn"></span><span>{t('Speichert …', 'Saving …')}</span>
      {:else if dirty}
        <span class="h-1.5 w-1.5 rounded-full {missing || error ? 'bg-danger' : 'animate-pulse bg-warn'}"></span><span>{missing || error ? t('Nicht gespeichert', 'Not saved') : t('Ungespeichert', 'Unsaved')}</span>
      {:else if info}
        <span>{t('Geändert', 'Changed')} {fmt(info.updated_at)}</span>
      {/if}
      {#if info}<span class="ml-auto font-mono">{info.slug}</span>{/if}
    </footer>
  {/if}
</aside>

{#if previewing}
  {#key previewing.id}
    <AttachmentPreview attachment={previewing} onclose={() => (previewing = null)} />
  {/key}
{/if}
