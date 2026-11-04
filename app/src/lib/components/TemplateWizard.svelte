<script lang="ts">
  import { t } from '#lib/i18n.ts';
  import { ArrowLeft, ArrowRight, Check, X } from '@lucide/svelte';
  import { api, errorMessage, TEMPLATE_KIND, type Id } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { fadeQuick, pop } from '#lib/ui/motion.ts';
  import ProjectPicker from '#lib/ui/ProjectPicker.svelte';
  import Segmented from '#lib/ui/Segmented.svelte';
  import MarkdownEditor from './MarkdownEditor.svelte';

  // Same rules as crates/bm-core/src/templates.rs.
  const PLACEHOLDER = /\{\{\s*([\p{L}\p{N}_-]+)\s*\}\}/gu;

  const STARTERS = [
    {
      id: 'review',
      name: t('Code-Review', 'Code review'),
      about: t('Worauf du bei jeder Änderung achtest', 'What you check in every change'),
      body: t(
        '## Review von {{thema}}\n\n- [ ] Tut es, was es soll?\n- [ ] Gibt es Tests für den neuen Weg?\n- [ ] Was passiert im Fehlerfall?\n\n## Anmerkungen\n\n',
        '## Review of {{topic}}\n\n- [ ] Does it do what it should?\n- [ ] Are there tests for the new path?\n- [ ] What happens when it fails?\n\n## Notes\n\n',
      ),
    },
    {
      id: 'prompt',
      name: t('Prompt für eine KI', 'Prompt for an AI'),
      about: t('Eine Anweisung, die du immer wieder gibst', 'An instruction you give again and again'),
      body: t(
        'Arbeite an {{aufgabe}} im Projekt {{projekt}}.\n\nLies zuerst die Doku des Projekts. Halte fest, was du entschieden hast und warum.\n',
        'Work on {{task}} in the project {{project}}.\n\nRead the project’s docs first. Write down what you decided and why.\n',
      ),
    },
    {
      id: 'meeting',
      name: t('Besprechung', 'Meeting'),
      about: t('Worum es ging und was daraus folgt', 'What it was about and what follows'),
      body: t(
        '## {{thema}} am {{datum}}\n\n### Dabei\n\n{{teilnehmer}}\n\n### Besprochen\n\n- \n\n### Entschieden\n\n- \n\n### Nächste Schritte\n\n- [ ] \n',
        '## {{topic}} on {{date}}\n\n### Present\n\n{{attendees}}\n\n### Discussed\n\n- \n\n### Decided\n\n- \n\n### Next steps\n\n- [ ] \n',
      ),
    },
    {
      id: 'bug',
      name: t('Fehlerbericht', 'Bug report'),
      about: t('Damit ein Fehler nachvollziehbar bleibt', 'So a bug stays traceable'),
      body: t(
        '## {{fehler}}\n\n### So tritt er auf\n\n1. \n\n### Erwartet\n\n\n### Tatsächlich\n\n',
        '## {{bug}}\n\n### Steps to reproduce\n\n1. \n\n### Expected\n\n\n### Actual\n\n',
      ),
    },
    { id: 'empty', name: t('Leer', 'Empty'), about: t('Du schreibst sie selbst', 'You write it yourself'), body: '' },
  ];
  const STEPS = [t('Art', 'Kind'), t('Inhalt', 'Content'), t('Verfügbar', 'Available')];

  let step = $state(0);
  let starter = $state<string | null>(null);
  let name = $state('');
  let body = $state('');
  let placeholder = $state('');
  let everywhere = $state<'all' | 'some'>('all');
  let chosen = $state.raw<Id[]>([]);
  let busy = $state(false);
  let editor: MarkdownEditor | undefined = $state();

  const names = $derived([...new Set([...body.matchAll(PLACEHOLDER)].map((m) => m[1]))]);
  const canContinue = $derived(step === 0 ? starter !== null && !!name.trim() : step === 1 ? !!body.trim() : !busy && (everywhere === 'all' || chosen.length > 0));

  function pick(id: string) {
    const previous = STARTERS.find((s) => s.id === starter);
    const next = STARTERS.find((s) => s.id === id)!;
    starter = id;
    // Follows the choice, until something of one's own is in there.
    if (!name.trim() || name === previous?.name) name = id === 'empty' ? '' : next.name;
    if (!body.trim() || body === previous?.body) body = next.body;
  }

  function addPlaceholder(e: SubmitEvent) {
    e.preventDefault();
    const clean = placeholder.trim().replace(/[^\p{L}\p{N}_-]+/gu, '-').replace(/^-+|-+$/g, '');
    if (clean) editor?.insert(`{{${clean}}}`);
    placeholder = '';
  }

  const close = () => (ws.templateWizard = false);

  async function finish() {
    busy = true;
    try {
      const node = await api.createNode({
        title: name.trim(),
        body,
        kind: TEMPLATE_KIND,
        projects: everywhere === 'some' ? chosen : [],
      });
      close();
      ws.panel = 'library';
      await ws.select(node.id);
    } catch (e) {
      ws.notify(errorMessage(e));
    } finally {
      busy = false;
    }
  }

  function next() {
    if (!canContinue) return;
    if (step < STEPS.length - 1) step++;
    else finish();
  }

  async function completeTitles(prefix: string) {
    const hits = await api.search(prefix, { limit: 8 });
    return hits.map((h) => h.node.title);
  }

  function onkeydowncapture(e: KeyboardEvent) {
    if (e.key !== 'Escape' || (e.target as HTMLElement).closest('[role="listbox"], .cm-editor')) return;
    e.stopPropagation();
    close();
  }

  const projectName = (id: Id) => ws.projects.find((p) => p.project.id === id)?.project.name ?? '';
