// Typed bridge to the Rust commands (app/src-tauri/src/commands.rs).
// Field names mirror the serde output of bm-core (snake_case).
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type Id = string;

export interface Node {
  id: Id;
  title: string;
  body: string;
  kind: string;
  props: unknown | null;
  reviewed: boolean;
  rev: number;
  tags: string[];
  created_at: number;
  updated_at: number;
  created_by: Id;
  updated_by: Id;
  deleted_at: number | null;
}

export type EdgeOrigin = 'manual' | 'wikilink' | 'ai' | 'suggestion';

export interface Edge {
  id: Id;
  src: Id;
  dst: Id;
  relation: string;
  weight: number;
  note: string | null;
  origin: EdgeOrigin;
  created_at: number;
  created_by: Id;
  deleted_at: number | null;
}

export interface Project {
  id: Id;
  parent_id: Id | null;
  name: string;
  description: string;
  color: string | null;
  icon: string | null;
  sort: number;
  created_at: number;
  created_by: Id;
  deleted_at: number | null;
}

export interface ProjectSummary {
  project: Project;
  node_count: number;
}

export interface NodeBrief {
  id: Id;
  title: string;
  kind: string;
}

export interface EdgeView {
  edge: Edge;
  other: NodeBrief;
}

export interface NodeDetail {
  node: Node;
  projects: { id: Id; name: string }[];
  outgoing: EdgeView[];
  incoming: EdgeView[];
}

export interface GraphNode {
  id: Id;
  title: string;
  kind: string;
  reviewed: boolean;
  updated_at: number;
  created_by: Id;
  projects: Id[];
  degree: number;
  x: number | null;
  y: number | null;
  pinned: boolean;
}

export interface GraphEdge {
  id: Id;
  src: Id;
  dst: Id;
  relation: string;
  origin: EdgeOrigin;
}

