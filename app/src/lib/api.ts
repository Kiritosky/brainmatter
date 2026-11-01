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
  attachments: Attachment[];
  file_links: Omit<FileLinkStatus, 'exists' | 'is_dir'>[];
  /** A documentation page that mirrors a file and can only be changed there. */
  read_only: boolean;
}

export interface TagCount {
  tag: string;
  count: number;
}

export interface Actor {
  id: Id;
  kind: 'user' | 'ai';
  name: string;
  color: string | null;
  created_at: number;
}

export interface ProjectDeletePreview {
  subprojects: number;
  entries_only_here: number;
  entries_elsewhere: number;
  documents: number;
}

export interface DocPage {
  node: NodeBrief;
  parent: Id | null;
  depth: number;
  read_only: boolean;
  source_path: string | null;
  reviewed: boolean;
  updated_at: number;
  tags: string[];
}

export interface DocSource {
  id: Id;
  project_id: Id;
  path: string;
  created_at: number;
  pages: number;
}

export interface DocSyncStatus {
  changed: number;
  skipped: number;
  unreachable: string[];
}

export interface FileLinkStatus {
  id: Id;
  node_id: Id;
  path: string;
  label: string;
  approved_for_ai: boolean;
  exists: boolean;
  is_dir: boolean;
}

export const TEMPLATE_KIND = 'template';

export interface TemplateInfo {
  node: NodeBrief;
  slug: string;
  body: string;
  arguments: string[];
  projects: { id: Id; name: string }[];
  reviewed: boolean;
}

export interface TrashedNode {
  node: NodeBrief;
  deleted_at: number;
}

export interface Attachment {
  id: Id;
  node_id: Id;
  filename: string;
  mime: string;
  size: number;
  created_at: number;
  created_by: Id;
}

export interface GraphNode {
  id: Id;
  title: string;
  kind: string;
  reviewed: boolean;
  updated_at: number;
  created_by: Id;
  projects: Id[];
  tags: string[];
  degree: number;
  /** PageRank within the view; sums to about 1. */
  rank: number;
  /** Topic cluster within the view; 0 is the largest. */
  cluster: number;
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
  /** Found by meaning, not only by words. */
  semantic: boolean;
}

export interface Similar {
  node: NodeBrief;
  score: number;
  likely_duplicate: boolean;
}

export interface ModelStatus {
  state: 'missing' | 'downloading' | 'ready';
  downloaded: number;
  size: number;
  error: string | null;
  indexed: number;
  total: number;
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
  auto_backup: boolean;
  theme: 'system' | 'light' | 'dark';
  quick_capture: boolean;
  auto_update: boolean;
}

export interface AvailableUpdate {
  version: string;
  notes: string | null;
}

export interface BackupInfo {
  name: string;
  at: number;
  size: number;
}

export interface BackupStatus {
  running: boolean;
  error: string | null;
  backups: BackupInfo[];
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

export type IntegrationTarget = 'claude_code' | 'claude_desktop' | 'cursor' | 'codex';

export interface Integration {
  target: IntegrationTarget;
  name: string;
  installed: boolean;
  configured: boolean;
  available: boolean;
  unavailable_reason: string | null;
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
  | { type: 'change_set_committed'; id: Id; actor: Id; summary: string }
  | { type: 'index_updated'; indexed: number; total: number };

/** When something was created: UUIDv7 ids start with the time in ms. */
export function createdAt(id: Id): number {
  return parseInt(id.slice(0, 8) + id.slice(9, 13), 16);
}

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

  appVersion: () => call<string>('app_version'),
  checkUpdate: () => call<AvailableUpdate | null>('check_update'),
  installUpdate: () => call<void>('install_update'),
  hideCapture: () => call<void>('hide_capture'),
  showMainWindow: () => call<void>('show_main_window'),

  // backups
  backupStatus: () => call<BackupStatus>('backup_status'),
  backupNow: () => call<BackupInfo>('backup_now'),
  restoreBackup: (name: string, password: string) => call<void>('restore_backup', { name, password }),

  // AI clients
  mcpInfo: () => call<McpInfo>('mcp_info'),
  createClient: (name: string, scopes: Scope[], projects: Id[]) =>
    call<{ client: AiClient; token: string }>('create_client', { name, scopes, projects }),
  revokeClient: (id: Id) => call<void>('revoke_client', { id }),
  integrations: () => call<Integration[]>('integrations'),
  connectIntegration: (target: IntegrationTarget, scopes: Scope[], projects: Id[]) =>
    call<AiClient>('connect_integration', { target, scopes, projects }),

