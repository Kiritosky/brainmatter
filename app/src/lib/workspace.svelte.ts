// Shared UI state of the unlocked workspace.
import {
  api,
  errorMessage,
  onDomainEvent,
  onModel,
  onResync,
  onUpdateAvailable,
  type Actor,
  type AvailableUpdate,
  type TagCount,
  type DomainEvent,
  type GraphNode,
  type GraphSnapshot,
  type Id,
  type ModelStatus,
  type NodeBrief,
  type NodeDetail,
  type ProjectSummary,
  type Similar,
} from './api';
import { isNewer, releasesSince, type Release } from './changelog';
import { prefs } from './prefs.svelte.ts';

const PROJECT_PALETTE = [
  '#7c9cff', '#5fd3a5', '#ff8a7a', '#f5c451', '#c38bff',
  '#4cc9f0', '#ff6fb5', '#9bd65a', '#ffa24c', '#8f9bb3',
];

export const KIND_COLORS: Record<string, string> = {
  note: '#8f9bb3',
  concept: '#5fd3a5',
  task: '#f5c451',
  person: '#ff6fb5',
  source: '#4cc9f0',
  decision: '#ffa24c',
  snippet: '#c38bff',
};

const NEUTRAL = '#8f9bb3';
const COLOR_MODE_KEY = 'bm.colorMode';
const HIDDEN_PROJECTS_KEY = 'bm.hiddenProjects';
const SAVED_VIEWS_KEY = 'bm.savedViews';

export interface SavedView {
  id: string;
  name: string;
  view: Id | null;
  colorMode: ColorMode;
  filter: { kinds: string[]; tags: string[]; actors: Id[] };
  hiddenProjects: Id[];
}
/** How long the note about an AI's last change stays up. */
const ACTIVITY_MS = 6000;
/** Matches the animation in GraphView. */
const PULSE_MS = 2600;
const SEEN_VERSION_KEY = 'bm.seenVersion';
/** Embedding runs in small batches; suggestions need not follow each one. */
const INDEX_REFRESH_MS = 1500;

export type PaletteMode = 'open' | 'link';
export type ColorMode = 'project' | 'kind' | 'theme';

/** Everything within `depth` links of `start`, in either direction. */
function reachable(graph: GraphSnapshot, start: Id, depth: number): Set<Id> {
  const neighbours = new Map<Id, Id[]>();
  const add = (a: Id, b: Id) => {
    const list = neighbours.get(a);
    if (list) list.push(b);
    else neighbours.set(a, [b]);
  };
  for (const e of graph.edges) {
    add(e.src, e.dst);
    add(e.dst, e.src);
  }
  const seen = new Set([start]);
  let frontier = [start];
  for (let i = 0; i < depth && frontier.length; i++) {
    const next: Id[] = [];
    for (const id of frontier) {
      for (const other of neighbours.get(id) ?? []) {
        if (!seen.has(other)) {
          seen.add(other);
          next.push(other);
        }
      }
    }
    frontier = next;
  }
  return seen;
}

class Workspace {
  projects = $state<ProjectSummary[]>([]);
  /** Project id, or null for the whole graph. */
  view = $state<Id | null>(null);
  /** What the middle shows. Documentation belongs to a project. */
  main = $state<'graph' | 'docs'>('graph');
  graph = $state.raw<GraphSnapshot | null>(null);
  selected = $state<Id | null>(null);
  detail = $state<NodeDetail | null>(null);
  palette = $state<PaletteMode | null>(null);
  /** Right column when no entry is selected. */
  panel = $state<'activity' | 'review' | 'trash' | 'library' | null>(null);
  settingsOpen = $state(false);
  /** Bumped after every coalesced refresh, for views that reload on change. */
  revision = $state(0);
  /** Increment to ask the graph to centre the selected node. */
  focusTick = $state(0);
  toast = $state<string | null>(null);
  colorMode = $state<ColorMode>((localStorage.getItem(COLOR_MODE_KEY) as ColorMode | null) ?? 'project');
  model = $state<ModelStatus | null>(null);
  /** Link suggestions for the selected entry. */
  suggestions = $state.raw<Similar[]>([]);
  unreviewed = $state.raw<NodeBrief[]>([]);
  /** A newer version, once a check has found one. */
  update = $state<AvailableUpdate | null>(null);
  favorites = $state.raw<NodeBrief[]>([]);
  tagCounts = $state.raw<TagCount[]>([]);
  actors = $state.raw<Actor[]>([]);
  /** What the graph shows. Empty lists mean everything. */
  filter = $state.raw<{ kinds: string[]; tags: string[]; actors: Id[] }>({ kinds: [], tags: [], actors: [] });
  /** Local graph: only what is this many links away from the selected entry. */
  neighbourhood = $state<number | null>(null);
  /** The latest change by an AI, while it is recent. Reads leave no trace, so only writes show. */
  aiActivity = $state<{ actor: Id; summary: string } | null>(null);
  aiActivityName = $derived(this.actors.find((a) => a.id === this.aiActivity?.actor)?.name ?? 'KI');
  /** Entries an AI just touched, for the graph to point out. */
  pulsing = $state.raw(new Set<Id>());
  /** Projects kept out of sight. Only a view: AIs and search still reach them. */
  hiddenProjects = $state.raw(new Set<Id>(JSON.parse(localStorage.getItem(HIDDEN_PROJECTS_KEY) ?? '[]')));

