// In-memory stand-in for the Rust backend, used when the UI runs in a plain
// browser (`bun run dev`, Playwright). Loaded only in dev builds outside Tauri.
// `?mock=fresh` starts without a vault to exercise onboarding.
import { mockIPC } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import type {
  AiClient,
  ChangeSetInfo,
  DomainEvent,
  Edge,
  GraphSnapshot,
  Node,
  NodeDetail,
  Project,
  ProjectSummary,
  SearchHit,
  Settings,
  Status,
} from './api';

type Args = Record<string, any>;

const USER = 'actor-user';
let seq = 0;
const id = () => `00000000-0000-7000-8000-${(++seq).toString(16).padStart(12, '0')}`;
const now = () => Date.now();

const nodes = new Map<string, Node>();
const edges = new Map<string, Edge>();
const projects = new Map<string, Project>();
const members = new Set<string>(); // `${project}|${node}`
const layouts = new Map<string, { x: number; y: number; pinned: boolean }>(); // `${view}|${node}`
const changeSets: ChangeSetInfo[] = [];
const touched = new Map<string, string[]>(); // change set → node ids
const clients: AiClient[] = [];

let state: Status['state'] = new URLSearchParams(location.search).get('mock') === 'fresh' ? 'none' : 'locked';
let touchId = state === 'locked';
let userName = 'Lasse';
let settings: Settings = { auto_lock_minutes: 60, lock_on_sleep: true, lock_on_screen_lock: false };

const status = (): Status => ({
  state,
  touch_id_enrolled: touchId,
  biometry_available: true,
  user_name: state === 'unlocked' ? userName : null,
  settings,
});

function fail(code: string, message: string): never {
  throw { code, message };
}

function notify(...events: DomainEvent[]) {
  setTimeout(() => events.forEach((e) => emit('bm:event', e)), 30);
}

function record(
  summary: string,
  nodeIds: string[] = [],
  opts: { actor?: string; client?: string; ago?: number; reverted?: boolean } = {},
) {
  // Like the engine: repeated edits of one entry are one change set.
  const last = changeSets[0];
  if (last && !opts.ago && !last.reverted_by && last.summary === summary && summary.endsWith('bearbeitet') && last.client === (opts.client ?? null)) {
    last.at = now();
    return;
  }
  const cs: ChangeSetInfo = {
    id: id(),
    actor: opts.actor ? `actor-${opts.actor}` : USER,
    actor_name: opts.actor ?? userName,
    client: opts.client ?? null,
    tool: opts.client ? 'capture' : 'app',
    summary,
    at: now() - (opts.ago ?? 0),
    reverted_by: opts.reverted ? 'reverted' : null,
    revisions: Math.max(1, nodeIds.length),
  };
  changeSets.unshift(cs);
  touched.set(cs.id, nodeIds);
}

const live = (nid: string) => {
  const n = nodes.get(nid);
  if (!n || n.deleted_at) fail('not_found', `Eintrag „${nid}“ nicht gefunden`);
  return n;
};

