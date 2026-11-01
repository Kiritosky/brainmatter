<script lang="ts">
  import { prefs, type LabelAmount, type Scale } from '#lib/prefs.svelte.ts';
  import Segmented from '#lib/ui/Segmented.svelte';
  import Toggle from '#lib/ui/Toggle.svelte';

  const SCALES: { value: Scale; label: string }[] = [
    { value: 'small', label: 'Klein' },
    { value: 'medium', label: 'Mittel' },
    { value: 'large', label: 'Groß' },
  ];
  const WEIGHTS: { value: Scale; label: string }[] = [
    { value: 'small', label: 'Dünn' },
    { value: 'medium', label: 'Normal' },
    { value: 'large', label: 'Kräftig' },
  ];
  const LABELS: { value: LabelAmount; label: string }[] = [
    { value: 'few', label: 'Wenige' },
    { value: 'normal', label: 'Normal' },
    { value: 'many', label: 'Viele' },
  ];
</script>

<h2 class="mb-1 text-lg font-semibold">Graph</h2>
<p class="mb-5 text-muted">Gilt nur auf diesem Mac und ändert nichts an deinen Einträgen.</p>

<h3 class="section-title">Projekte</h3>
<div class="card">
  <div class="row">
    <div class="flex-1">
      <div>Hintergründe der Projekte</div>
      <div class="text-xs text-faint">Die Kreise um jedes Projekt. Ohne sie bleiben die Projekte trotzdem getrennt angeordnet.</div>
    </div>
    <Toggle checked={prefs.values.groupBackgrounds} label="Hintergründe der Projekte" onchange={(v) => prefs.set('groupBackgrounds', v)} />
  </div>
  <div class="row">
    <div class="flex-1">
      <div>Namen der Projekte</div>
      <div class="text-xs text-faint">Klickbar, um das Projekt zu öffnen</div>
    </div>
    <Toggle checked={prefs.values.groupNames} label="Namen der Projekte" onchange={(v) => prefs.set('groupNames', v)} />
  </div>
</div>

<h3 class="section-title mt-7">Mit der Maus über einem Eintrag</h3>
<div class="card">
  <div class="row">
    <div class="flex-1">
      <div>Einträge weichen aus</div>
      <div class="text-xs text-faint">Der Eintrag wird größer, die Einträge daneben rücken zur Seite</div>
    </div>
    <Toggle checked={prefs.values.hoverPush} label="Einträge weichen aus" onchange={(v) => prefs.set('hoverPush', v)} />
  </div>
  <div class="row">
    <div class="flex-1">
      <div>Unverbundenes abdunkeln</div>
      <div class="text-xs text-faint">Hebt hervor, womit der Eintrag verlinkt ist</div>
    </div>
    <Toggle checked={prefs.values.hoverDim} label="Unverbundenes abdunkeln" onchange={(v) => prefs.set('hoverDim', v)} />
  </div>
</div>

<h3 class="section-title mt-7">Darstellung</h3>
<div class="card">
  <div class="row">
    <div class="flex-1">Größe der Einträge</div>
    <div class="w-56"><Segmented value={prefs.values.nodeSize} options={SCALES} label="Größe der Einträge" onchange={(v) => prefs.set('nodeSize', v)} /></div>
  </div>
  <div class="row">
    <div class="flex-1">Stärke der Linien</div>
    <div class="w-56"><Segmented value={prefs.values.edgeWeight} options={WEIGHTS} label="Stärke der Linien" onchange={(v) => prefs.set('edgeWeight', v)} /></div>
  </div>
  <div class="row">
    <div class="flex-1">
      <div>Beschriftungen</div>
      <div class="text-xs text-faint">Wie viele Namen gleichzeitig zu sehen sind</div>
    </div>
    <div class="w-56"><Segmented value={prefs.values.labels} options={LABELS} label="Beschriftungen" onchange={(v) => prefs.set('labels', v)} /></div>
  </div>
</div>
