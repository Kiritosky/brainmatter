<script lang="ts">
  import { t } from '#lib/i18n.ts';
  import { flip } from 'svelte/animate';
  import { BookMarked, Bot, Check, FolderGit2, Paperclip, Plus, Puzzle, Search, X } from '@lucide/svelte';
  import { api, errorMessage, type SkillInfo, type TemplateInfo } from '#lib/api.ts';
  import { filesLabel, shortRevision } from '#lib/skills.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';
  import Segmented from '#lib/ui/Segmented.svelte';

  const SEARCH_FROM = 6;

  const TABS: { value: 'templates' | 'skills'; label: string }[] = [
    { value: 'templates', label: t('Vorlagen', 'Templates') },
    { value: 'skills', label: 'Skills' },
  ];

  let items = $state.raw<TemplateInfo[]>([]);
  let skills = $state.raw<SkillInfo[] | null>(null);
  let query = $state('');
  const q = $derived(query.trim().toLowerCase());
  const shown = $derived(q ? items.filter((item) => item.node.title.toLowerCase().includes(q) || item.projects.some((p) => p.name.toLowerCase().includes(q))) : items);
  const shownSkills = $derived(
    (skills ?? []).filter(
      (s) => !q || s.node.title.toLowerCase().includes(q) || s.description.toLowerCase().includes(q) || s.projects.some((p) => p.name.toLowerCase().includes(q)),
    ),
  );
  const count = $derived(ws.libraryTab === 'skills' ? (skills?.length ?? 0) : items.length);

  $effect(() => {
    void ws.revision;
    api.templates().then((found) => (items = found)).catch(() => {});
    api.skills().then((found) => (skills = found)).catch(() => (skills ??= []));
  });

  async function markReviewed(skill: SkillInfo) {
    try {
      await api.updateNode(skill.node.id, { reviewed: true });
      skills = (skills ?? []).map((s) => (s === skill ? { ...s, reviewed: true } : s));
    } catch (e) {
      ws.notify(errorMessage(e));
    }
  }
</script>