export interface GraphSnapshot {
  view: string;
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface SearchHit {
  node: NodeBrief;
  snippet: string;
  score: number;
}

export interface ChangeSetInfo {
  id: Id;
  actor: Id;
  actor_name: string;
  client: string | null;
  tool: string | null;
  summary: string;
  at: number;
  reverted_by: Id | null;
  revisions: number;
}

export interface RecentChange {
  change_set: ChangeSetInfo;
  nodes: NodeBrief[];
}

export interface NodeRevision {
  change_set: Id;
  actor_name: string;
  at: number;
  rev: number;
  before: Node | null;
  after: Node | null;
}

export interface Settings {
  auto_lock_minutes: number;
  lock_on_sleep: boolean;
  lock_on_screen_lock: boolean;
}

export interface Status {
  state: 'none' | 'locked' | 'unlocked';
  touch_id_enrolled: boolean;
  biometry_available: boolean;
  user_name: string | null;
  settings: Settings;
}

export interface ApiError {
  code: string;
  message: string;
  candidates?: NodeBrief[];
}

export type Scope = 'read' | 'write' | 'delete';

export interface AiClient {
  id: Id;
  name: string;
  scopes: Scope[];
  projects: Id[];
  created_at: number;
  last_seen_at: number | null;
}

export interface McpInfo {
  url: string;
  running: boolean;
  error: string | null;
  bridge_path: string;
  bridge_exists: boolean;
  clients: AiClient[];
}

export type BodyEdit = { mode: 'replace' | 'append' | 'prepend'; text: string };

export interface NewNode {
  title: string;
  body?: string;
  kind?: string;
  tags?: string[];
  projects?: Id[];
}

export interface NodePatch {
  title?: string;
  body?: BodyEdit;
  kind?: string;
  tags_add?: string[];
  tags_remove?: string[];
  reviewed?: boolean;
}

export interface SearchOptions {
  project?: Id | null;
  include_subprojects?: boolean;
  kinds?: string[];
  tags?: string[];
  limit?: number;
}

export type DomainEvent =
  | { type: 'node_upserted'; id: Id }
  | { type: 'node_deleted'; id: Id }
  | { type: 'edge_upserted'; id: Id; src: Id; dst: Id }
  | { type: 'edge_deleted'; id: Id; src: Id; dst: Id }
  | { type: 'project_upserted'; id: Id }
  | { type: 'project_deleted'; id: Id }
  | { type: 'membership_changed'; project: Id; node: Id; member: boolean }
  | { type: 'change_set_committed'; id: Id; actor: Id; summary: string };

export function isApiError(e: unknown): e is ApiError {
  return typeof e === 'object' && e !== null && 'code' in e && 'message' in e;
}

export function errorMessage(e: unknown): string {
  return isApiError(e) ? e.message : String(e);
}

const call = <T>(cmd: string, args?: Record<string, unknown>) => invoke<T>(cmd, args);

export const api = {
  // session
  status: () => call<Status>('session_status'),
  createVault: (password: string, name: string) => call<string>('create_vault', { password, name }),
  unlockPassword: (password: string) => call<void>('unlock_password', { password }),
  unlockRecovery: (key: string) => call<void>('unlock_recovery', { key }),
  unlockTouchId: () => call<void>('unlock_touch_id'),
  enrollTouchId: () => call<void>('enroll_touch_id'),
  disableTouchId: () => call<void>('disable_touch_id'),
  changePassword: (current: string, password: string) => call<void>('change_password', { current, password }),
  rotateRecoveryKey: (current: string) => call<string>('rotate_recovery_key', { current }),
  lock: () => call<void>('lock'),
  getSettings: () => call<Settings>('get_settings'),
  updateSettings: (settings: Settings) => call<Settings>('update_settings', { settings }),
  touch: () => call<void>('touch'),

  // AI clients
  mcpInfo: () => call<McpInfo>('mcp_info'),
  createClient: (name: string, scopes: Scope[], projects: Id[]) =>
    call<{ client: AiClient; token: string }>('create_client', { name, scopes, projects }),
  revokeClient: (id: Id) => call<void>('revoke_client', { id }),

  // data
  projects: () => call<ProjectSummary[]>('projects'),
  createProject: (name: string, parent: Id | null = null) =>
    call<Project>('create_project', { input: { name, parent } }),
  addToProject: (project: Id, nodes: Id[]) => call<number>('add_to_project', { project, nodes }),
  removeFromProject: (project: Id, nodes: Id[]) => call<number>('remove_from_project', { project, nodes }),
  graph: (project: Id | null) => call<GraphSnapshot>('graph', { project }),
  saveLayout: (view: string, positions: { node: Id; x: number; y: number; pinned: boolean }[]) =>
    call<void>('save_layout', { view, positions }),
  nodeDetail: (id: Id) => call<NodeDetail>('node_detail', { id }),
  search: (text: string, opts: SearchOptions = {}) =>
    call<SearchHit[]>('search', {
      query: {
        text,
        project: opts.project ?? null,
        include_subprojects: opts.include_subprojects ?? true,
        kinds: opts.kinds ?? [],
        tags: opts.tags ?? [],
        limit: opts.limit ?? 20,
      },
    }),
  createNode: (input: NewNode) => call<Node>('create_node', { input }),
  updateNode: (id: Id, patch: NodePatch) => call<Node>('update_node', { id, patch }),
  deleteNode: (id: Id) => call<void>('delete_node', { id }),
  link: (src: Id, dst: Id, relation?: string) =>
    call<Edge>('link', { input: { src, dst, relation: relation ?? null } }),
  unlink: (src: Id, dst: Id, relation?: string) =>
    call<Edge[]>('unlink', { src, dst, relation: relation ?? null }),
  changeSets: (limit = 50) => call<ChangeSetInfo[]>('change_sets', { limit }),
  recentChanges: (limit = 100) => call<RecentChange[]>('recent_changes', { limit }),
  nodeHistory: (id: Id) => call<NodeRevision[]>('node_history', { id }),
  revert: (changeSet: Id) => call<unknown>('revert', { changeSet }),
};

export function onDomainEvent(cb: (e: DomainEvent) => void): Promise<UnlistenFn> {
  return listen<DomainEvent>('bm:event', (e) => cb(e.payload));
}

export function onSession(cb: (s: Status) => void): Promise<UnlistenFn> {
  return listen<Status>('bm:session', (e) => cb(e.payload));
}

export function onResync(cb: () => void): Promise<UnlistenFn> {
  return listen('bm:resync', () => cb());
}
