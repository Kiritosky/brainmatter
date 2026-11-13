<script lang="ts">
  import { locale, t } from '#lib/i18n.ts';
  import { onMount } from 'svelte';
  import { flip } from 'svelte/animate';
  import { ArrowLeft, Bot, Check, Copy, Plus, Plug, Trash2 } from '@lucide/svelte';
  import { api, errorMessage, type AiClient, type Integration, type McpInfo, type Scope } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import Toggle from '#lib/ui/Toggle.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import Segmented from '#lib/ui/Segmented.svelte';
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
  /** Days until the token stops working; `never` for one that lasts. Only for hand-made connections. */
  type Lifetime = 'never' | '1' | '7' | '30';
  let lifetime = $state<Lifetime>('never');
  const LIFETIMES: { value: Lifetime; label: string }[] = [
    { value: 'never', label: t('Unbegrenzt', 'No limit') },
    { value: '1', label: t('1 Tag', '1 day') },
    { value: '7', label: t('7 Tage', '7 days') },
    { value: '30', label: t('30 Tage', '30 days') },
  ];

  let memory = $state(true);
  let skillMenu = $state(true);

  let token = $state('');
  let copied = $state(false);
  let ruleCopied = $state(false);

  onMount(load);

  async function load() {
    try {
      info = await api.mcpInfo();
      integrations = await api.integrations();
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
    lifetime = 'never';
    memory = true;
    skillMenu = true;
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
      await api.connectIntegration(integration.target, chosen, limitProjects ? projects : [], memory, skillMenu && !!integration.skills_dir);
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
      const days = lifetime === 'never' ? null : Number(lifetime);
      const created = await api.createClient(name, chosen, limitProjects ? projects : [], days);
      token = created.token;
      copied = false;
      ruleCopied = false;
      await load();
      view = 'created';
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }

  async function setRule(i: Integration, on: boolean) {
    error = null;
    try {
      await api.setMemoryRule(i.target, on);
    } catch (e) {
      error = errorMessage(e);
    }
    integrations = await api.integrations().catch(() => integrations);
  }

  async function setSkillMenu(i: Integration, on: boolean) {
    error = null;
    try {
      await api.setSkillMenu(i.target, on);
    } catch (e) {
      error = errorMessage(e);
    }
    integrations = await api.integrations().catch(() => integrations);
  }

  /** What a tool finds in its skill folder, such as „14 Befehle und Skills in ~/.claude/skills“. */
  function menuStatus(i: Integration) {
    if (!i.skills) return t(`Noch nichts in ${i.skills_dir}`, `Nothing in ${i.skills_dir} yet`);
    if (i.skills === 1) return t(`1 Befehl oder Skill in ${i.skills_dir}`, `1 command or skill in ${i.skills_dir}`);
    return t(`${i.skills} Befehle und Skills in ${i.skills_dir}`, `${i.skills} commands and skills in ${i.skills_dir}`);
  }
  const MENU_WHY = t('Für Apps, die Skills zeigen, MCP-Befehle aber nicht.', 'For apps that show skills but not MCP commands.');
  const MENU_PRIVACY = t(
    'Namen und Beschreibungen liegen dort unverschlüsselt, Skills mit „Direkt als Dateien“ ganz.',
    'Names and descriptions are stored there unencrypted, and skills set to “Directly as files” in full.',
  );

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

  /** What makes the AI use Brainmatter unasked; without write access it only looks things up. */
  const rule = $derived(info ? (scopes.write ? info.rule : info.rule_read_only) : '');
  const ruleTitle = $derived(scopes.write ? t('Von selbst nachschlagen und sichern', 'Look up and save unasked') : t('Von selbst nachschlagen', 'Look things up unasked'));
  /** Written for it, unless it keeps its instructions where Brainmatter cannot reach. */
  const ruleWritten = $derived(!!integration?.available && !!integration.rule_file);
  const ruleWhere = $derived.by(() => {
    if (preset === 'Cursor') return t('In Cursor unter Customize → Rules als User Rule einfügen', 'In Cursor, add it as a user rule under Customize → Rules');
    if (preset === 'Claude Desktop') return t('In Claude bei den persönlichen Präferenzen in den Einstellungen einfügen', "Paste it into the personal preferences in Claude's settings");
    if (integration?.rule_file) return t(`In ${integration.rule_file} einfügen`, `Add it to ${integration.rule_file}`);
    return t('In die Anweisungen oder Regeln der KI einfügen', "Paste it into the AI client's instructions or rules");
  });

  async function copyRule() {
    await navigator.clipboard.writeText(rule);
    ruleCopied = true;
    setTimeout(() => (ruleCopied = false), 1600);
  }

  /** `läuft in 3 Tagen ab`, or `abgelaufen` once the token no longer works. */
  function expiry(at: number): { text: string; expired: boolean } {
    const left = at - Date.now();
    if (left <= 0) return { text: t('abgelaufen', 'expired'), expired: true };
    const hours = Math.ceil(left / 3_600_000);
    if (hours < 24) return { text: t(`läuft in ${hours} Std. ab`, `expires in ${hours} h`), expired: false };
    const days = Math.ceil(left / 86_400_000);
    return { text: days === 1 ? t('läuft morgen ab', 'expires tomorrow') : t(`läuft in ${days} Tagen ab`, `expires in ${days} days`), expired: false };
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

{#snippet copyBlock(text: string, done: boolean, onCopy: () => void, wrap = false)}
  <div class="relative mb-3">
    <pre class="selectable overflow-x-auto rounded-xl border border-line bg-bg/70 p-3.5 pr-12 text-left font-mono text-[11.5px] leading-relaxed {wrap ? 'whitespace-pre-wrap' : 'whitespace-pre'}">{text}</pre>
    <button class="icon-btn absolute top-2 right-2 !bg-raised" aria-label={t('Kopieren', 'Copy')} title={t('Kopieren', 'Copy')} onclick={onCopy}>
      {#if done}<Check size={14} class="text-ok" />{:else}<Copy size={14} />{/if}
    </button>
  </div>
{/snippet}

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
          {@const ig = integrations.find((i) => i.name === c.name && i.rule_file)}
          {@const menu = integrations.find((i) => i.name === c.name && i.skills_dir)}
          <div class="row group" animate:flip={{ duration: 180 }}>
            <div class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full" style={avatarStyle(tint(c.name))}><Bot size={16} /></div>
            <div class="min-w-0 flex-1">
              <div class="truncate font-medium">{c.name}</div>
              <div class="truncate text-xs text-faint">
                {c.scopes.map((s) => SCOPE_LABEL[s]).join(', ')}
                · {c.projects.length ? c.projects.map(projectName).join(', ') : t('Alle Projekte', 'All projects')}
                · {seen(c.last_seen_at)}
                {#if c.expires_at}
                  {@const e = expiry(c.expires_at)}
                  · <span class={e.expired ? 'text-danger' : ''}>{e.text}</span>
                {/if}
              </div>
              {#if menu?.skill_menu}
                <div class="truncate text-xs text-faint" in:pop={{ duration: 140, y: -2 }}>{menuStatus(menu)}</div>
              {/if}
            </div>
            {#if (ig || menu) && confirmRevoke !== c.id}
              <div class="flex shrink-0 flex-col items-end gap-1.5 text-xs text-muted">
                {#if ig}
                  <div class="flex items-center gap-2" title={t(`Regel in ${ig.rule_file}`, `Rule in ${ig.rule_file}`)}>
                    {t('Gedächtnis', 'Memory')}
                    <Toggle checked={ig.rule} label={t(`${c.name} nutzt Brainmatter von selbst`, `${c.name} uses Brainmatter unasked`)} onchange={(on) => setRule(ig, on)} />
                  </div>
                {/if}
                {#if menu}
                  <div class="flex items-center gap-2" title={MENU_WHY}>
                    {t('Skill-Menü', 'Skill menu')}
                    <Toggle checked={menu.skill_menu} label={t(`Befehle und Skills im Skill-Menü von ${c.name}`, `Commands and skills in the skill menu of ${c.name}`)} onchange={(on) => setSkillMenu(menu, on)} />
                  </div>
                {/if}
              </div>
            {/if}
            {#if confirmRevoke === c.id}
              <button class="btn-danger" in:pop={{ duration: 120, y: 0 }} onclick={() => revoke(c)} onblur={() => (confirmRevoke = null)}>{t('Wirklich trennen?', 'Really disconnect?')}</button>
            {:else}
              <button class="icon-btn opacity-0 group-hover:opacity-100 hover:!text-danger focus-visible:opacity-100"
                aria-label={t(`Verbindung von ${c.name} trennen`, `Disconnect ${c.name}`)} title={t('Verbindung trennen', 'Disconnect')} onclick={() => (confirmRevoke = c.id)}><Trash2 size={14} /></button>
            {/if}
          </div>
        {/each}
      </div>
      {#if integrations.some((i) => i.configured && i.skills_dir)}
        <p class="-mt-2 mb-4 text-xs leading-relaxed text-faint">{t('Das Skill-Menü legt Brainmatters Befehle und Skills als Skills in den Ordner der App.', "The skill menu puts Brainmatter's commands and skills into the app's skill folder.")} {MENU_WHY} {MENU_PRIVACY}</p>
      {/if}
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
      {#if !integration?.available}
        <div class="row">
          <div class="flex-1">
            <div>{t('Gültig', 'Valid for')}</div>
            <div class="text-xs text-faint">{t('Danach weist Brainmatter das Token ab, etwa nach einem Test', 'After that, Brainmatter turns the token away, say after a trial')}</div>
          </div>
          <div class="w-64 shrink-0"><Segmented bind:value={lifetime} options={LIFETIMES} label={t('Gültig', 'Valid for')} compact /></div>
        </div>
      {/if}
    </div>

    <div class="section-title">{t('Gedächtnis', 'Memory')}</div>
    <div class="card mb-5">
      <div class="row">
        <div class="flex-1">
          <div>{ruleTitle}</div>
          <div class="text-xs text-faint">
            {#if ruleWritten}
              {t(`Schreibt eine kurze Regel nach ${integration?.rule_file}, damit ${integration?.name} Brainmatter vor jeder Aufgabe nutzt.`, `Writes a short rule to ${integration?.rule_file}, so that ${integration?.name} turns to Brainmatter before every task.`)}
            {:else}
              {t('Du bekommst eine kurze Regel für die Anweisungen der KI, damit sie Brainmatter vor jeder Aufgabe nutzt.', "You get a short rule for the AI client's instructions, so that it turns to Brainmatter before every task.")}
            {/if}
          </div>
        </div>
        <Toggle bind:checked={memory} label={ruleTitle} />
      </div>
    </div>

    {#if integration?.available && integration.skills_dir}
      <div class="section-title" in:pop={{ duration: 140, y: -3 }}>{t('Im Skill-Menü', 'In the skill menu')}</div>
      <div class="card mb-5" in:pop={{ duration: 140, y: -3 }}>
        <div class="row">
          <div class="flex-1">
            <div>{t('Befehle und Skills als Skills', 'Commands and skills as skills')}</div>
            <div class="text-xs text-faint">
              {t(`Legt sie nach ${integration.skills_dir}, so wie ${integration.name} sie sehen darf.`, `Puts them into ${integration.skills_dir}, as far as ${integration.name} may see them.`)}
              {MENU_WHY} {MENU_PRIVACY}
            </div>
          </div>
          <Toggle bind:checked={skillMenu} label={t('Im Skill-Menü', 'In the skill menu')} />
        </div>
      </div>
    {/if}

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
        `Restart ${name} once, and Brainmatter is there. While the app is locked, you confirm with Touch ID before the AI client can read anything.`,
      )}
    </p>
    {#if integration?.skill_menu}
      <p class="-mt-3 mb-4 flex items-center gap-1.5 text-xs text-faint"><Check size={13} class="text-ok" /> {menuStatus(integration)}</p>
    {/if}
    {#if memory && integration?.rule}
      <p class="-mt-3 mb-6 flex items-center gap-1.5 text-xs text-faint"><Check size={13} class="text-ok" /> {t(`Die Regel steht in ${integration.rule_file}.`, `The rule is in ${integration.rule_file}.`)}</p>
    {:else if memory && integration?.rule_file}
      <p class="-mt-3 mb-6 max-w-sm text-xs text-warn">{t(`Die Regel ließ sich nicht nach ${integration.rule_file} schreiben. Du kannst sie unter KI-Verbindungen noch einmal einschalten.`, `The rule could not be written to ${integration.rule_file}. You can switch it on again under AI connections.`)}</p>
    {:else if memory}
      <div class="mb-4 w-full max-w-md">
        <div class="section-title text-left">{ruleWhere}</div>
        {@render copyBlock(rule, ruleCopied, copyRule, true)}
      </div>
    {/if}
    <button class="btn-primary" onclick={() => (view = 'list')}>{t('Fertig', 'Done')}</button>
  </div>
{:else}
  <h2 class="mb-1 text-lg font-semibold">{t(`${name} verbinden`, `Connect ${name}`)}</h2>
  <p class="mb-5 text-muted"><strong class="font-medium text-warn">{t('Nur jetzt sichtbar.', 'Visible only now.')}</strong> {t('Das Token wird nicht gespeichert und lässt sich später nicht mehr anzeigen.', 'The token is not stored and cannot be shown again later.')}</p>

  <div class="section-title">{snippet.where}</div>
  {@render copyBlock(snippet.text, copied, copy)}
  {#if memory}
    <div class="section-title mt-5">{ruleWhere}</div>
    {@render copyBlock(rule, ruleCopied, copyRule, true)}
  {/if}
  {#if usesBridge && info && !info.bridge_exists}
    <p class="mb-3 rounded-lg bg-warn/10 px-3 py-2 text-xs text-warn">
      {t('Die Bridge ist in diesem Dev-Build noch nicht gebaut. Führe', 'The bridge is not built yet in this dev build. Run')} <code class="font-mono">cargo build -p bm-cli</code>{t(' aus.', '.')}
    </p>
  {/if}
  <p class="mb-5 text-xs leading-relaxed text-faint">
    {t(
      'Danach die KI neu starten. Sie findet Brainmatter dann als MCP-Server. Ist die App gesperrt, fragt Touch ID nach, bevor sie zugreifen darf.',
      'Then restart the AI client. It finds Brainmatter as an MCP server. While the app is locked, you confirm with Touch ID before it can read anything.',
    )}
  </p>
  <button class="btn-primary" onclick={() => ((token = ''), (view = 'list'))}>{t('Fertig', 'Done')}</button>
{/if}