<aside class="flex h-full w-100 flex-col border-l border-line bg-panel">
  <header class="px-4 pt-3 pb-3" data-tauri-drag-region>
    <div class="flex items-center gap-1">
      <h2 class="flex-1 text-[15px] font-semibold tracking-tight">{t('Bibliothek', 'Library')}</h2>
      {#if ws.libraryTab === 'skills'}
        <button class="btn-ghost !h-7 text-xs" title={t('Skills aus einem GitHub-Repo holen', 'Get skills from a GitHub repo')} onclick={() => (ws.skillImport = true)}><FolderGit2 size={13} /> {t('Importieren', 'Import')}</button>
        <button class="btn-ghost !h-7 text-xs" onclick={() => ws.startSkill()}><Plus size={13} /> {t('Neuer Skill', 'New skill')}</button>
      {:else}
        <button class="btn-ghost !h-7 text-xs" onclick={() => (ws.templateWizard = true)}><Plus size={13} /> {t('Neue Vorlage', 'New template')}</button>
      {/if}
      <button class="icon-btn -mr-1" aria-label={t('Schließen', 'Close')} onclick={() => (ws.panel = null)}><X size={15} /></button>
    </div>
    <div class="mt-3">
      <Segmented bind:value={ws.libraryTab} options={TABS} label={t('Bibliothek', 'Library')} onchange={() => (query = '')} />
    </div>
    <p class="mt-2.5 text-xs leading-relaxed text-muted">
      {#if ws.libraryTab === 'skills'}
        {t('Anleitungen, die eine KI von selbst lädt, wenn eine Aufgabe passt. Eine Vorlage rufst du auf; einen Skill wählt die KI anhand seiner Beschreibung.', 'Instructions an AI client loads by itself when a task fits. You call up a template; an AI picks a skill by its description.')}
      {:else}
        {t('Wiederverwendbare Texte mit Platzhaltern. Geprüfte Vorlagen stehen verbundenen KIs auch als Prompt zur Verfügung.', 'Reusable texts with placeholders. Reviewed templates are also offered to connected AI clients as prompts.')}
      {/if}
    </p>
  </header>

  {#if count >= SEARCH_FROM}
    {@const label = ws.libraryTab === 'skills' ? t('Skill suchen', 'Search skills') : t('Vorlage suchen', 'Search templates')}
    <div class="relative mx-4 mb-2">
      <Search size={13} class="absolute top-2 left-2 text-faint" />
      <input class="field !h-7.5 w-full !pl-7 text-xs" placeholder={label} aria-label={label} bind:value={query} />
    </div>
  {/if}

  <div class="flex-1 overflow-y-auto px-2 pb-3">
    {#if ws.libraryTab === 'skills'}
      {#each shownSkills as skill (skill.node.id)}
        <div class="group flex items-start gap-2 rounded-lg px-2 py-2 transition-colors hover:bg-elevated" animate:flip={{ duration: 180 }}>
          <button class="min-w-0 flex-1 text-left" onclick={() => ws.select(skill.node.id)}>
            <div class="truncate font-medium">{skill.node.title}</div>
            <div class="mt-0.5 line-clamp-2 text-xs leading-relaxed text-muted">{skill.description}</div>
            <div class="mt-1.5 flex flex-wrap items-center gap-1">
              {#each skill.projects as p (p.id)}
                <span class="chip !h-5 !px-1.5 text-[11px]"><span class="h-1.5 w-1.5 rounded-full" style="background: {ws.projectColor.get(p.id)}"></span>{p.name}</span>
              {:else}
                <span class="chip !h-5 !border-transparent !bg-transparent !px-0.5 text-[11px] !text-faint">{t('Überall verfügbar', 'Available everywhere')}</span>
              {/each}
              {#if skill.source}
                <span class="chip !h-5 gap-1 !px-1.5 text-[11px]" title={t(`Aus ${skill.source.repo}, Stand ${shortRevision(skill.source.revision)}`, `From ${skill.source.repo}, as of ${shortRevision(skill.source.revision)}`)}>
                  <FolderGit2 size={10} class="shrink-0" />{skill.source.repo}<span class="font-mono text-faint">{shortRevision(skill.source.revision)}</span>
                </span>
              {/if}
              {#if skill.files.length}
                <span class="flex h-5 items-center gap-1 px-1 text-[11px] text-faint" title={filesLabel(skill.files.length)}><Paperclip size={10} />{skill.files.length}</span>
              {/if}
            </div>
          </button>
          {#if !skill.reviewed}
            <button class="chip shrink-0 gap-1 !border-ai/30 !bg-ai/10 !text-ai transition-colors hover:!bg-ai/20"
              title={t('Von einer KI geschrieben. Klicken, um ihn als geprüft zu markieren; erst dann bekommen KIs ihn auch als Befehl.', 'Written by an AI client. Click to mark it as reviewed; only then do AI clients get it as a command too.')}
              onclick={() => markReviewed(skill)}>
              <Bot size={11} /> {t('ungeprüft', 'unreviewed')} <Check size={11} />
            </button>
          {/if}
        </div>
      {:else}
        {#if q}
          <p class="px-2 py-6 text-center text-xs text-faint">{t('Kein Skill gefunden.', 'No skill found.')}</p>
        {:else if skills}
          <div class="flex flex-col items-center px-8 pt-16 text-center" in:pop={{ duration: 240, y: 8 }}>
            <div class="mb-3 flex h-11 w-11 items-center justify-center text-faint"><Puzzle size={20} /></div>
            <div class="font-medium">{t('Noch keine Skills', 'No skills yet')}</div>
            <p class="mt-1 text-xs leading-relaxed text-muted">{t('Bring einer KI bei, wie du etwas erledigt haben willst, etwa Release-Notizen oder ein Code-Review. Sie greift dann von selbst darauf zurück.', 'Teach an AI client how you want something done, such as release notes or a code review. It then turns to it by itself.')}</p>
            <div class="mt-4 flex gap-2">
              <button class="btn-ghost !h-8 text-xs" onclick={() => (ws.skillImport = true)}><FolderGit2 size={13} /> {t('Von GitHub', 'From GitHub')}</button>
              <button class="btn-primary !h-8 text-xs" onclick={() => ws.startSkill()}>{t('Ersten Skill anlegen', 'Create a first skill')}</button>
            </div>
          </div>
        {/if}
      {/each}
    {:else}
      {#each shown as item (item.node.id)}
        <button class="flex w-full items-center gap-2.5 rounded-lg px-2 py-2 text-left transition-colors hover:bg-elevated" animate:flip={{ duration: 180 }}
          onclick={() => ws.select(item.node.id)}>
          <div class="min-w-0 flex-1">
            <div class="truncate font-medium">{item.node.title}</div>
            <div class="truncate text-xs text-faint">
              {[item.projects.length ? item.projects.map((p) => p.name).join(', ') : t('Überall verfügbar', 'Available everywhere'), ...(item.arguments.length ? [item.arguments.join(', ')] : [])].join(' · ')}
            </div>
          </div>
          {#if !item.reviewed}
            <span class="chip shrink-0 gap-1 !border-ai/30 !bg-ai/10 !text-ai" title={t('Von einer KI geschrieben. Wird erst nach deiner Prüfung als Prompt angeboten.', 'Written by an AI client. Offered as a prompt only once you have reviewed it.')}><Bot size={11} /> {t('ungeprüft', 'unreviewed')}</span>
          {/if}
        </button>
      {:else}
        {#if q}
          <p class="px-2 py-6 text-center text-xs text-faint">{t('Keine Vorlage gefunden.', 'No template found.')}</p>
        {:else}
        <div class="flex flex-col items-center px-8 pt-16 text-center" in:pop={{ duration: 240, y: 8 }}>
          <div class="mb-3 flex h-11 w-11 items-center justify-center text-faint"><BookMarked size={20} /></div>
          <div class="font-medium">{t('Noch keine Vorlagen', 'No templates yet')}</div>
          <p class="mt-1 text-xs leading-relaxed text-muted">{t('Zum Beispiel eine Code-Review-Checkliste oder dein Prompt für ein Projekt.', 'A code review checklist, say, or your prompt for a project.')}</p>
          <button class="btn-primary mt-4 !h-8 text-xs" onclick={() => (ws.templateWizard = true)}>{t('Erste Vorlage anlegen', 'Create a first template')}</button>
        </div>
        {/if}
      {/each}
    {/if}
  </div>
</aside>