  /** Ignores leftovers of deleted projects. */
  hiddenProjectCount = $derived(this.projects.filter((p) => this.hiddenProjects.has(p.project.id)).length);

  /** Nodes the current filters hide. */
  hiddenNodes = $derived.by(() => {
    const hidden = new Set<Id>();
    const graph = this.graph;
    if (!graph) return hidden;
    const { kinds, tags, actors } = this.filter;
    // Opening a hidden project on purpose has to show what is in it.
    const opened = this.view && this.hiddenProjects.has(this.view) ? this.subtreeOf(this.view) : null;
    const concealed = (p: Id) => this.hiddenProjects.has(p) && !opened?.has(p);
    for (const n of graph.nodes) {
      const shown =
        (!kinds.length || kinds.includes(n.kind)) &&
        (!tags.length || n.tags.some((t) => tags.includes(t))) &&
        (!actors.length || actors.includes(n.created_by)) &&
        // An entry that is also in a visible project stays, as do those in none.
        !(n.projects.length && n.projects.every(concealed));
      if (!shown) hidden.add(n.id);
    }
    if (this.neighbourhood !== null && this.selected) {
      const near = reachable(graph, this.selected, this.neighbourhood);
      for (const n of graph.nodes) if (!near.has(n.id)) hidden.add(n.id);
    }
    return hidden;
  });

  activeFilters = $derived(
    this.filter.kinds.length + this.filter.tags.length + this.filter.actors.length + (this.neighbourhood !== null ? 1 : 0),
  );

  toggleFilter(key: 'kinds' | 'tags' | 'actors', value: string) {
    const list = this.filter[key];
    const next = list.includes(value) ? list.filter((v) => v !== value) : [...list, value];
    this.filter = { ...this.filter, [key]: next };
  }

  clearFilters() {
    this.filter = { kinds: [], tags: [], actors: [] };
    this.neighbourhood = null;
  }

  private setHiddenProjects(next: Set<Id>) {
    this.hiddenProjects = next;
    localStorage.setItem(HIDDEN_PROJECTS_KEY, JSON.stringify([...next]));
  }

  /** Subprojects go along either way; a hidden parent can still have a visible child. */
  toggleProjectHidden(id: Id) {
    const next = new Set(this.hiddenProjects);
    const hide = !next.has(id);
    for (const p of this.subtreeOf(id)) {
      if (hide) next.add(p);
      else next.delete(p);
    }
    this.setHiddenProjects(next);
  }

  /** Hides everything but this project and what is below it. */
  showOnlyProject(id: Id) {
    const keep = this.subtreeOf(id);
    this.setHiddenProjects(new Set(this.projects.map((p) => p.project.id).filter((p) => !keep.has(p))));
  }

  showAllProjects() {
    this.setHiddenProjects(new Set());
  }

  // Named combinations of what the graph shows. Per Mac, like hidden projects.
  savedViews = $state.raw<SavedView[]>(JSON.parse(localStorage.getItem(SAVED_VIEWS_KEY) ?? '[]'));

  private currentViewState(): Omit<SavedView, 'id' | 'name'> {
    return { view: this.view, colorMode: this.colorMode, filter: this.filter, hiddenProjects: [...this.hiddenProjects].sort() };
  }

  /** The saved view that shows exactly what is on screen now. */
  activeSavedView = $derived.by(() => {
    const now = JSON.stringify(this.currentViewState());
    return this.savedViews.find(({ id: _id, name: _name, ...state }) => JSON.stringify(state) === now)?.id ?? null;
  });

  private setSavedViews(next: SavedView[]) {
    this.savedViews = next;
    localStorage.setItem(SAVED_VIEWS_KEY, JSON.stringify(next));
  }

  saveCurrentView(name: string) {
    const entry: SavedView = { id: crypto.randomUUID(), name: name.trim(), ...this.currentViewState() };
    // Saving under a name in use replaces that view, as with templates.
    this.setSavedViews([...this.savedViews.filter((v) => v.name.toLowerCase() !== entry.name.toLowerCase()), entry]);
  }

