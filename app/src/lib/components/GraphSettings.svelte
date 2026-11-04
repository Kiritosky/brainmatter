<script lang="ts">
  import { t } from '#lib/i18n.ts';
  import { prefs, type LabelAmount, type Scale } from '#lib/prefs.svelte.ts';
  import Segmented from '#lib/ui/Segmented.svelte';
  import Toggle from '#lib/ui/Toggle.svelte';

  const SCALES: { value: Scale; label: string }[] = [
    { value: 'small', label: t('Klein', 'Small') },
    { value: 'medium', label: t('Mittel', 'Medium') },
    { value: 'large', label: t('Groß', 'Large') },
  ];
  const WEIGHTS: { value: Scale; label: string }[] = [
    { value: 'small', label: t('Dünn', 'Thin') },
    { value: 'medium', label: t('Normal', 'Normal') },
    { value: 'large', label: t('Kräftig', 'Bold') },
  ];
  const LABELS: { value: LabelAmount; label: string }[] = [
    { value: 'few', label: t('Wenige', 'Few') },
    { value: 'normal', label: t('Normal', 'Normal') },
    { value: 'many', label: t('Viele', 'Many') },
  ];
</script>

<h2 class="mb-1 text-lg font-semibold">Graph</h2>
<p class="mb-5 text-muted">{t('Gilt nur auf diesem Mac und ändert nichts an deinen Einträgen.', 'Applies only on this Mac and changes nothing in your entries.')}</p>

<h3 class="section-title">{t('Projekte', 'Projects')}</h3>
<div class="card">
  <div class="row">
    <div class="flex-1">
      <div>{t('Hintergründe der Projekte', 'Project backgrounds')}</div>
      <div class="text-xs text-faint">{t('Die Kreise um jedes Projekt. Ohne sie bleiben die Projekte trotzdem getrennt angeordnet.', 'The circles around each project. Without them, projects are still laid out apart.')}</div>
    </div>
    <Toggle checked={prefs.values.groupBackgrounds} label={t('Hintergründe der Projekte', 'Project backgrounds')} onchange={(v) => prefs.set('groupBackgrounds', v)} />
  </div>
  <div class="row">
    <div class="flex-1">
      <div>{t('Namen der Projekte', 'Project names')}</div>
      <div class="text-xs text-faint">{t('Klickbar, um das Projekt zu öffnen', 'Click one to open the project')}</div>
    </div>
    <Toggle checked={prefs.values.groupNames} label={t('Namen der Projekte', 'Project names')} onchange={(v) => prefs.set('groupNames', v)} />
  </div>
</div>

<h3 class="section-title mt-7">{t('Mit der Maus über einem Eintrag', 'With the pointer over an entry')}</h3>
<div class="card">
  <div class="row">
    <div class="flex-1">
      <div>{t('Einträge weichen aus', 'Entries make room')}</div>
      <div class="text-xs text-faint">{t('Der Eintrag wird größer, die Einträge daneben rücken zur Seite', 'The entry grows, and the ones next to it move aside')}</div>
    </div>
    <Toggle checked={prefs.values.hoverPush} label={t('Einträge weichen aus', 'Entries make room')} onchange={(v) => prefs.set('hoverPush', v)} />
  </div>
  <div class="row">
    <div class="flex-1">
      <div>{t('Unverbundenes abdunkeln', 'Dim what is not linked')}</div>
      <div class="text-xs text-faint">{t('Hebt hervor, womit der Eintrag verlinkt ist', 'Brings out what the entry is linked to')}</div>
    </div>
    <Toggle checked={prefs.values.hoverDim} label={t('Unverbundenes abdunkeln', 'Dim what is not linked')} onchange={(v) => prefs.set('hoverDim', v)} />
  </div>
</div>

<h3 class="section-title mt-7">{t('Darstellung', 'Display')}</h3>
<div class="card">
  <div class="row">
    <div class="flex-1">{t('Größe der Einträge', 'Entry size')}</div>
    <div class="w-56"><Segmented value={prefs.values.nodeSize} options={SCALES} label={t('Größe der Einträge', 'Entry size')} onchange={(v) => prefs.set('nodeSize', v)} /></div>
  </div>
  <div class="row">
    <div class="flex-1">{t('Stärke der Linien', 'Line weight')}</div>
    <div class="w-56"><Segmented value={prefs.values.edgeWeight} options={WEIGHTS} label={t('Stärke der Linien', 'Line weight')} onchange={(v) => prefs.set('edgeWeight', v)} /></div>
  </div>
  <div class="row">
    <div class="flex-1">
      <div>{t('Beschriftungen', 'Labels')}</div>
      <div class="text-xs text-faint">{t('Wie viele Namen gleichzeitig zu sehen sind', 'How many names show at once')}</div>
    </div>
    <div class="w-56"><Segmented value={prefs.values.labels} options={LABELS} label={t('Beschriftungen', 'Labels')} onchange={(v) => prefs.set('labels', v)} /></div>
  </div>
</div>
