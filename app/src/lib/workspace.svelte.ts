// Shared UI state of the unlocked workspace.
import {
  api,
  errorMessage,
  onDomainEvent,
  onResync,
  type DomainEvent,
  type GraphSnapshot,
  type Id,
  type NodeDetail,
  type ProjectSummary,
} from './api';

const PROJECT_PALETTE = [
  '#7c9cff', '#5fd3a5', '#ff8a7a', '#f5c451', '#c38bff',
  '#4cc9f0', '#ff6fb5', '#9bd65a', '#ffa24c', '#8f9bb3',
];

export type PaletteMode = 'open' | 'link';

class Workspace {
  projects = $state<ProjectSummary[]>([]);
  /** Project id, or null for the whole graph. */
  view = $state<Id | null>(null);
  graph = $state.raw<GraphSnapshot | null>(null);
  selected = $state<Id | null>(null);
  detail = $state<NodeDetail | null>(null);
  palette = $state<PaletteMode | null>(null);
  /** Right column when no entry is selected. */
  panel = $state<'activity' | null>(null);
  settingsOpen = $state(false);
  /** Bumped after every coalesced refresh, for views that reload on change. */
  revision = $state(0);
  /** Increment to ask the graph to centre the selected node. */
  focusTick = $state(0);
  toast = $state<string | null>(null);

  projectColor = $derived.by(() => {
    const map = new Map<Id, string>();
    this.projects.forEach((p, i) => map.set(p.project.id, p.project.color ?? PROJECT_PALETTE[i % PROJECT_PALETTE.length]));
    return map;
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
  private unlisten: (() => void)[] = [];

  async start() {
    this.unlisten.push(await onDomainEvent((e) => this.onEvent(e)));
    this.unlisten.push(await onResync(() => this.scheduleRefresh()));
    await Promise.all([this.loadProjects(), this.loadGraph()]);
  }

  stop() {
    this.unlisten.forEach((u) => u());
    this.unlisten = [];
    clearTimeout(this.refreshTimer);
  }

  async loadProjects() {
    this.projects = await api.projects();
  }

  async loadGraph() {
    this.graph = await api.graph(this.view);
  }

  async setView(view: Id | null) {
    this.view = view;
    await this.loadGraph();
  }

  async select(id: Id | null, focus = false) {
    this.selected = id;
    if (focus) this.focusTick++;
    await this.refreshDetail();
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

  private onEvent(_e: DomainEvent) {
    this.scheduleRefresh();
  }

  /** Coalesces bursts of events (e.g. an AI capturing 50 entries). Project
   * node counts change with almost every event, so everything reloads. */
  private scheduleRefresh() {
    clearTimeout(this.refreshTimer);
    this.refreshTimer = setTimeout(async () => {
      await Promise.all([this.loadProjects(), this.loadGraph(), this.refreshDetail()]);
      this.revision++;
    }, 120);
  }
}

export const ws = new Workspace();
