<script lang="ts">
  import { locale, plural, t } from '#lib/i18n.ts';
  import { onMount, tick, untrack } from 'svelte';
  import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
  import { AppWindow, Bot, CornerDownLeft, Fingerprint, Lock, LockOpen, Search, Zap } from '@lucide/svelte';
  import {
    api,
    errorMessage,
    onDomainEvent,
    onSession,
    onTrayOpen,
    type ChangeSetInfo,
    type Id,
    type McpInfo,
    type SearchHit,
    type Status,
  } from '#lib/api.ts';
  import { summary as readable } from '#lib/summaries.ts';
  import { avatarStyle, tint } from '#lib/ui/tint.ts';
  import { pop } from '#lib/ui/motion.ts';

  let { status: initial }: { status: Status } = $props();

  /** In points, as the window is made in `tray.rs`. */
  const WIDTH = 340;
  const RECENT = 5;
  const inTauri = document.documentElement.classList.contains('tauri');

  let status = $state(untrack(() => initial));
  let changes = $state.raw<ChangeSetInfo[]>([]);
  let mcp = $state.raw<McpInfo | null>(null);
  let query = $state('');
  let hits = $state.raw<SearchHit[]>([]);
  let active = $state(0);
  let password = $state('');
  let error = $state<string | null>(null);
  let busy = $state(false);
  // A Touch ID prompt takes the focus; that must not close the panel.
  let unlocking = $state(false);
  let now = $state(Date.now());
  // Counts openings, so the panel comes in with its animation every time.
  let opened = $state(0);
  let panel: HTMLElement | undefined = $state();
  let searchField: HTMLInputElement | undefined = $state();
  let passwordField: HTMLInputElement | undefined = $state();

  const locked = $derived(status.state !== 'unlocked');
  const isAi = (c: ChangeSetInfo) => c.client !== null || c.actor_name !== status.user_name;

  const subline = $derived.by(() => {
    if (status.state === 'none') return t('Noch kein Tresor', 'No vault yet');
    if (locked) return t('Verschlüsselt auf diesem Mac', 'Encrypted on this Mac');
    if (!mcp) return '';
    if (!mcp.running) return t('KI-Server aus', 'AI server off');
    return plural(mcp.clients.length, ['KI-Verbindung', 'KI-Verbindungen'], ['AI connection', 'AI connections']);
  });

  function ago(ms: number) {
    const s = Math.max(0, Math.round((now - ms) / 1000));
    if (s < 60) return t('gerade eben', 'just now');
    if (s < 3600) return t(`vor ${Math.round(s / 60)} Min.`, `${Math.round(s / 60)} min ago`);
    if (s < 86_400) return t(`vor ${Math.round(s / 3600)} Std.`, `${Math.round(s / 3600)} h ago`);
    const days = Math.round(s / 86_400);
    if (days === 1) return t('gestern', 'yesterday');
    if (days < 7) return t(`vor ${days} Tagen`, `${days} days ago`);
    return new Date(ms).toLocaleDateString(locale, { day: 'numeric', month: 'short' });
  }

  async function load() {
    now = Date.now();
    if (status.state !== 'unlocked') return;
    const [recent, info] = await Promise.allSettled([api.changeSets(RECENT), api.mcpInfo()]);
    if (recent.status === 'fulfilled') changes = recent.value;
    if (info.status === 'fulfilled') mcp = info.value;
  }

  async function focusField() {
    await tick();
    (locked ? passwordField : searchField)?.focus();
  }

  async function reset() {
    query = '';
    hits = [];
    active = 0;
    error = null;
    password = '';
    status = await api.status();
    opened++;
    await load();
    await focusField();
  }

  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let searchRun = 0;
  function onQuery() {
    clearTimeout(searchTimer);
    active = 0;
    const text = query.trim();
    const run = ++searchRun;
    if (!text) {
      hits = [];
      return;
    }
    searchTimer = setTimeout(async () => {
      try {
        const found = await api.search(text, { limit: 6 });
        if (run === searchRun) hits = found;
      } catch {
        if (run === searchRun) hits = [];
      }
    }, 90);
  }

  const close = (giveBack: boolean) => api.hideTray(giveBack);
  const openEntry = (id: Id) => api.openInMain({ entry: id });
  const openActivity = () => api.openInMain({ panel: 'activity' });
  const openMain = () => api.openInMain({});

  async function unlockTouchId() {
    unlocking = true;
    error = null;
    try {
      await api.unlockTouchId();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      unlocking = false;
    }
  }

  async function unlockPassword(e: SubmitEvent) {
    e.preventDefault();
    if (!password || busy) return;
    busy = true;
    error = null;
    try {
      await api.unlockPassword(password);
      password = '';
    } catch (err) {
      error = errorMessage(err);
      passwordField?.select();
    } finally {
      busy = false;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      if (query) {
        query = '';
        onQuery();
      } else close(true);
      return;
    }
    if (locked || !hits.length) return;
    if (e.key === 'ArrowDown') active = Math.min(active + 1, hits.length - 1);
    else if (e.key === 'ArrowUp') active = Math.max(active - 1, 0);
    else if (e.key === 'Enter' && document.activeElement === searchField) openEntry(hits[active].node.id);
    else return;
    e.preventDefault();
  }

  // The window takes the height of what it shows. `offsetHeight`, because the
  // opening animation scales the panel, and the window must not follow that.
  $effect(() => {
    if (!inTauri || !panel) return;
    const element = panel;
    const window = getCurrentWindow();
    const fit = () => window.setSize(new LogicalSize(WIDTH, element.offsetHeight + 16));
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    return () => observer.disconnect();
  });

  onMount(() => {
    let reload: ReturnType<typeof setTimeout> | undefined;
    const cleanups = [
      onTrayOpen(reset),
      onSession((s) => {
        const changed = s.state !== status.state;
        status = s;
        if (changed) {
          error = null;
          load();
          focusField();
        }
      }),
      // What an AI does shows here while the panel is open.
      onDomainEvent((e) => {
        if (e.type !== 'change_set_committed') return;
        clearTimeout(reload);
        reload = setTimeout(load, 150);
      }),
      // Clicking elsewhere closes it, as with any menu. Not in a plain browser.
      inTauri
        ? getCurrentWindow().onFocusChanged(({ payload: focused }) => {
            if (!focused && !unlocking) close(false);
          })
        : Promise.resolve(() => {}),
    ];
    const clock = setInterval(() => (now = Date.now()), 30_000);
    reset();
    return () => {
      clearInterval(clock);
      clearTimeout(reload);
      cleanups.forEach((c) => c.then((off) => off()));
    };
  });