  deleteSavedView(id: string) {
    this.setSavedViews(this.savedViews.filter((v) => v.id !== id));
  }

  async applySavedView(saved: SavedView) {
    this.setColorMode(saved.colorMode);
    this.filter = saved.filter;
    this.neighbourhood = null;
    this.setHiddenProjects(new Set(saved.hiddenProjects));
    // Its project may have been deleted since.
    const exists = saved.view && this.projects.some((p) => p.project.id === saved.view);
    await this.setView(exists ? saved.view : null);
  }

  async toggleFavorite(id: Id) {
    const favorite = !this.favorites.some((f) => f.id === id);
    try {
      await api.setFavorite(id, favorite);
      this.favorites = await api.favorites();
    } catch (e) {
      this.notify(errorMessage(e));
    }
  }

  async loadFilterData() {
    [this.favorites, this.tagCounts, this.actors] = await Promise.all([api.favorites(), api.tagCounts(), api.actors()]);
  }

  /** The changelog, while it is open. */
  changelog = $state.raw<{ heading: string; items: Release[] } | null>(null);

  projectColor = $derived.by(() => {
    const map = new Map<Id, string>();
    this.projects.forEach((p, i) => map.set(p.project.id, p.project.color ?? PROJECT_PALETTE[i % PROJECT_PALETTE.length]));
    return map;
  });

  /** Single entries are not a theme, so they stay neutral. */
  private clusterSizes = $derived.by(() => {
    const sizes = new Map<number, number>();
    this.graph?.nodes.forEach((n) => sizes.set(n.cluster, (sizes.get(n.cluster) ?? 0) + 1));
    return sizes;
  });

  currentProject = $derived(this.projects.find((p) => p.project.id === this.view)?.project ?? null);

  /** Ancestors of the current view, root first, for the breadcrumb. */
  breadcrumb = $derived.by(() => {
    const byId = new Map(this.projects.map((p) => [p.project.id, p.project]));
    const chain = [];
    let cur = this.view ? byId.get(this.view) : undefined;
    while (cur) {
      chain.unshift(cur);
      cur = cur.parent_id ? byId.get(cur.parent_id) : undefined;
    }
    return chain;
  });

  /** The project and everything below it. */
  subtreeOf(id: Id): Set<Id> {
    const out = new Set([id]);
    // Parents come before their children in the list, so one pass is enough.
    for (const { project } of this.projects) {
      if (project.parent_id && out.has(project.parent_id)) out.add(project.id);
    }
    return out;
  }

  private refreshTimer: ReturnType<typeof setTimeout> | undefined;
  private indexTimer: ReturnType<typeof setTimeout> | undefined;
  private activityTimer: ReturnType<typeof setTimeout> | undefined;
  private pulseTimer: ReturnType<typeof setTimeout> | undefined;
  /** Entries changed by the change set whose commit event is still to come. */
  private touched = new Set<Id>();
  /** Waiting for the refresh that puts them into the graph. */
  private toPulse = new Set<Id>();
  private unlisten: (() => void)[] = [];

  async start() {
    this.unlisten.push(await onDomainEvent((e) => this.onEvent(e)));
    this.unlisten.push(await onResync(() => this.scheduleRefresh()));
    this.unlisten.push(await onUpdateAvailable((u) => (this.update = u)));
    this.unlisten.push(
      await onModel((m) => {
        this.model = m;
        this.refreshSuggestions();
      }),
    );
    await Promise.all([this.loadProjects(), this.loadGraph(), this.loadModel(), this.loadUnreviewed(), this.loadFilterData()]);
    await this.showWhatsNew();
    // Here and not at launch: until the vault is unlocked, nothing could show it.
    const settings = await api.getSettings();
    if (settings.auto_update) api.checkUpdate().then((u) => (this.update = u)).catch(() => {});
  }

  /** Once after an update. A new vault has nothing to compare with, so it stays
   * quiet; existing data without a stored version was just updated from before
   * this existed. */
  private async showWhatsNew() {
    const current = await api.appVersion();
    const seen = localStorage.getItem(SEEN_VERSION_KEY);
    localStorage.setItem(SEEN_VERSION_KEY, current);
    const updated = seen ? isNewer(current, seen) : (this.graph?.nodes.length ?? 0) > 0;
    if (!updated) return;
    const items = releasesSince(seen, current);
    if (items.length && prefs.values.whatsNew) this.changelog = { heading: 'Was ist neu', items };
  }

  stop() {
    this.unlisten.forEach((u) => u());
    this.unlisten = [];
    clearTimeout(this.refreshTimer);
    clearTimeout(this.indexTimer);
  }

