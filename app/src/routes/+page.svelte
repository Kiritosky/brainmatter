<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { invoke } from '@tauri-apps/api/core';

  onMount(async () => {
    const spike = await invoke<string | null>('spike_mode').catch(() => null);
    if (spike === 'graph') goto('/spike');
  });
</script>

<main>
  <h1>Brainmatter</h1>
  <p>Phase 0 · <a href="/spike">Graph-Spike starten</a></p>
</main>

<style>
  :global(body) {
    margin: 0;
    background: #11141b;
    color: #d8deea;
    font: 14px -apple-system, system-ui, sans-serif;
  }
  main {
    padding: 48px;
  }
  a {
    color: #7c9cff;
  }
</style>