</script>

<svelte:window {onkeydown} />

<div class="p-2">
  {#key opened}
    <div bind:this={panel} class="surface overflow-hidden rounded-2xl" role="dialog" aria-label="Brainmatter" in:pop={{ duration: 170, y: -4 }}>
      <header class="flex items-center gap-3 px-4 pt-3.5 pb-3">
        <img src="/icon.png" alt="" class="h-7 w-7 shrink-0 rounded-[7px]" />
        <div class="min-w-0 flex-1">
          <div class="text-[13.5px] leading-tight font-semibold">Brainmatter</div>
          <div class="truncate text-[11.5px] text-faint">{subline}</div>
        </div>
        {#if status.state !== 'none'}
          <span class="state {locked ? 'locked' : 'open'}">
            {#if locked}<Lock size={11} /> {t('Gesperrt', 'Locked')}{:else}<span class="dot"></span> {t('Offen', 'Unlocked')}{/if}
          </span>
        {/if}
      </header>

      {#if status.state === 'none'}
        <div class="px-4 pb-4">
          <button class="btn-primary w-full" onclick={openMain}>{t('Brainmatter einrichten', 'Set up Brainmatter')}</button>
        </div>
      {:else if locked}
        <div class="px-4 pb-4">
          {#if status.touch_id_enrolled}
            <button class="btn-primary w-full" disabled={unlocking} onclick={unlockTouchId}>
              <Fingerprint size={15} /> {t('Mit Touch ID entsperren', 'Unlock with Touch ID')}
            </button>
            <div class="my-3 flex items-center gap-3 text-[11px] text-faint">
              <span class="h-px flex-1 bg-line-soft"></span>{t('oder', 'or')}<span class="h-px flex-1 bg-line-soft"></span>
            </div>
          {/if}
          <form class="flex gap-2" onsubmit={unlockPassword}>
            <input
              bind:this={passwordField}
              bind:value={password}
              type="password"
              class="field min-w-0 flex-1"
              placeholder={t('Master-Passwort', 'Master password')}
              aria-label={t('Master-Passwort', 'Master password')}
              autocomplete="current-password" />
            <button class="btn-ghost shrink-0 !px-3" type="submit" disabled={!password || busy} aria-label={t('Entsperren', 'Unlock')} title={t('Entsperren', 'Unlock')}>
              <LockOpen size={15} />
            </button>
          </form>
          {#if error}<p class="mt-2 text-xs text-danger" in:pop={{ duration: 140, y: -2 }}>{error}</p>{/if}
        </div>
      {:else}
        <div class="px-3 pb-2">
          <label class="search">
            <Search size={14} class="shrink-0 text-faint" />
            <input
              bind:this={searchField}
              bind:value={query}
              oninput={onQuery}
              placeholder={t('Einträge suchen …', 'Search entries …')}
              aria-label={t('Einträge suchen', 'Search entries')} />
            {#if hits.length}<span class="kbd"><CornerDownLeft size={10} /></span>{/if}
          </label>
        </div>

        {#if query.trim()}
          <div class="px-2 pb-2" role="listbox" aria-label={t('Treffer', 'Results')}>
            {#each hits as hit, i (hit.node.id)}
              <button class="hit" class:active={i === active} role="option" aria-selected={i === active}
                onmouseenter={() => (active = i)} onclick={() => openEntry(hit.node.id)}>
                <span class="truncate">{hit.node.title}</span>
              </button>
            {:else}
              <div class="px-3 py-2 text-[12.5px] text-faint">{t('Nichts gefunden', 'Nothing found')}</div>
            {/each}
          </div>
        {:else}
          <div class="px-2 pb-2">
            <div class="flex items-center justify-between px-2 pt-0.5 pb-1">
              <span class="text-[11.5px] font-medium text-faint">{t('Zuletzt', 'Recent')}</span>
              {#if changes.length}
                <button class="text-[11.5px] text-faint transition-colors duration-150 hover:text-text" onclick={openActivity}>{t('Alle anzeigen', 'Show all')}</button>
              {/if}
            </div>
            {#each changes as c (c.id)}
              {@const ai = isAi(c)}
              <button class="change" onclick={openActivity}>
                <span class="avatar" style={avatarStyle(ai ? tint(c.actor_name) : 'var(--color-accent)')}>
                  {#if ai}<Bot size={13} />{:else}{c.actor_name.slice(0, 1).toUpperCase()}{/if}
                </span>
                <span class="min-w-0 flex-1 text-left">
                  <span class="flex items-baseline gap-2 text-[11.5px]">
                    <span class="truncate font-medium" style={ai ? `color: ${tint(c.actor_name)}` : ''}>{c.actor_name}</span>
                    <time class="ml-auto shrink-0 text-faint">{ago(c.at)}</time>
                  </span>
                  <span class="block truncate text-[12.5px] {c.reverted_by ? 'text-faint line-through' : 'text-muted'}">{readable(c.summary)}</span>
                </span>
              </button>
            {:else}
              <div class="px-2 py-3 text-[12.5px] text-faint">{t('Noch keine Änderungen.', 'No changes yet.')}</div>
            {/each}
          </div>
        {/if}
      {/if}

      {#if status.state === 'unlocked'}
        <footer class="grid grid-cols-3 gap-1 border-t border-line-soft p-1.5">
          <button class="action" onclick={() => api.toggleCapture()} title={t('Schnell erfassen (⌥⇧Leertaste)', 'Quick capture (⌥⇧Space)')}>
            <Zap size={15} />{t('Erfassen', 'Capture')}
          </button>
          <button class="action" onclick={openMain}><AppWindow size={15} />{t('Öffnen', 'Open')}</button>
          <button class="action" onclick={() => api.lock()}><Lock size={15} />{t('Sperren', 'Lock')}</button>
        </footer>
      {:else if locked}
        <footer class="border-t border-line-soft p-1.5">
          <button class="action !flex-row !justify-center !gap-2 w-full" onclick={openMain}>
            <AppWindow size={14} />{t('Brainmatter öffnen', 'Open Brainmatter')}
          </button>
        </footer>
      {/if}
    </div>
  {/key}
</div>

<style>
  .state {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 5px;
    height: 22px;
    padding: 0 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 500;
  }
  .state.open {
    color: var(--color-ok);
    background: color-mix(in srgb, var(--color-ok) 12%, transparent);
  }
  .state.locked {
    color: var(--color-muted);
    background: color-mix(in srgb, var(--color-text) 6%, transparent);
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: currentColor;
    box-shadow: 0 0 0 3px color-mix(in srgb, currentColor 22%, transparent);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    border-radius: 10px;
    background: var(--color-elevated);
    border: 1px solid var(--color-line-soft);
    transition: border-color 150ms;
  }
  .search:focus-within {
    border-color: color-mix(in srgb, var(--color-accent) 60%, var(--color-line));
  }
  .search input {
    flex: 1;
    min-width: 0;
    background: transparent;
    outline: none;
    font-size: 13px;
    color: var(--color-text);
  }
  .search input::placeholder {
    color: var(--color-faint);
  }
  .hit {
    display: flex;
    width: 100%;
    align-items: center;
    height: 32px;
    padding: 0 10px;
    border-radius: 8px;
    font-size: 13px;
    color: var(--color-muted);
    transition: background-color 120ms, color 120ms;
  }
  .hit.active {
    background: color-mix(in srgb, var(--color-text) 7%, transparent);
    color: var(--color-text);
  }
  .change {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 10px;
    padding: 7px 8px;
    border-radius: 10px;
    transition: background-color 120ms;
  }
  .change:hover {
    background: color-mix(in srgb, var(--color-text) 5%, transparent);
  }
  .avatar {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 600;
  }
  .action {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 0 7px;
    border-radius: 9px;
    font-size: 11.5px;
    color: var(--color-muted);
    transition: background-color 120ms, color 120ms;
  }
  .action:hover {
    background: color-mix(in srgb, var(--color-text) 6%, transparent);
    color: var(--color-text);
  }
</style>
