<script lang="ts">
  import { t } from '#lib/i18n.ts';
  import { onDestroy, onMount } from 'svelte';
  import { Check, Download, RefreshCw } from '@lucide/svelte';
  import { api, errorMessage, onUpdateProgress, type Settings } from '#lib/api.ts';
  import { formatSize } from '#lib/attachments.ts';
  import { releases } from '#lib/changelog.ts';
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

  /** The notes of an update that is not installed yet come with the update check. */
  function showUpdateNotes() {
    const update = ws.update;
    if (!update) return;
    ws.changelog = {
      heading: t(`Neu in Version ${update.version}`, `New in version ${update.version}`),
      items: [{ version: update.version, date: null, notes: update.notes ?? '' }],
    };
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
      <div>{t('Automatisch nach Updates suchen', 'Check for updates automatically')}</div>
      <div class="text-xs text-faint">{t('Fragt einmal täglich bei GitHub nach. Dabei werden keine Einträge übertragen.', 'Asks GitHub once a day. No entries are sent.')}</div>
    </div>
    <Toggle checked={settings.auto_update} label={t('Automatisch nach Updates suchen', 'Check for updates automatically')} onchange={(v) => onsave({ auto_update: v })} />
  </div>
  <div class="row">
    {#if ws.update}
      <div class="min-w-0 flex-1" in:pop={{ duration: 180, y: 3 }}>
        <div class="font-medium">{t(`Version ${ws.update.version} ist verfügbar`, `Version ${ws.update.version} is available`)}</div>
        <div class="text-xs text-faint">{t(`Installiert ist ${version}. Die App sperrt sich und startet dann neu.`, `${version} is installed. The app locks and then restarts.`)}</div>
      </div>
      {#if ws.update.notes}
        <button class="btn-ghost !h-7.5 shrink-0 text-xs" onclick={showUpdateNotes}>{t('Was ist neu?', 'What’s new?')}</button>
      {/if}
      <button class="btn-primary shrink-0" disabled={installing} onclick={install}>
        <Download size={14} /> {installing ? t('Wird geladen …', 'Downloading …') : t('Installieren', 'Install')}
      </button>
    {:else}
      <div class="min-w-0 flex-1">
        <div>{t('Version', 'Version')} {version}</div>
        {#if upToDate}
          <div class="flex items-center gap-1 text-xs text-ok" in:pop={{ duration: 160, y: 2 }}><Check size={12} /> {t('Das ist die neueste Version.', 'This is the latest version.')}</div>
        {:else}
          <div class="text-xs text-faint">{t('Updates sind signiert, andere installiert die App nicht.', 'Updates are signed; the app installs no others.')}</div>
        {/if}
      </div>
      <button class="btn-ghost !h-7.5 shrink-0 text-xs" onclick={() => (ws.changelog = { heading: t('Alle Änderungen', 'All changes'), items: releases })}>{t('Änderungen', 'Changes')}</button>
      <button class="btn-ghost !h-7.5 shrink-0 text-xs" disabled={checking} onclick={check}>
        <RefreshCw size={13} class={checking ? 'animate-spin' : ''} /> {t('Jetzt suchen', 'Check now')}
      </button>
    {/if}
  </div>
  {#if installing && progress}
    <div class="border-t border-line-soft px-4 py-3" in:pop={{ duration: 160, y: -2 }}>
      <div class="h-1.5 overflow-hidden rounded-full bg-bg/70">
        <div class="h-full rounded-full bg-accent transition-[width] duration-200 ease-(--ease-out-soft)" style="width: {share}%"></div>
      </div>
      <div class="mt-1.5 text-right text-xs text-faint tabular-nums">{formatSize(progress.downloaded)}{progress.total ? t(` von ${formatSize(progress.total)}`, ` of ${formatSize(progress.total)}`) : ''}</div>
    </div>
  {/if}
</div>
{#if error}
  <p class="mt-2 px-1 text-xs text-danger" in:pop={{ duration: 140, y: 2 }}>{error}</p>
{/if}