  // data
  projects: () => call<ProjectSummary[]>('projects'),
  createProject: (name: string, parent: Id | null = null) =>
    call<Project>('create_project', { input: { name, parent } }),
  renameProject: (id: Id, name: string) => call<Project>('rename_project', { id, name }),
  moveProject: (id: Id, parent: Id | null, position: number) => call<void>('move_project', { id, parent, position }),
  projectDeletePreview: (id: Id) => call<ProjectDeletePreview>('project_delete_preview', { id }),
  deleteProject: (id: Id, withEntries: boolean) => call<number>('delete_project', { id, withEntries }),
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
  modelStatus: () => call<ModelStatus>('model_status'),
  downloadModel: () => call<void>('download_model'),
  removeModel: () => call<void>('remove_model'),
  linkSuggestions: (id: Id) => call<Similar[]>('link_suggestions', { id }),
  dismissSuggestion: (a: Id, b: Id) => call<void>('dismiss_suggestion', { a, b }),
  acceptSuggestion: (src: Id, dst: Id) => call<Edge>('accept_suggestion', { src, dst }),
  mergeNodes: (keep: Id, others: Id[]) => call<Node>('merge_nodes', { keep, others }),
  orphans: (limit = 200) => call<NodeBrief[]>('orphans', { limit }),
  unreviewed: (limit = 200) => call<NodeBrief[]>('unreviewed', { limit }),
  addAttachment: async (node: Id, file: File) => {
    // See parse_attachment in commands.rs.
    const meta = new TextEncoder().encode(JSON.stringify({ node, filename: file.name, mime: file.type || null }));
    const body = new Uint8Array(4 + meta.length + file.size);
    new DataView(body.buffer).setUint32(0, meta.length, true);
    body.set(meta, 4);
    body.set(new Uint8Array(await file.arrayBuffer()), 4 + meta.length);
    return invoke<Attachment>('add_attachment', body);
  },
  attachmentData: (id: Id) => call<ArrayBuffer>('attachment_data', { id }),
  deleteAttachment: (id: Id) => call<void>('delete_attachment', { id }),
  exportAttachment: (id: Id) => call<boolean>('export_attachment', { id }),
  createNode: (input: NewNode) => call<Node>('create_node', { input }),
  updateNode: (id: Id, patch: NodePatch) => call<Node>('update_node', { id, patch }),
  deleteNode: (id: Id) => call<void>('delete_node', { id }),
  favorites: () => call<NodeBrief[]>('favorites'),
  setFavorite: (id: Id, favorite: boolean) => call<void>('set_favorite', { id, favorite }),
  tagCounts: () => call<TagCount[]>('tag_counts'),
  actors: () => call<Actor[]>('actors'),
  docTree: (project: Id) => call<DocPage[]>('doc_tree', { project }),
  createDocPage: (project: Id, title: string, parent: Id | null = null) => call<Node>('create_doc_page', { project, title, parent }),
  placeDocPage: (id: Id, parent: Id | null, position: number) => call<void>('place_doc_page', { id, parent, position }),
  docSources: (project: Id) => call<DocSource[]>('doc_sources', { project }),
  addDocSource: (project: Id, folder: boolean) => call<unknown>('add_doc_source', { project, folder }),
  syncDocSources: (project: Id) => call<DocSyncStatus>('sync_doc_sources', { project }),
  removeDocSource: (id: Id) => call<void>('remove_doc_source', { id }),
  fileLinks: (node: Id) => call<FileLinkStatus[]>('file_links', { node }),
  linkFiles: (node: Id, folders: boolean) => call<number>('link_files', { node, folders }),
  setFileLinkApproved: (id: Id, approved: boolean) => call<void>('set_file_link_approved', { id, approved }),
  deleteFileLink: (id: Id) => call<void>('delete_file_link', { id }),
  revealFile: (id: Id) => call<void>('reveal_file', { id }),
  openFile: (id: Id) => call<void>('open_file', { id }),
  templates: () => call<TemplateInfo[]>('templates'),
  trash: (limit = 500) => call<TrashedNode[]>('trash', { limit }),
  restoreNode: (id: Id) => call<Node>('restore_node', { id }),
  purgeNode: (id: Id) => call<void>('purge_node', { id }),
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

export function onModel(cb: (m: ModelStatus) => void): Promise<UnlistenFn> {
  return listen<ModelStatus>('bm:model', (e) => cb(e.payload));
}

export function onUpdateAvailable(cb: (u: AvailableUpdate) => void): Promise<UnlistenFn> {
  return listen<AvailableUpdate>('bm:update-available', (e) => cb(e.payload));
}

export function onUpdateProgress(cb: (p: { downloaded: number; total: number | null }) => void): Promise<UnlistenFn> {
  return listen<{ downloaded: number; total: number | null }>('bm:update-progress', (e) => cb(e.payload));
}

export function onCaptureOpen(cb: () => void): Promise<UnlistenFn> {
  return listen('bm:capture-open', () => cb());
}

export function onResync(cb: () => void): Promise<UnlistenFn> {
  return listen('bm:resync', () => cb());
}
