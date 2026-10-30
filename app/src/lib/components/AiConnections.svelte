<script lang="ts">
  import { onMount } from 'svelte';
  import { flip } from 'svelte/animate';
  import { ArrowLeft, Bot, Check, Copy, Plus, Trash2 } from '@lucide/svelte';
  import { api, errorMessage, type AiClient, type McpInfo, type Scope } from '#lib/api.ts';
  import { ws } from '#lib/workspace.svelte.ts';
  import Toggle from '#lib/ui/Toggle.svelte';
  import Checkbox from '#lib/ui/Checkbox.svelte';
  import { pop } from '#lib/ui/motion.ts';
  import { avatarStyle, tint } from '#lib/ui/tint.ts';

  type Preset = 'Claude Code' | 'Claude Desktop' | 'Cursor' | 'Codex' | 'Andere';
  const PRESETS: { name: Preset; hint: string }[] = [
    { name: 'Claude Code', hint: 'Terminal' },
    { name: 'Claude Desktop', hint: 'App' },
    { name: 'Cursor', hint: 'Editor' },
    { name: 'Codex', hint: 'Terminal' },
    { name: 'Andere', hint: 'Eigene' },
  ];
  const SCOPES: { scope: Scope; label: string; hint: string }[] = [
    { scope: 'read', label: 'Lesen', hint: 'Suchen und Einträge lesen' },
    { scope: 'write', label: 'Schreiben', hint: 'Einträge und Verbindungen anlegen und ändern' },
    { scope: 'delete', label: 'Löschen', hint: 'Einträge in den Papierkorb verschieben' },
  ];
  const SCOPE_LABEL: Record<Scope, string> = { read: 'Lesen', write: 'Schreiben', delete: 'Löschen' };

  let info = $state<McpInfo | null>(null);
  let view = $state<'list' | 'new' | 'created'>('list');
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

  function start() {
    preset = 'Claude Code';
    name = uniqueName('Claude Code');
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
          where: 'Im Terminal ausführen',
          text: `claude mcp add --transport http --scope user brainmatter ${url} \\\n  --header "Authorization: Bearer ${token}"`,
        };
      case 'Cursor':
        return {
          where: 'In ~/.cursor/mcp.json eintragen',
          text: JSON.stringify({ mcpServers: { brainmatter: { url, headers: { Authorization: `Bearer ${token}` } } } }, null, 2),
        };
      case 'Claude Desktop':
        return {
          where: 'In claude_desktop_config.json eintragen',
          text: JSON.stringify({ mcpServers: { brainmatter: { command: bridge, env: { BRAINMATTER_TOKEN: token, BRAINMATTER_URL: url } } } }, null, 2),
        };
      case 'Codex':
        return {
          where: 'In ~/.codex/config.toml eintragen',
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
    if (!ms) return 'Noch nie verbunden';
    const s = Math.round((Date.now() - ms) / 1000);
    if (s < 120) return 'Gerade aktiv';
    if (s < 3600) return `Zuletzt vor ${Math.round(s / 60)} Min.`;
    if (s < 86_400) return `Zuletzt vor ${Math.round(s / 3600)} Std.`;
    return `Zuletzt am ${new Date(ms).toLocaleDateString('de-DE')}`;
  }

  const projectName = (id: string) => ws.projects.find((p) => p.project.id === id)?.project.name ?? '?';
</script>

