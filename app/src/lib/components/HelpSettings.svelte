<script lang="ts">
  import { t } from '#lib/i18n.ts';
  import { Compass } from '@lucide/svelte';
  import { APP_FEATURES, MCP_COMMANDS, MCP_TOOLS } from '#lib/help.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import Segmented from '#lib/ui/Segmented.svelte';

  let section = $state<'app' | 'ai'>('app');

  const SECTIONS: { value: 'app' | 'ai'; label: string }[] = [
    { value: 'app', label: t('In der App', 'In the app') },
    { value: 'ai', label: t('Für KIs', 'For AI') },
  ];

  function startTour() {
    ws.settingsOpen = false;
    ws.tourOpen = true;
  }
</script>

<h2 class="mb-1 text-lg font-semibold">{t('Hilfe', 'Help')}</h2>
<p class="mb-4 text-muted">{t('Was du in Brainmatter tun kannst, und was verbundene KIs tun können.', 'What you can do in Brainmatter, and what connected AI clients can do.')}</p>

<div class="mb-4 flex items-center gap-3">
  <div class="w-56"><Segmented bind:value={section} options={SECTIONS} label={t('Bereich', 'Section')} /></div>
  <div class="flex-1"></div>
  <button class="btn-ghost !h-8 text-xs" onclick={startTour}><Compass size={13} /> {t('Einführung starten', 'Start the tour')}</button>
</div>

{#if section === 'app'}
  {#each APP_FEATURES as group (group.title)}
    <h3 class="section-title mt-5">{group.title}</h3>
    <div class="card">
      {#each group.items as item (item.label)}
        <div class="row !min-h-9 !py-1.5">
          <span class="flex-1">{item.label}</span>
          {#if item.keys}<kbd class="kbd !px-1.5">{item.keys}</kbd>{/if}
        </div>
      {/each}
    </div>
  {/each}
{:else}
  <p class="mb-1 leading-relaxed text-muted">
    {t(
      'Eine verbundene KI kann diese Werkzeuge von selbst nutzen. Du musst sie nicht kennen, aber du kannst sie gezielt verlangen: „Suche in Brainmatter nach …“. Einrichten geht unter KI-Verbindungen.',
      'A connected AI client uses these tools on its own. You need not know them, but you can ask for one: “Search Brainmatter for …”. Set it up under AI connections.',
    )}
  </p>

  <h3 class="section-title mt-5">{t('Befehle', 'Commands')}</h3>
  <div class="card">
    {#each MCP_COMMANDS as command (command.name)}
      <div class="row !min-h-9 !py-1.5">
        <code class="w-36 shrink-0 font-mono text-xs">{command.name}</code>
        <span class="min-w-0 flex-1 text-muted">{command.about}</span>
      </div>
    {/each}
  </div>

  <p class="mt-2 text-xs leading-relaxed text-faint">
    {t(
      'Geprüfte Vorlagen und Skills erscheinen dort ebenfalls als Befehle. Mit dem Skill-Menü einer Verbindung liegen sie alle auch im Skill-Ordner der App. Projekte und Doku-Seiten sehen KIs außerdem als Ressourcen, die sie durchstöbern können, nur für die Projekte ihrer Verbindung.',
      "Reviewed templates and skills show up there as commands too. With a connection's skill menu, all of them are in the app's skill folder as well. AI clients also see projects and documentation pages as resources to browse, only for the projects of their connection.",
    )}
  </p>

  {#each MCP_TOOLS as group (group.title)}
    <h3 class="section-title mt-5">{group.title}</h3>
    <div class="card">
      {#each group.tools as tool (tool.name)}
        <div class="row !min-h-9 !py-1.5">
          <code class="w-36 shrink-0 font-mono text-xs">{tool.name}</code>
          <span class="min-w-0 flex-1 text-muted">{tool.about}</span>
        </div>
      {/each}
    </div>
  {/each}
{/if}