  async loadProjects() {
    this.projects = await api.projects();
  }

  async loadGraph() {
    this.graph = await api.graph(this.view);
  }

  async loadModel() {
    this.model = await api.modelStatus();
  }

  async loadUnreviewed() {
    this.unreviewed = await api.unreviewed();
  }

  async setView(view: Id | null) {
    this.view = view;
    if (!view) this.main = 'graph';
    await this.loadGraph();
  }

  async select(id: Id | null, focus = false) {
    this.selected = id;
    if (focus) this.focusTick++;
    await Promise.all([this.refreshDetail(), this.refreshSuggestions()]);
  }

  async refreshDetail() {
    if (!this.selected) {
      this.detail = null;
      return;
    }
    try {
      this.detail = await api.nodeDetail(this.selected);
    } catch {
      // Deleted meanwhile (e.g. by an AI): drop the selection.
      this.selected = null;
      this.detail = null;
    }
  }

  async refreshSuggestions() {
    const id = this.selected;
    if (!id || this.model?.state !== 'ready') {
      this.suggestions = [];
      return;
    }
    const found = await api.linkSuggestions(id).catch(() => []);
    // The selection may have moved on while this was running.
    if (this.selected === id) this.suggestions = found;
  }

  setColorMode(mode: ColorMode) {
    this.colorMode = mode;
    localStorage.setItem(COLOR_MODE_KEY, mode);
  }

  colorFor(n: GraphNode): string {
    if (this.colorMode === 'kind') return KIND_COLORS[n.kind] ?? NEUTRAL;
    if (this.colorMode === 'theme') {
      if ((this.clusterSizes.get(n.cluster) ?? 0) < 2) return NEUTRAL;
      return PROJECT_PALETTE[n.cluster % (PROJECT_PALETTE.length - 1)];
    }
    // Inside a project view, colour by subproject; otherwise by first project.
    const p = n.projects.find((id) => id !== this.view) ?? n.projects[0];
    return (p && this.projectColor.get(p)) || KIND_COLORS[n.kind] || NEUTRAL;
  }

  async createNode(title: string, body = '') {
    try {
      const node = await api.createNode({ title, body, projects: this.view ? [this.view] : [] });
      await this.select(node.id, true);
      return node;
    } catch (e) {
      this.notify(errorMessage(e));
    }
  }

  notify(message: string) {
    this.toast = message;
    setTimeout(() => {
      if (this.toast === message) this.toast = null;
    }, 4000);
  }

  /** A change set's commit event comes after the events of what it changed. */
  private noteChange(actor: Id, summary: string) {
    const touched = this.touched;
    this.touched = new Set();
    // An actor not known yet is an AI writing for the first time: the user always exists.
    if (this.actors.find((a) => a.id === actor)?.kind === 'user') return;
    this.aiActivity = { actor, summary };
    clearTimeout(this.activityTimer);
    this.activityTimer = setTimeout(() => (this.aiActivity = null), ACTIVITY_MS);
    touched.forEach((id) => this.toPulse.add(id));
  }

  private startPulse() {
    if (!this.toPulse.size) return;
    this.pulsing = this.toPulse;
    this.toPulse = new Set();
    clearTimeout(this.pulseTimer);
    this.pulseTimer = setTimeout(() => (this.pulsing = new Set()), PULSE_MS);
  }

  private onEvent(e: DomainEvent) {
    if (e.type !== 'index_updated') {
      if (e.type === 'node_upserted') this.touched.add(e.id);
      if (e.type === 'change_set_committed') this.noteChange(e.actor, e.summary);
      this.scheduleRefresh();
      return;
    }
    // New vectors change no entry, so the graph stays as it is.
    if (this.model) this.model = { ...this.model, indexed: e.indexed, total: e.total };
    clearTimeout(this.indexTimer);
    this.indexTimer = setTimeout(() => this.refreshSuggestions(), INDEX_REFRESH_MS);
  }

  /** Coalesces bursts of events (e.g. an AI capturing 50 entries). Project
   * node counts change with almost every event, so everything reloads. */
  private scheduleRefresh() {
    clearTimeout(this.refreshTimer);
    this.refreshTimer = setTimeout(async () => {
      // A change set's events arrive in one burst. Whatever is left over never got its
      // commit event, and pointing at the wrong entry is worse than at none.
      this.touched = new Set();
      await Promise.all([
        this.loadProjects(),
        this.loadGraph(),
        this.loadUnreviewed(),
        this.loadFilterData(),
        this.refreshDetail(),
        this.refreshSuggestions(),
      ]);
      this.startPulse();
      this.revision++;
    }, 120);
  }
}

export const ws = new Workspace();
