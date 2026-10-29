<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { SCENARIOS, baselineFps, runScenario, type ScenarioResult } from '#lib/spike/graphBench.ts';

  let container: HTMLDivElement;
  let status = $state('starte…');
  let results = $state<ScenarioResult[]>([]);

  onMount(async () => {
    const baseline = await baselineFps();
    for (const s of SCENARIOS) {
      status = `läuft: ${s.name}`;
      results.push(await runScenario(container, s));
    }
    status = 'fertig';
    const report = { userAgent: navigator.userAgent, dpr: devicePixelRatio, baselineFps: baseline, results };
    console.table(results);
    await invoke('spike_report', { report }).catch(() => {});
  });
</script>

<div class="hud">
  <strong>Graph-Spike</strong> · {status}
  {#each results as r}
    <div>{r.name}: {r.fps} fps · p95 {r.frameP95Ms} ms · init {r.firstRenderMs} ms</div>
  {/each}
</div>
<div class="stage" bind:this={container}></div>

<style>
  :global(body) {
    margin: 0;
    background: #11141b;
    color: #d8deea;
    font: 13px -apple-system, system-ui, sans-serif;
  }
  .stage {
    position: fixed;
    inset: 0;
  }
  .hud {
    position: fixed;
    z-index: 1;
    top: 36px;
    left: 12px;
    padding: 8px 12px;
    border-radius: 8px;
    background: rgb(20 24 33 / 0.85);
  }
</style>