{#if view === 'list'}
  <h2 class="mb-1 text-lg font-semibold">KI-Verbindungen</h2>
  <p class="mb-4 text-muted">Jede KI bekommt ein eigenes Token, eigene Rechte und ist im Verlauf erkennbar.</p>

  {#if info}
    <div class="mb-4 flex items-center gap-2 text-xs">
      <span class="h-2 w-2 rounded-full {info.running ? 'bg-ok' : 'bg-danger'}"></span>
      {#if info.running}
        <span class="text-muted">Server läuft</span><code class="selectable font-mono text-faint">{info.url}</code>
      {:else}
        <span class="text-danger">Server konnte nicht starten: {info.error}</span>
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
                · {c.projects.length ? c.projects.map(projectName).join(', ') : 'Alle Projekte'}
                · {seen(c.last_seen_at)}
              </div>
            </div>
            {#if confirmRevoke === c.id}
              <button class="btn-danger" in:pop={{ duration: 120, y: 0 }} onclick={() => revoke(c)} onblur={() => (confirmRevoke = null)}>Wirklich trennen?</button>
            {:else}
              <button class="icon-btn opacity-0 group-hover:opacity-100 hover:!text-danger focus-visible:opacity-100"
                aria-label="Verbindung von {c.name} trennen" title="Verbindung trennen" onclick={() => (confirmRevoke = c.id)}><Trash2 size={14} /></button>
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="mb-4 flex flex-col items-center rounded-xl border border-dashed border-line px-6 py-8 text-center">
        <div class="mb-3 flex h-11 w-11 items-center justify-center rounded-full bg-accent/12 text-accent"><Bot size={20} /></div>
        <p class="font-medium">Noch keine KI verbunden</p>
        <p class="mt-1 text-xs text-faint">Verbinde Claude, Cursor oder Codex, damit sie dein Wissen lesen und erweitern können.</p>
      </div>
    {/if}
    <button class="btn-primary" onclick={start}><Plus size={15} /> KI verbinden</button>
  {/if}
  {#if error}<p class="mt-3 text-danger">{error}</p>{/if}
{:else if view === 'new'}
  <button class="mb-3 -ml-1 flex items-center gap-1 text-muted transition-colors hover:text-text" onclick={() => (view = 'list')}><ArrowLeft size={14} /> Zurück</button>
  <h2 class="mb-4 text-lg font-semibold">KI verbinden</h2>

  <form onsubmit={create}>
    <div class="section-title">Welche KI?</div>
    <div class="mb-3 grid grid-cols-5 gap-1.5">
      {#each PRESETS as p (p.name)}
        <button type="button"
          class="flex h-16 flex-col items-center justify-center gap-0.5 rounded-xl border px-1 text-center transition-[background-color,border-color,scale] duration-150 active:scale-[0.97] {preset === p.name ? 'border-accent bg-accent/12' : 'border-line-soft bg-elevated/50 hover:border-line hover:bg-elevated'}"
          aria-pressed={preset === p.name}
          onclick={() => choose(p.name)}>
          <span class="text-xs leading-tight font-medium">{p.name}</span>
          <span class="text-[10.5px] text-faint">{p.hint}</span>
        </button>
      {/each}
    </div>
    <input class="field mb-5 w-full" placeholder="Name" aria-label="Name" bind:value={name} />

    <div class="section-title">Was darf sie?</div>
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
          <div>Nur bestimmte Projekte</div>
          <div class="text-xs text-faint">Alles andere bleibt für diese KI unsichtbar</div>
        </div>
        <Toggle bind:checked={limitProjects} label="Nur bestimmte Projekte" />
      </div>
      {#if limitProjects}
        <div class="flex flex-col gap-2 border-t border-line-soft px-4 py-3" in:pop={{ duration: 160, y: -3 }}>
          {#each ws.projects as p (p.project.id)}
            <Checkbox
              checked={projects.includes(p.project.id)}
              onchange={(on) => (projects = on ? [...projects, p.project.id] : projects.filter((id) => id !== p.project.id))}>
              {p.project.name}
            </Checkbox>
          {:else}
            <p class="text-xs text-faint">Noch keine Projekte vorhanden.</p>
          {/each}
        </div>
      {/if}
    </div>

    {#if error}<p class="mb-3 text-danger" in:pop={{ duration: 160, y: -3 }}>{error}</p>{/if}
    <button class="btn-primary" disabled={busy || !name.trim() || !Object.values(scopes).some(Boolean) || (limitProjects && !projects.length)}>
      Token erstellen
    </button>
  </form>
{:else}
  <h2 class="mb-1 text-lg font-semibold">{name} verbinden</h2>
  <p class="mb-5 text-muted"><strong class="font-medium text-warn">Nur jetzt sichtbar.</strong> Das Token wird nicht gespeichert und lässt sich später nicht mehr anzeigen.</p>

  <div class="section-title">{snippet.where}</div>
  <div class="relative mb-3">
    <pre class="selectable overflow-x-auto rounded-xl border border-line bg-bg/70 p-3.5 pr-12 font-mono text-[11.5px] leading-relaxed whitespace-pre">{snippet.text}</pre>
    <button class="icon-btn absolute top-2 right-2 !bg-raised" aria-label="Kopieren" title="Kopieren" onclick={copy}>
      {#if copied}<Check size={14} class="text-ok" />{:else}<Copy size={14} />{/if}
    </button>
  </div>
  {#if usesBridge && info && !info.bridge_exists}
    <p class="mb-3 rounded-lg bg-warn/10 px-3 py-2 text-xs text-warn">
      Die Bridge ist in diesem Dev-Build noch nicht gebaut. Führe <code class="font-mono">cargo build -p bm-cli</code> aus.
    </p>
  {/if}
  <p class="mb-5 text-xs leading-relaxed text-faint">
    Danach die KI neu starten. Sie findet Brainmatter dann als MCP-Server. Ist die App gesperrt, fragt Touch ID nach, bevor sie zugreifen darf.
  </p>
  <button class="btn-primary" onclick={() => ((token = ''), (view = 'list'))}>Fertig</button>
{/if}