const wikilinks = (body: string) => [...body.matchAll(/\[\[([^\]\n|#]+)/g)].map((m) => m[1].trim().toLowerCase());

function syncWikilinks(n: Node) {
  const wanted = new Set(
    wikilinks(n.body)
      .map((t) => [...nodes.values()].find((o) => !o.deleted_at && o.id !== n.id && o.title.toLowerCase() === t)?.id)
      .filter(Boolean) as string[],
  );
  for (const e of edges.values()) {
    if (e.src === n.id && e.origin === 'wikilink' && !e.deleted_at && !wanted.has(e.dst)) e.deleted_at = now();
  }
  for (const dst of wanted) {
    if (![...edges.values()].some((e) => e.src === n.id && e.dst === dst && !e.deleted_at)) addEdge(n.id, dst, 'mentions', 'wikilink');
  }
  // Backlinks from nodes that already mention this title.
  for (const o of nodes.values()) {
    if (o.id !== n.id && !o.deleted_at && wikilinks(o.body).includes(n.title.toLowerCase()) &&
      ![...edges.values()].some((e) => e.src === o.id && e.dst === n.id && !e.deleted_at)) {
      addEdge(o.id, n.id, 'mentions', 'wikilink');
    }
  }
}

function addEdge(src: string, dst: string, relation = 'related_to', origin: Edge['origin'] = 'manual'): Edge {
  const e: Edge = { id: id(), src, dst, relation, weight: 1, note: null, origin, created_at: now(), created_by: USER, deleted_at: null };
  edges.set(e.id, e);
  return e;
}

function createNode(input: Args, reviewed = true): Node {
  const title = String(input.title ?? '').trim();
  if (!title) fail('invalid', 'title must not be empty');
  const n: Node = {
    id: id(), title, body: input.body ?? '', kind: input.kind ?? 'note', props: null, reviewed, rev: 1,
    tags: input.tags ?? [], created_at: now(), updated_at: now(), created_by: USER, updated_by: USER, deleted_at: null,
  };
  nodes.set(n.id, n);
  for (const p of input.projects ?? []) members.add(`${p}|${n.id}`);
  syncWikilinks(n);
  return n;
}

function createProject(name: string, parent: string | null = null, color: string | null = null): Project {
  const p: Project = { id: id(), parent_id: parent, name, description: '', color, icon: null, sort: projects.size, created_at: now(), created_by: USER, deleted_at: null };
  projects.set(p.id, p);
  return p;
}

function subtree(root: string): Set<string> {
  const out = new Set([root]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const p of projects.values()) if (p.parent_id && out.has(p.parent_id) && !out.has(p.id)) (out.add(p.id), (grew = true));
  }
  return out;
}

const projectsOf = (nid: string) => [...projects.values()].filter((p) => members.has(`${p.id}|${nid}`));

function projectList(): ProjectSummary[] {
  const all = [...projects.values()].filter((p) => !p.deleted_at);
  const out: ProjectSummary[] = [];
  const visit = (parent: string | null) => {
    for (const p of all.filter((x) => x.parent_id === parent).sort((a, b) => a.sort - b.sort)) {
      out.push({ project: p, node_count: [...nodes.values()].filter((n) => !n.deleted_at && members.has(`${p.id}|${n.id}`)).length });
      visit(p.id);
    }
  };
  visit(null);
  return out;
}

function graph(project: string | null): GraphSnapshot {
  const view = project ?? 'all';
  const scope = project ? subtree(project) : null;
  const ns = [...nodes.values()].filter((n) => !n.deleted_at && (!scope || projectsOf(n.id).some((p) => scope.has(p.id))));
  const ids = new Set(ns.map((n) => n.id));
  const es = [...edges.values()].filter((e) => !e.deleted_at && ids.has(e.src) && ids.has(e.dst));
  const degree = new Map<string, number>();
  es.forEach((e) => [e.src, e.dst].forEach((x) => degree.set(x, (degree.get(x) ?? 0) + 1)));
  return {
    view,
    nodes: ns.map((n) => {
      const l = layouts.get(`${view}|${n.id}`);
      return {
        id: n.id, title: n.title, kind: n.kind, reviewed: n.reviewed, updated_at: n.updated_at, created_by: n.created_by,
        projects: projectsOf(n.id).map((p) => p.id), degree: degree.get(n.id) ?? 0, x: l?.x ?? null, y: l?.y ?? null, pinned: l?.pinned ?? false,
      };
    }),
    edges: es.map((e) => ({ id: e.id, src: e.src, dst: e.dst, relation: e.relation, origin: e.origin })),
  };
}

function detail(nid: string): NodeDetail {
  const node = live(nid);
  const brief = (x: string) => ({ id: x, title: nodes.get(x)!.title, kind: nodes.get(x)!.kind });
  const active = [...edges.values()].filter((e) => !e.deleted_at && !nodes.get(e.src)!.deleted_at && !nodes.get(e.dst)!.deleted_at);
  return {
    node,
    projects: projectsOf(nid).map((p) => ({ id: p.id, name: p.name })),
    outgoing: active.filter((e) => e.src === nid).map((e) => ({ edge: e, other: brief(e.dst) })),
    incoming: active.filter((e) => e.dst === nid).map((e) => ({ edge: e, other: brief(e.src) })),
  };
}

function search(q: Args): SearchHit[] {
  const terms = String(q.text ?? '').toLowerCase().split(/[^\p{L}\p{N}]+/u).filter(Boolean);
  const scope = q.project ? subtree(q.project) : null;
  return [...nodes.values()]
    .filter((n) => !n.deleted_at && (!scope || projectsOf(n.id).some((p) => scope.has(p.id))))
    .map((n) => {
      const hay = `${n.title} ${n.body}`.toLowerCase();
      const score = terms.reduce((s, t) => s + (n.title.toLowerCase().includes(t) ? 5 : hay.includes(t) ? 1 : -100), 0);
      const t = terms[0];
      const at = t ? n.body.toLowerCase().indexOf(t) : -1;
      const snippet = at >= 0 ? `…${n.body.slice(Math.max(0, at - 30), at)}**${n.body.slice(at, at + t.length)}**${n.body.slice(at + t.length, at + 60)}…` : n.body.slice(0, 100);
      return { node: { id: n.id, title: n.title, kind: n.kind }, snippet, score: terms.length ? score : n.updated_at };
    })
    .filter((h) => !terms.length || h.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, q.limit ?? 20);
}

function seed() {
  const bm = createProject('Brainmatter', null, '#7c9cff');
  const core = createProject('Kern', bm.id, '#5fd3a5');
  const ui = createProject('UI', bm.id, '#4cc9f0');
  const life = createProject('Privat', null, '#ffa24c');
  const n = (title: string, body: string, ps: Project[], kind = 'note', reviewed = true) =>
    createNode({ title, body, kind, projects: ps.map((p) => p.id), tags: kind === 'concept' ? ['konzept'] : [] }, reviewed);
  n('Brainmatter', 'Lokaler Wissensgraph mit [[MCP]]-Hub. Kern in Rust: [[SQLCipher]], [[Tokio]]. UI mit [[Svelte 5]] und [[Sigma.js]].', [bm], 'concept');
  n('SQLCipher', 'Verschlüsselt jede Seite der SQLite-Datenbank mit AES-256. Auf macOS über **CommonCrypto**.', [core], 'concept');
  n('Argon2id', 'Speicherintensive Passwort-KDF. Wir nutzen 128 MiB, t=3, p=4 (~0,5 s).', [core], 'concept');
  n('Tresor', 'Der DEK wird pro Slot gewrappt: Passwort ([[Argon2id]]), Recovery-Key, [[Touch ID]].', [core]);
  n('Touch ID', 'LocalAuthentication + Keychain ohne UI. Nach Updates einmal Passwort.', [core]);
  n('Tokio', 'Asynchrone Laufzeit für Rust mit Work-Stealing-Scheduler.', [core], 'concept');
  n('MCP', 'Model Context Protocol: KIs rufen Tools von Servern auf. Wir nutzen rmcp über Streamable HTTP.', [bm], 'concept');
  n('Svelte 5', 'Runes: `$state`, `$derived`, `$effect`.', [ui], 'concept');
  n('Sigma.js', 'WebGL-Graph-Rendering, 50k Knoten bei 60 fps in WKWebView.', [ui], 'source');
  n('Command-Palette', '⌘K öffnet Suche und Aktionen. Verlinkt [[Svelte 5]].', [ui], 'task');
  n('Graph-Layout', 'ForceAtlas2 im Worker, Positionen pro Ansicht gespeichert. Siehe [[Sigma.js]].', [ui]);
  n('Hybrid-Suche', 'BM25 + Vektoren mit RRF. Kommt in Phase 4. Ergänzt [[SQLCipher]] FTS5.', [core], 'decision', false);
  n('Sauerteig', 'Anstellgut 1:5:5 füttern, 12 h Gare bei 24 °C.', [life]);
  n('Roggenbrot', 'Basiert auf [[Sauerteig]]. 70 % Roggen, 30 % Dinkel.', [life]);
  n('Marathon-Plan', '16 Wochen, lange Läufe sonntags, Intervalle dienstags.', [life], 'task');
  const ids = [...nodes.values()];
  addEdge(ids[3].id, ids[1].id, 'depends_on');
  addEdge(ids[6].id, ids[5].id, 'depends_on', 'ai');
  const H = 3_600_000;
  const byTitle = (t: string) => ids.find((x) => x.title === t)!.id;
  clients.push({ id: id(), name: 'Claude Code', scopes: ['read', 'write'], projects: [], created_at: now() - 72 * H, last_seen_at: now() - 6 * 60_000 });
  clients.push({ id: id(), name: 'Cursor', scopes: ['read'], projects: [core.id], created_at: now() - 50 * H, last_seen_at: null });
  record('Demo-Daten angelegt', [], { ago: 72 * H });
  record('„Marathon-Plan“ erstellt', [byTitle('Marathon-Plan')], { ago: 50 * H });
  record('5 Einträge, 4 Verbindungen erfasst', ['Brainmatter', 'SQLCipher', 'Argon2id', 'Tresor', 'Touch ID'].map(byTitle),
    { actor: 'Claude Code', client: 'claude-code', ago: 27 * H });
  record('„Roggenbrot“ bearbeitet', [byTitle('Roggenbrot')], { ago: 26 * H, reverted: true });
  record('Projekt „Privat“ erstellt', [], { ago: 5 * H });
  record('„Hybrid-Suche“ erstellt', [byTitle('Hybrid-Suche')], { actor: 'Cursor', client: 'cursor', ago: 3 * H });
  record('„Sauerteig“ gelöscht', [byTitle('Sauerteig')], { actor: 'Codex', client: 'codex', ago: 2 * H, reverted: true });
  record('Verbindung erstellt', [], { actor: 'Claude Code', client: 'claude-code', ago: 40 * 60_000 });
  record('„Tresor“ bearbeitet', [byTitle('Tresor')], { actor: 'Claude Code', client: 'claude-code', ago: 6 * 60_000 });
}

const handlers: Record<string, (a: Args) => unknown> = {
  mcp_info: () => ({
    url: 'http://127.0.0.1:47474/mcp',
    running: true,
    error: null,
    bridge_path: '/Applications/Brainmatter.app/Contents/MacOS/brainmatter-mcp',
    bridge_exists: true,
    clients,
  }),
  create_client: (a) => {
    if (!String(a.name).trim()) fail('invalid', 'Bitte einen Namen angeben.');
    if (clients.some((c) => c.name.toLowerCase() === a.name.trim().toLowerCase())) fail('invalid', `Es gibt schon eine Verbindung namens „${a.name}“.`);
    const client: AiClient = { id: id(), name: a.name.trim(), scopes: a.scopes, projects: a.projects, created_at: now(), last_seen_at: null };
    clients.push(client);
    return { client, token: 'bm_mock_Zm9vYmFyYmF6cXV4MTIzNDU2Nzg5MGFiY2RlZg' };
  },
  revoke_client: (a) => void clients.splice(clients.findIndex((c) => c.id === a.id), 1),
  session_status: () => status(),
  create_vault: (a) => {
    if (String(a.password).length < 8) fail('invalid', 'Das Passwort braucht mindestens 8 Zeichen.');
    userName = a.name;
    state = 'unlocked';
    emit('bm:session', status());
    return '7K3Q-M9XA-2PZD-W4TB-8HNR-C6FE-1VJS-QY5G-0KAM';
  },
  unlock_password: (a) => {
    if (a.password !== 'demo1234') fail('wrong_password', 'Falsches Passwort. (Mock: demo1234)');
    state = 'unlocked';
    emit('bm:session', status());
  },
  unlock_recovery: () => ((state = 'unlocked'), emit('bm:session', status())),
  // Behaves as if the user dismissed the prompt, unless `?touchid=ok`.
  unlock_touch_id: () => {
    if (new URLSearchParams(location.search).get('touchid') !== 'ok') fail('touch_id_cancelled', 'Touch ID abgebrochen.');
    state = 'unlocked';
    emit('bm:session', status());
  },
  enroll_touch_id: () => void (touchId = true),
  disable_touch_id: () => void (touchId = false),
  change_password: (a) => {
    if (a.current !== 'demo1234') fail('wrong_password', 'Falsches Passwort.');
  },
  rotate_recovery_key: (a) => {
    if (a.current !== 'demo1234') fail('wrong_password', 'Falsches Passwort.');
    return 'N3W4-K3Y5-ABCD-EFGH-JKMN-PQRS-TVWX-YZ01-2345';
  },
  lock: () => ((state = 'locked'), emit('bm:session', status())),
  get_settings: () => settings,
  update_settings: (a) => (settings = { ...a.settings, auto_lock_minutes: Math.max(1, a.settings.auto_lock_minutes) }),
  touch: () => undefined,

  projects: () => projectList(),
  create_project: (a) => {
    const p = createProject(a.input.name, a.input.parent ?? null);
    record(`Projekt „${p.name}“ erstellt`);
    notify({ type: 'project_upserted', id: p.id });
    return p;
  },
  add_to_project: (a) => {
    a.nodes.forEach((n: string) => members.add(`${a.project}|${n}`));
    notify({ type: 'membership_changed', project: a.project, node: a.nodes[0], member: true });
    return a.nodes.length;
  },
  remove_from_project: (a) => {
    a.nodes.forEach((n: string) => members.delete(`${a.project}|${n}`));
    notify({ type: 'membership_changed', project: a.project, node: a.nodes[0], member: false });
    return a.nodes.length;
  },
  graph: (a) => graph(a.project ?? null),
  save_layout: (a) => {
    for (const p of a.positions) layouts.set(`${a.view}|${p.node}`, p);
  },
  node_detail: (a) => detail(a.id),
  search: (a) => search(a.query),
  create_node: (a) => {
    const n = createNode(a.input);
    record(`„${n.title}“ erstellt`, [n.id]);
    notify({ type: 'node_upserted', id: n.id });
    return n;
  },
  update_node: (a) => {
    const n = live(a.id);
    const p = a.patch;
    if (p.title) n.title = p.title.trim();
    if (p.body) n.body = p.body.mode === 'replace' ? p.body.text : p.body.mode === 'append' ? `${n.body}\n\n${p.body.text}` : `${p.body.text}\n\n${n.body}`;
    if (p.kind) n.kind = p.kind;
    if (p.tags_add) n.tags = [...new Set([...n.tags, ...p.tags_add.map((t: string) => t.toLowerCase())])].sort();
    if (p.tags_remove) n.tags = n.tags.filter((t) => !p.tags_remove.includes(t));
    if (p.reviewed !== undefined) n.reviewed = p.reviewed;
    n.rev++;
    n.updated_at = now();
    syncWikilinks(n);
    record(`„${n.title}“ bearbeitet`, [n.id]);
    notify({ type: 'node_upserted', id: n.id });
    return n;
  },
  delete_node: (a) => {
    const n = live(a.id);
    n.deleted_at = now();
    for (const e of edges.values()) if (e.src === n.id || e.dst === n.id) e.deleted_at ??= now();
    record(`„${n.title}“ gelöscht`, [n.id]);
    notify({ type: 'node_deleted', id: n.id });
  },
  link: (a) => {
    live(a.input.src);
    live(a.input.dst);
    const e = addEdge(a.input.src, a.input.dst, a.input.relation ?? 'related_to');
    notify({ type: 'edge_upserted', id: e.id, src: e.src, dst: e.dst });
    return e;
  },
  unlink: (a) => {
    const found = [...edges.values()].filter((e) => !e.deleted_at && ((e.src === a.src && e.dst === a.dst) || (e.src === a.dst && e.dst === a.src)) && (!a.relation || e.relation === a.relation));
    found.forEach((e) => (e.deleted_at = now()));
    if (found[0]) notify({ type: 'edge_deleted', id: found[0].id, src: found[0].src, dst: found[0].dst });
    return found;
  },
  change_sets: (a) => changeSets.slice(0, a.limit ?? 50),
  recent_changes: (a) =>
    changeSets.slice(0, a.limit ?? 50).map((change_set) => ({
      change_set,
      nodes: (touched.get(change_set.id) ?? [])
        .map((nid) => nodes.get(nid))
        .filter((n): n is Node => !!n)
        .map((n) => ({ id: n.id, title: n.title, kind: n.kind })),
    })),
  node_history: (a) => {
    const n = live(a.id);
    return [{ change_set: changeSets.at(-1)!.id, actor_name: userName, at: n.created_at, rev: 1, before: null, after: n }];
  },
  revert: (a) => {
    const cs = changeSets.find((c) => c.id === a.changeSet);
    if (!cs) fail('not_found', 'Änderung nicht gefunden');
    cs.reverted_by = 'reverted';
    record('Änderung rückgängig gemacht');
    notify({ type: 'change_set_committed', id: cs.id, actor: USER, summary: cs.summary });
  },
};

export function install() {
  if (state === 'locked') seed();
  mockIPC(
    (cmd, payload) => {
      const handler = handlers[cmd];
      if (!handler) throw { code: 'internal', message: `Mock: unbekannter Befehl ${cmd}` };
      return handler((payload ?? {}) as Args);
    },
    { shouldMockEvents: true },
  );
  console.info('[brainmatter] Mock-Backend aktiv (Browser-Dev). Passwort: demo1234');
}
