<script lang="ts">
  import { locale, t } from '#lib/i18n.ts';
  import { onMount } from 'svelte';
  import { flip } from 'svelte/animate';
  import { ArrowLeft, Bot, Check, Copy, Plus, Plug, Trash2 } from '@lucide/svelte';
  import { api, errorMessage, type AiClient, type Integration, type McpInfo, type Scope } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import Toggle from '#lib/ui/Toggle.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import { pop } from '#lib/ui/motion.ts';
  import { avatarStyle, tint } from '#lib/ui/tint.ts';

  type Preset = 'Claude Code' | 'Claude Desktop' | 'Cursor' | 'Codex' | 'Andere';
  const PRESETS: Preset[] = ['Claude Code', 'Claude Desktop', 'Cursor', 'Codex', 'Andere'];
  const SCOPES: { scope: Scope; label: string; hint: string }[] = [
    { scope: 'read', label: t('Lesen', 'Read'), hint: t('Suchen und Einträge lesen', 'Search and read entries') },
    { scope: 'write', label: t('Schreiben', 'Write'), hint: t('Einträge und Verbindungen anlegen und ändern', 'Create and change entries and links') },
    { scope: 'delete', label: t('Löschen', 'Delete'), hint: t('Einträge in den Papierkorb verschieben', 'Move entries to the trash') },
  ];
  const SCOPE_LABEL: Record<Scope, string> = { read: t('Lesen', 'Read'), write: t('Schreiben', 'Write'), delete: t('Löschen', 'Delete') };
  type Found = 'manual' | 'connected' | 'found' | 'missing';
  const FOUND_LABEL: Record<Found, string> = {
    manual: t('Manuell', 'Manual'),
    connected: t('Verbunden', 'Connected'),
    found: t('Erkannt', 'Found'),
    missing: t('Nicht gefunden', 'Not found'),
  };

  let info = $state<McpInfo | null>(null);
  let integrations = $state<Integration[]>([]);
  let view = $state<'list' | 'new' | 'created' | 'connected'>('list');
  let error = $state<string | null>(null);
  let busy = $state(false);
  let confirmRevoke = $state<string | null>(null);

  let preset = $state<Preset>('Claude Code');
  let name = $state('Claude Code');
  let scopes = $state<Record<Scope, boolean>>({ read: true, write: true, delete: false });
  let limitProjects = $state(false);
  let projects = $state<string[]>([]);

  let token = $state('');
  let copied = $state(false);

  onMount(load);

  async function load() {
    try {
      info = await api.mcpInfo();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  const integration = $derived(integrations.find((i) => i.name === preset));
  const invalid = $derived(!Object.values(scopes).some(Boolean) || (limitProjects && !projects.length));

  function presetHint(p: Preset): Found {
    if (p === 'Andere') return 'manual';
    const i = integrations.find((x) => x.name === p);
    if (i?.configured) return 'connected';
    return i?.installed ? 'found' : 'missing';
  }

  async function start() {
    integrations = await api.integrations().catch(() => []);
    // Start with what is actually here, preferring what is not connected yet.
    const first =
      integrations.find((i) => i.available && !i.configured) ?? integrations.find((i) => i.available);
    choose((first?.name as Preset) ?? 'Claude Code');
    scopes = { read: true, write: true, delete: false };
    limitProjects = false;
    projects = [];
    error = null;
    view = 'new';
  }

  /** A second Claude Code on another machine needs its own name. */
  function uniqueName(base: string) {
    const taken = new Set(info?.clients.map((c) => c.name.toLowerCase()));
    if (!taken.has(base.toLowerCase())) return base;
    for (let i = 2; ; i++) if (!taken.has(`${base} ${i}`.toLowerCase())) return `${base} ${i}`;
  }

  function choose(p: Preset) {
    preset = p;
    name = p === 'Andere' ? '' : uniqueName(p);
  }

  async function connect() {
    if (!integration) return;
    busy = true;
    error = null;
    try {
      const chosen = (Object.keys(scopes) as Scope[]).filter((s) => scopes[s]);
      await api.connectIntegration(integration.target, chosen, limitProjects ? projects : []);
      name = integration.name;
      await load();
      view = 'connected';
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = null;
    try {
      const chosen = (Object.keys(scopes) as Scope[]).filter((s) => scopes[s]);
      const created = await api.createClient(name, chosen, limitProjects ? projects : []);
      token = created.token;
      copied = false;
      await load();
      view = 'created';
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }

  async function revoke(c: AiClient) {
    try {
      await api.revokeClient(c.id);
      confirmRevoke = null;
      await load();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  const snippet = $derived.by(() => {
    if (!info) return { where: '', text: '' };
    const { url, bridge_path: bridge } = info;
    switch (preset) {
      case 'Claude Code':
        return {
          where: t('Im Terminal ausführen', 'Run in the terminal'),
          text: `claude mcp add --transport http --scope user brainmatter ${url} \\\n  --header "Authorization: Bearer ${token}"`,
        };
      case 'Cursor':
        return {
          where: t('In ~/.cursor/mcp.json eintragen', 'Add to ~/.cursor/mcp.json'),
          text: JSON.stringify({ mcpServers: { brainmatter: { url, headers: { Authorization: `Bearer ${token}` } } } }, null, 2),
        };
      case 'Claude Desktop':
        return {
          where: t('In claude_desktop_config.json eintragen', 'Add to claude_desktop_config.json'),
          text: JSON.stringify({ mcpServers: { brainmatter: { command: bridge, env: { BRAINMATTER_TOKEN: token, BRAINMATTER_URL: url } } } }, null, 2),
        };
      case 'Codex':
        return {
          where: t('In ~/.codex/config.toml eintragen', 'Add to ~/.codex/config.toml'),
          text: `[mcp_servers.brainmatter]\ncommand = "${bridge}"\nenv = { BRAINMATTER_TOKEN = "${token}", BRAINMATTER_URL = "${url}" }`,
        };
      default:
        return { where: 'MCP-Server (Streamable HTTP)', text: `URL:    ${url}\nHeader: Authorization: Bearer ${token}` };
    }
  });
  const usesBridge = $derived(preset === 'Claude Desktop' || preset === 'Codex');

  async function copy() {
    await navigator.clipboard.writeText(snippet.text);
    copied = true;
    setTimeout(() => (copied = false), 1600);
  }

  function seen(ms: number | null) {
    if (!ms) return t('Noch nie verbunden', 'Never connected');
    const s = Math.round((Date.now() - ms) / 1000);
    if (s < 120) return t('Gerade aktiv', 'Active now');
    if (s < 3600) return t(`Zuletzt vor ${Math.round(s / 60)} Min.`, `Last seen ${Math.round(s / 60)} min ago`);
    if (s < 86_400) return t(`Zuletzt vor ${Math.round(s / 3600)} Std.`, `Last seen ${Math.round(s / 3600)} h ago`);
    const day = new Date(ms).toLocaleDateString(locale);
    return t(`Zuletzt am ${day}`, `Last seen on ${day}`);
  }

  const SEARCH_FROM = 8;
  let projectQuery = $state('');
  const projectName = (id: string) => ws.projects.find((p) => p.project.id === id)?.project.name ?? '?';
</script>

{#if view === 'list'}
  <h2 class="mb-1 text-lg font-semibold">{t('KI-Verbindungen', 'AI connections')}</h2>
  <p class="mb-4 text-muted">{t('Jede KI bekommt ein eigenes Token, eigene Rechte und ist im Verlauf erkennbar.', 'Each AI client gets its own token and its own permissions, and you can tell it apart in the history.')}</p>

  {#if info}
    <div class="mb-4 flex items-center gap-2 text-xs">
      <span class="h-2 w-2 rounded-full {info.running ? 'bg-ok' : 'bg-danger'}"></span>
      {#if info.running}
        <span class="text-muted">{t('Server läuft', 'Server running')}</span><code class="selectable font-mono text-faint">{info.url}</code>
      {:else}
        <span class="text-danger">{t('Server konnte nicht starten:', 'The server could not start:')} {info.error}</span>
      {/if}
    </div>

    {#if info.clients.length}
      <div class="card mb-4">
        {#each info.clients as c (c.id)}
          <div class="row group" animate:flip={{ duration: 180 }}>
            <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full" style={avatarStyle(tint(c.name))}><Bot size={16} /></div>
            <div class="min-w-0 flex-1">
              <div class="truncate font-medium">{c.name}</div>
              <div class="truncate text-xs text-faint">
                {c.scopes.map((s) => SCOPE_LABEL[s]).join(', ')}
                · {c.projects.length ? c.projects.map(projectName).join(', ') : t('Alle Projekte', 'All projects')}
                · {seen(c.last_seen_at)}
              </div>
            </div>
            {#if confirmRevoke === c.id}
              <button class="btn-danger" in:pop={{ duration: 120, y: 0 }} onclick={() => revoke(c)} onblur={() => (confirmRevoke = null)}>{t('Wirklich trennen?', 'Really disconnect?')}</button>
            {:else}
              <button class="icon-btn opacity-0 group-hover:opacity-100 hover:!text-danger focus-visible:opacity-100"
                aria-label={t(`Verbindung von ${c.name} trennen`, `Disconnect ${c.name}`)} title={t('Verbindung trennen', 'Disconnect')} onclick={() => (confirmRevoke = c.id)}><Trash2 size={14} /></button>
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="mb-4 flex flex-col items-center rounded-xl border border-dashed border-line px-6 py-8 text-center">
        <div class="mb-3 flex h-11 w-11 items-center justify-center text-faint"><Bot size={20} /></div>
        <p class="font-medium">{t('Noch keine KI verbunden', 'No AI client connected yet')}</p>
        <p class="mt-1 text-xs text-faint">{t('Verbinde Claude, Cursor oder Codex, damit sie dein Wissen lesen und erweitern können.', 'Connect Claude, Cursor or Codex so they can read and extend what you know.')}</p>
      </div>
    {/if}
    <button class="btn-primary" onclick={start}><Plus size={15} /> {t('KI verbinden', 'Connect AI')}</button>
  {/if}
  {#if error}<p class="mt-3 text-danger">{error}</p>{/if}
{:else if view === 'new'}
  <button class="mb-3 -ml-1 flex items-center gap-1 text-muted transition-colors hover:text-text" onclick={() => (view = 'list')}><ArrowLeft size={14} /> {t('Zurück', 'Back')}</button>
  <h2 class="mb-4 text-lg font-semibold">{t('KI verbinden', 'Connect AI')}</h2>

  <form onsubmit={create}>
    <div class="section-title">{t('Welche KI?', 'Which AI?')}</div>
    <div class="mb-3 grid grid-cols-5 gap-1.5">
      {#each PRESETS as p (p)}
        {@const hint = presetHint(p)}
        <button type="button"
          class="flex h-16 flex-col items-center justify-center gap-1 rounded-xl border px-1 text-center transition-[background-color,border-color] duration-150 {preset === p ? 'border-accent bg-accent/12' : 'border-line-soft bg-elevated/50 hover:border-line hover:bg-elevated'}"
          aria-pressed={preset === p}
          onclick={() => choose(p)}>
          <span class="text-xs leading-tight font-medium">{p === 'Andere' ? t('Andere', 'Other') : p}</span>
          <span class="flex items-center gap-1 text-[10.5px] {hint === 'missing' ? 'text-faint' : 'text-muted'}">
            {#if hint === 'found' || hint === 'connected'}<span class="h-1.5 w-1.5 rounded-full {hint === 'connected' ? 'bg-accent' : 'bg-ok'}"></span>{/if}{FOUND_LABEL[hint]}
          </span>
        </button>
      {/each}
    </div>
    {#if !integration?.available}
      <input class="field mb-2 w-full" placeholder={t('Name', 'Name')} aria-label={t('Name', 'Name')} bind:value={name} />
    {/if}
    {#if integration?.configured}
      <p class="mb-2 text-xs text-muted" in:pop={{ duration: 140, y: -3 }}>{t('Schon verbunden. Neu einrichten ersetzt das alte Token.', 'Already connected. Setting it up again replaces the old token.')}</p>
    {:else if integration && !integration.available}
      <p class="mb-2 text-xs text-muted" in:pop={{ duration: 140, y: -3 }}>{integration.unavailable_reason} {t('Du kannst die Verbindung trotzdem manuell einrichten.', 'You can still set up the connection by hand.')}</p>
    {/if}
    <div class="mb-5"></div>

    <div class="section-title">{t('Was darf sie?', 'What may it do?')}</div>
    <div class="card mb-5">
      {#each SCOPES as s (s.scope)}
        <div class="row">
          <div class="flex-1">
            <div>{s.label}</div>
            <div class="text-xs text-faint">{s.hint}</div>
          </div>
          <Toggle bind:checked={scopes[s.scope]} label={s.label} />
        </div>
      {/each}
      <div class="row">
        <div class="flex-1">
          <div>{t('Nur bestimmte Projekte', 'Only certain projects')}</div>
          <div class="text-xs text-faint">{t('Alles andere bleibt für diese KI unsichtbar', 'Everything else stays invisible to this AI client')}</div>
        </div>
        <Toggle bind:checked={limitProjects} label={t('Nur bestimmte Projekte', 'Only certain projects')} />
      </div>
      {#if limitProjects}
        <div class="flex max-h-64 flex-col gap-2 overflow-y-auto border-t border-line-soft px-4 py-3" in:pop={{ duration: 160, y: -3 }}>
          {#if ws.projects.length > SEARCH_FROM}
            <input class="field !h-7.5 w-full text-xs" placeholder={t('Projekt suchen', 'Search projects')} aria-label={t('Projekt suchen', 'Search projects')} bind:value={projectQuery} />
          {/if}
          {#each ws.projects.filter((p) => p.project.name.toLowerCase().includes(projectQuery.trim().toLowerCase())) as p (p.project.id)}
            <Checkbox
              checked={projects.includes(p.project.id)}
              onchange={(on) => (projects = on ? [...projects, p.project.id] : projects.filter((id) => id !== p.project.id))}>
              {p.project.name}
            </Checkbox>
          {:else}
            <p class="text-xs text-faint">{t('Noch keine Projekte vorhanden.', 'No projects yet.')}</p>
          {/each}
        </div>
      {/if}
    </div>

    {#if error}<p class="mb-3 leading-relaxed text-danger" in:pop={{ duration: 160, y: -3 }}>{error}</p>{/if}
    <div class="flex items-center gap-2">
      {#if integration?.available}
        <button type="button" class="btn-primary" disabled={busy || invalid} onclick={connect}>
          <Plug size={15} /> {busy ? t('Wird eingerichtet …', 'Setting up …') : t('Automatisch einrichten', 'Set up automatically')}
        </button>
        <button class="btn-ghost" disabled={busy || invalid}>{t('Manuell einrichten', 'Set up by hand')}</button>
      {:else}
        <button class="btn-primary" disabled={busy || invalid || !name.trim()}>{t('Token erstellen', 'Create token')}</button>
      {/if}
    </div>
  </form>
{:else if view === 'connected'}
  <div class="flex flex-col items-center pt-12 text-center" in:pop={{ duration: 280, y: 8 }}>
    <div class="mb-4 flex h-14 w-14 items-center justify-center rounded-full bg-ok/15 text-ok"><Check size={28} /></div>
    <h2 class="text-lg font-semibold">{t(`${name} ist verbunden`, `${name} is connected`)}</h2>
    <p class="mt-2 mb-6 max-w-sm leading-relaxed text-muted">
      {t(
        `Starte ${name} einmal neu, dann steht Brainmatter dort zur Verfügung. Ist die App gesperrt, fragt Touch ID nach, bevor die KI zugreifen darf.`,
        `Restart ${name} once, and Brainmatter is there. While the app is locked, Touch ID asks before the AI client gets in.`,
      )}
    </p>
    <button class="btn-primary" onclick={() => (view = 'list')}>{t('Fertig', 'Done')}</button>
  </div>
{:else}
  <h2 class="mb-1 text-lg font-semibold">{t(`${name} verbinden`, `Connect ${name}`)}</h2>
  <p class="mb-5 text-muted"><strong class="font-medium text-warn">{t('Nur jetzt sichtbar.', 'Visible only now.')}</strong> {t('Das Token wird nicht gespeichert und lässt sich später nicht mehr anzeigen.', 'The token is not stored and cannot be shown again later.')}</p>

  <div class="section-title">{snippet.where}</div>
  <div class="relative mb-3">
    <pre class="selectable overflow-x-auto rounded-xl border border-line bg-bg/70 p-3.5 pr-12 font-mono text-[11.5px] leading-relaxed whitespace-pre">{snippet.text}</pre>
    <button class="icon-btn absolute top-2 right-2 !bg-raised" aria-label={t('Kopieren', 'Copy')} title={t('Kopieren', 'Copy')} onclick={copy}>
      {#if copied}<Check size={14} class="text-ok" />{:else}<Copy size={14} />{/if}
    </button>
  </div>
  {#if usesBridge && info && !info.bridge_exists}
    <p class="mb-3 rounded-lg bg-warn/10 px-3 py-2 text-xs text-warn">
      {t('Die Bridge ist in diesem Dev-Build noch nicht gebaut. Führe', 'The bridge is not built yet in this dev build. Run')} <code class="font-mono">cargo build -p bm-cli</code>{t(' aus.', '.')}
    </p>
  {/if}
  <p class="mb-5 text-xs leading-relaxed text-faint">
    {t(
      'Danach die KI neu starten. Sie findet Brainmatter dann als MCP-Server. Ist die App gesperrt, fragt Touch ID nach, bevor sie zugreifen darf.',
      'Then restart the AI client. It finds Brainmatter as an MCP server. While the app is locked, Touch ID asks before it gets in.',
    )}
  </p>
  <button class="btn-primary" onclick={() => ((token = ''), (view = 'list'))}>{t('Fertig', 'Done')}</button>
{/if}
