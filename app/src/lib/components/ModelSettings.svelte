<script lang="ts">
  import { num, t } from '#lib/i18n.ts';
  import { Check, Download, ShieldCheck, Trash2 } from '@lucide/svelte';
  import { api, errorMessage } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import { pop } from '#lib/ui/motion.ts';

  let confirmRemove = $state(false);
  let error = $state<string | null>(null);

  const model = $derived(ws.model);
  const mb = (bytes: number) => Math.round(bytes / 1_000_000);
  const share = (part: number, whole: number) => (whole > 0 ? Math.min(100, (part / whole) * 100) : 0);
  const indexing = $derived(!!model && model.state === 'ready' && model.indexed < model.total);

  async function run(fn: () => Promise<unknown>) {
    error = null;
    try {
      await fn();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function remove() {
    confirmRemove = false;
    await run(api.removeModel);
  }
</script>

<h2 class="mb-1 text-lg font-semibold">{t('Suche nach Sinn', 'Search by meaning')}</h2>
<p class="mb-5 text-muted">
  {t(
    'Findet Einträge auch ohne die passenden Wörter, erkennt Duplikate und schlägt Verbindungen vor. Ohne Modell funktioniert die Wortsuche ganz normal.',
    'Finds entries even without the right words, spots duplicates and suggests links. Without the model, search by words works as usual.',
  )}
</p>

{#if model}
  <div class="card p-4">
    {#if model.state === 'ready'}
      <div class="flex items-center gap-2 font-medium" in:pop={{ duration: 200, y: 4 }}>
        <span class="flex h-5 w-5 items-center justify-center rounded-full bg-ok/15 text-ok"><Check size={12} /></span>
        {indexing ? t('Einträge werden erfasst', 'Reading entries') : t('Aktiv', 'Active')}
        <span class="ml-auto text-xs font-normal text-faint tabular-nums">{t(`${num(model.indexed)} von ${num(model.total)} Einträgen`, `${num(model.indexed)} of ${num(model.total)} entries`)}</span>
      </div>
      <div class="mt-3 h-1.5 overflow-hidden rounded-full bg-bg/70">
        <div class="h-full rounded-full bg-accent transition-[width] duration-300 ease-(--ease-out-soft)" style="width: {share(model.indexed, model.total)}%"></div>
      </div>
      <div class="mt-3 flex items-center">
        <span class="flex-1 text-xs text-faint">multilingual-e5-small · {mb(model.size)} MB</span>
        {#if confirmRemove}
          <button class="btn-danger !h-7" in:pop={{ duration: 120, y: 0 }} onclick={remove} onblur={() => (confirmRemove = false)}>{t('Wirklich entfernen?', 'Really remove?')}</button>
        {:else}
          <button class="btn-ghost !h-7 text-xs" onclick={() => (confirmRemove = true)}><Trash2 size={13} /> {t('Modell entfernen', 'Remove model')}</button>
        {/if}
      </div>
    {:else if model.state === 'downloading'}
      <div class="flex items-center font-medium">
        {t('Wird geladen', 'Downloading')}
        <span class="ml-auto text-xs font-normal text-faint tabular-nums">{t(`${mb(model.downloaded)} von ${mb(model.size)} MB`, `${mb(model.downloaded)} of ${mb(model.size)} MB`)}</span>
      </div>
      <div class="mt-3 h-1.5 overflow-hidden rounded-full bg-bg/70">
        <div class="h-full rounded-full bg-accent transition-[width] duration-300 ease-(--ease-out-soft)" style="width: {share(model.downloaded, model.size)}%"></div>
      </div>
      <p class="mt-3 text-xs text-faint">{t('Du kannst währenddessen normal weiterarbeiten.', 'You can keep working meanwhile.')}</p>
    {:else}
      <div class="flex items-center gap-4">
        <div class="min-w-0 flex-1">
          <div class="font-medium">{t('Modell laden', 'Download the model')}</div>
          <div class="text-xs text-faint">{t(`Einmalig ${mb(model.size)} MB von Hugging Face`, `${mb(model.size)} MB from Hugging Face, once`)}</div>
        </div>
        <button class="btn-primary shrink-0" onclick={() => run(api.downloadModel)}>
          <Download size={14} /> {model.error ? t('Erneut versuchen', 'Try again') : t('Laden', 'Download')}
        </button>
      </div>
    {/if}
    {#if error || model.error}
      <p class="mt-3 text-xs text-danger" in:pop={{ duration: 140, y: 2 }}>{error ?? model.error}</p>
    {/if}
  </div>

  <div class="mt-4 flex gap-2.5 px-1 text-xs leading-relaxed text-muted">
    <ShieldCheck size={15} class="mt-px shrink-0 text-ok" />
    <p>
      {t(
        'Das Modell läuft auf deinem Mac und wird nur dieses eine Mal aus dem Netz geladen. Deine Einträge verlassen den Mac dabei nie. Die daraus berechneten Daten liegen verschlüsselt im Tresor.',
        'The model runs on your Mac and is downloaded just this once. Your entries never leave the Mac. What is computed from them is stored encrypted in the vault.',
      )}
    </p>
  </div>
{/if}
