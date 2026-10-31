// Shared UI state of the unlocked workspace.
import {
  api,
  errorMessage,
  onDomainEvent,
  onModel,
  onResync,
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
/** Embedding runs in small batches; suggestions need not follow each one. */
const INDEX_REFRESH_MS = 1500;

export type PaletteMode = 'open' | 'link';
export type ColorMode = 'project' | 'kind' | 'theme';

class Workspace {
  projects = $state<ProjectSummary[]>([]);
  /** Project id, or null for the whole graph. */
  view = $state<Id | null>(null);
  graph = $state.raw<GraphSnapshot | null>(null);
  selected = $state<Id | null>(null);
  detail = $state<NodeDetail | null>(null);
  palette = $state<PaletteMode | null>(null);
  /** Right column when no entry is selected. */
  panel = $state<'activity' | 'review' | 'trash' | null>(null);
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

  private refreshTimer: ReturnType<typeof setTimeout> | undefined;
  private indexTimer: ReturnType<typeof setTimeout> | undefined;
  private unlisten: (() => void)[] = [];

  async start() {
    this.unlisten.push(await onDomainEvent((e) => this.onEvent(e)));
    this.unlisten.push(await onResync(() => this.scheduleRefresh()));
    this.unlisten.push(
      await onModel((m) => {
        this.model = m;
        this.refreshSuggestions();
      }),
    );
    await Promise.all([this.loadProjects(), this.loadGraph(), this.loadModel(), this.loadUnreviewed()]);
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

  private onEvent(e: DomainEvent) {
    if (e.type !== 'index_updated') {
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
      await Promise.all([
        this.loadProjects(),
        this.loadGraph(),
        this.loadUnreviewed(),
        this.refreshDetail(),
        this.refreshSuggestions(),
      ]);
      this.revision++;
    }, 120);
  }
}

export const ws = new Workspace();