</script>

<svelte:window {onkeydowncapture} />

<div class="fixed inset-0 z-[55] flex items-center justify-center bg-black/45 p-10 backdrop-blur-[2px]" role="presentation" transition:fadeQuick onclick={close}>
  <!-- svelte-ignore a11y_click_events_have_key_events -- the window handles Esc -->
  <div class="surface flex h-[540px] max-h-full w-[600px] flex-col overflow-hidden rounded-xl" role="dialog" aria-label={t('Neue Vorlage', 'New template')} tabindex="-1" transition:pop onclick={(e) => e.stopPropagation()}>
    <header class="flex h-12 shrink-0 items-center gap-3 border-b border-line-soft pr-3 pl-5">
      <h2 class="flex-1 text-[15px] font-semibold tracking-tight">{t('Neue Vorlage', 'New template')}</h2>
      <ol class="flex items-center gap-1 text-xs text-faint" aria-label={t('Fortschritt', 'Progress')}>
        {#each STEPS as label, i (label)}
          {#if i > 0}<span class="mx-0.5 h-px w-3 bg-line" aria-hidden="true"></span>{/if}
          <li class={i === step ? 'font-medium text-text' : i < step ? 'text-muted' : ''} aria-current={i === step ? 'step' : undefined}>{label}</li>
        {/each}
      </ol>
      <button class="icon-btn" aria-label={t('Schließen', 'Close')} onclick={close}><X size={15} /></button>
    </header>

    <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
      {#if step === 0}
        <h3 class="font-medium">{t('Womit möchtest du anfangen?', 'What would you like to start with?')}</h3>
        <p class="mt-0.5 mb-3 text-xs leading-relaxed text-muted">{t('Eine Vorlage ist ein Text, den du immer wieder brauchst. Was sich jedes Mal ändert, trägst du beim Verwenden ein.', 'A template is a text you need again and again. What changes each time, you fill in when you use it.')}</p>
        <div class="grid grid-cols-2 gap-2" role="radiogroup" aria-label={t('Art der Vorlage', 'Kind of template')}>
          {#each STARTERS as s (s.id)}
            <button class="rounded-lg border p-3 text-left transition-colors {starter === s.id ? 'border-accent/70 bg-text/5' : 'border-line-soft hover:border-line hover:bg-text/3'}"
              role="radio" aria-checked={starter === s.id} onclick={() => pick(s.id)}>
              <div class="font-medium">{s.name}</div>
              <div class="mt-0.5 text-xs leading-snug text-muted">{s.about}</div>
            </button>
          {/each}
        </div>
        <label class="mt-4 block">
          <span class="mb-1 block text-xs text-muted">{t('Name', 'Name')}</span>
          <input class="field w-full" placeholder={t('Wie soll sie heißen?', 'What should it be called?')} aria-label={t('Name der Vorlage', 'Template name')} bind:value={name} onkeydown={(e) => e.key === 'Enter' && next()} />
        </label>
      {:else if step === 1}
        <h3 class="font-medium">{t('Was steht drin?', 'What goes in it?')}</h3>
        <p class="mt-0.5 mb-3 text-xs leading-relaxed text-muted">{t('Schreib den Text, der gleich bleibt. Für alles, was sich ändert, füge einen Platzhalter ein.', 'Write the text that stays the same. For everything that changes, insert a placeholder.')} <kbd class="kbd">/</kbd> {t('zeigt alle Formate.', 'shows every format.')}</p>
        <form class="mb-2 flex gap-1.5" onsubmit={addPlaceholder}>
          <input class="field !h-7.5 min-w-0 flex-1 text-xs" placeholder={t('Name des Platzhalters, z. B. thema', 'Placeholder name, e.g. topic')} aria-label={t('Name des Platzhalters', 'Placeholder name')} bind:value={placeholder} />
          <button class="btn-ghost !h-7.5 shrink-0 text-xs" disabled={!placeholder.trim()}>{t('Einfügen', 'Insert')}</button>
        </form>
        <div class="min-h-40 rounded-lg border border-line-soft bg-bg/40 px-3 py-1">
          <MarkdownEditor bind:this={editor} value={body} placeholders titles={completeTitles} onchange={(text) => (body = text)} />
        </div>
        {#if names.length}
          <p class="mt-2 text-xs text-faint">{t('Beim Verwenden fragt sie nach:', 'When used, it asks for:')} {names.join(', ')}</p>
        {/if}
      {:else}
        <h3 class="font-medium">{t('Wo soll sie verfügbar sein?', 'Where should it be available?')}</h3>
        <p class="mt-0.5 mb-3 text-xs leading-relaxed text-muted">{t('Verbundene KIs bekommen sie dort auch als Befehl.', 'Connected AI clients get it there as a command too.')}</p>
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
      {/if}
    </div>

    <footer class="flex h-13 shrink-0 items-center gap-2 border-t border-line-soft px-5">
      {#if step > 0}
        <button class="btn-ghost !h-8 text-xs" onclick={() => step--}><ArrowLeft size={13} /> {t('Zurück', 'Back')}</button>
      {/if}
      <div class="flex-1"></div>
      <button class="btn-primary !h-8 text-xs" disabled={!canContinue} onclick={next}>
        {#if step < STEPS.length - 1}{t('Weiter', 'Next')} <ArrowRight size={13} />{:else}<Check size={13} /> {t('Vorlage anlegen', 'Create template')}{/if}
      </button>
    </footer>
  </div>
</div>
