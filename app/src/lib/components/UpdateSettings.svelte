<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { Check, Download, RefreshCw } from '@lucide/svelte';
  import { api, errorMessage, onUpdateProgress, type Settings } from '#lib/api.ts';
  import { formatSize } from '#lib/attachments.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import Toggle from '#lib/ui/Toggle.svelte';
  import { pop } from '#lib/ui/motion.ts';

  let { settings, onsave }: { settings: Settings; onsave: (patch: Partial<Settings>) => void } = $props();

  let version = $state('');
  let checking = $state(false);
  let upToDate = $state(false);
  let installing = $state(false);
  let error = $state<string | null>(null);
  let progress = $state<{ downloaded: number; total: number | null } | null>(null);
  let stop: (() => void) | undefined;

  onMount(async () => {
    version = await api.appVersion();
    stop = await onUpdateProgress((p) => (progress = p));
  });
  onDestroy(() => stop?.());

  async function check() {
    checking = true;
    upToDate = false;
    error = null;
    try {
      ws.update = await api.checkUpdate();
      upToDate = !ws.update;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      checking = false;
    }
  }

  async function install() {
    installing = true;
    error = null;
    try {
      // On success the app restarts, and this page is gone with it.
      await api.installUpdate();
    } catch (e) {
      error = errorMessage(e);
      installing = false;
    }
  }

  const share = $derived(progress?.total ? Math.min(100, (progress.downloaded / progress.total) * 100) : 0);
</script>

<h3 class="section-title mt-7">Updates</h3>
<div class="card">
  <div class="row">
    <div class="flex-1">
      <div>Automatisch nach Updates suchen</div>
      <div class="text-xs text-faint">Fragt einmal täglich bei GitHub nach. Dabei werden keine Einträge übertragen.</div>
    </div>
    <Toggle checked={settings.auto_update} label="Automatisch nach Updates suchen" onchange={(v) => onsave({ auto_update: v })} />
  </div>
  <div class="row">
    {#if ws.update}
      <div class="min-w-0 flex-1" in:pop={{ duration: 180, y: 3 }}>
        <div class="font-medium">Version {ws.update.version} ist verfügbar</div>
        <div class="text-xs text-faint">Installiert ist {version}. Die App sperrt sich und startet dann neu.</div>
      </div>
      <button class="btn-primary shrink-0" disabled={installing} onclick={install}>
        <Download size={14} /> {installing ? 'Wird geladen …' : 'Installieren'}
      </button>
    {:else}
      <div class="min-w-0 flex-1">
        <div>Version {version}</div>
        {#if upToDate}
          <div class="flex items-center gap-1 text-xs text-ok" in:pop={{ duration: 160, y: 2 }}><Check size={12} /> Das ist die neueste Version.</div>
        {:else}
          <div class="text-xs text-faint">Updates sind signiert, andere installiert die App nicht.</div>
        {/if}
      </div>
      <button class="btn-ghost !h-7.5 shrink-0 text-xs" disabled={checking} onclick={check}>
        <RefreshCw size={13} class={checking ? 'animate-spin' : ''} /> Jetzt suchen
      </button>
    {/if}
  </div>
  {#if installing && progress}
    <div class="border-t border-line-soft px-4 py-3" in:pop={{ duration: 160, y: -2 }}>
      <div class="h-1.5 overflow-hidden rounded-full bg-bg/70">
        <div class="h-full rounded-full bg-accent transition-[width] duration-200 ease-(--ease-out-soft)" style="width: {share}%"></div>
      </div>
      <div class="mt-1.5 text-right text-xs text-faint tabular-nums">{formatSize(progress.downloaded)}{progress.total ? ` von ${formatSize(progress.total)}` : ''}</div>
    </div>
  {/if}
</div>
{#if error}
  <p class="mt-2 px-1 text-xs text-danger" in:pop={{ duration: 140, y: 2 }}>{error}</p>
{/if}
